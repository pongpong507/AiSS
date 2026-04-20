//! InfoLit Web — 資訊判讀遊戲 Web 版
//!
//! Axum 後端 + 內嵌前端，一個 binary 就能跑。
//!
//! 用法：
//!   infolit-web --model qwen2.5:7b --ollama-url http://localhost:11434
//!   OLLAMA_URL=http://host:11434 MODEL=gemma4 infolit-web

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Json,
    },
    routing::{get, post},
    Router,
};
use clap::Parser;
use infolit_game::{
    actor::load_actors_from_dir,
    deception::load_deceptions_from_dir,
    pacing::PacingConfig,
    selector::assemble_session,
    session::GameSession,
    topic::load_topics_from_dir,
};
use llm_gateway::adapters::{OllamaProvider, ThinkingMode};
use llm_gateway::provider::LlmProvider;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

// ── CLI 參數 ─────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "infolit-web", about = "InfoLit 資訊判讀遊戲 Web 版")]
struct Args {
    /// 內容目錄（含 actors/, deception-patterns/, topics/）
    #[arg(long, env = "CONTENT_DIR", default_value = "./content")]
    content_dir: PathBuf,

    /// 監聽位址
    #[arg(long, env = "HOST", default_value = "0.0.0.0")]
    host: String,

    /// 監聽連接埠
    #[arg(long, env = "PORT", default_value_t = 3000)]
    port: u16,

    /// Ollama 服務位置
    #[arg(long, env = "OLLAMA_URL", default_value = "http://localhost:11434")]
    ollama_url: String,

    /// Ollama 模型名稱
    #[arg(long, env = "MODEL", default_value = "qwen2.5:7b")]
    model: String,

    /// 啟用 thinking 模式
    #[arg(long, env = "THINK")]
    think: bool,
}

// ── 應用狀態 ─────────────────────────────────────────────────────────────────

struct AppState {
    sessions: Mutex<HashMap<Uuid, GameSession>>,
    /// 玩家訊息緩衝區（獨立鎖，不與 session 競爭）
    pending_messages: Mutex<HashMap<Uuid, Vec<String>>>,
    actors: Vec<infolit_game::actor::Actor>,
    catalog: Vec<infolit_game::deception::DeceptionPattern>,
    topics: Vec<shared_types::Topic>,
    provider: Arc<dyn LlmProvider>,
    model: String,
}

// ── API 請求/回應型別 ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateGameReq {
    actors: Option<usize>,
    liars: Option<usize>,
}

#[derive(Serialize)]
struct GameCreated {
    session_id: String,
    topic: String,
    actors: Vec<ActorInfo>,
    liar_count: usize,
}

#[derive(Serialize)]
struct ActorInfo {
    index: usize,
    id: String,
    name: String,
    short_bio: String,
}

#[derive(Serialize)]
struct MessageInfo {
    speaker: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatReq {
    content: String,
}

#[derive(Deserialize)]
struct AccuseReq {
    actor_indices: Vec<usize>,
    reason: String,
}

#[derive(Serialize)]
struct AccuseResponse {
    verdict: String,
    feedback: String,
    correct_answer: String,
    liar_count: usize,
}

// ── 錯誤輔助 ────────────────────────────────────────────────────────────────

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg.into() })))
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<serde_json::Value>)>;

// ── Handlers ────────────────────────────────────────────────────────────────

async fn index_page() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn fallback(method: axum::http::Method, uri: axum::http::Uri) -> impl IntoResponse {
    warn!("404 未匹配: {} {}", method, uri);
    (StatusCode::NOT_FOUND, format!("404: {} {}", method, uri))
}

/// 建立新遊戲：隨機選題、組裝演員陣容
async fn create_game(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateGameReq>,
) -> ApiResult<GameCreated> {
    info!("POST /api/game — 建立新遊戲");
    let actor_count = req.actors.unwrap_or(3).min(5).max(2);
    let liar_count = req.liars.unwrap_or(1).min(actor_count - 1).max(1);

    let (selected, liar_ids, deceptions) =
        assemble_session(&state.actors, &state.catalog, actor_count, liar_count)
            .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?;

    let topic = state.topics[rand::random::<usize>() % state.topics.len()].clone();

    let pacing = PacingConfig {
        min_response_delay_ms: 0,
        max_response_delay_ms: 0,
        ..Default::default()
    };

    let session = GameSession::new(
        selected.clone(),
        liar_ids,
        deceptions,
        topic.clone(),
        pacing,
        state.provider.clone(),
    );

    let session_id = session.session_id;

    let actor_infos: Vec<ActorInfo> = selected
        .iter()
        .enumerate()
        .map(|(i, a)| ActorInfo {
            index: i + 1,
            id: a.id.clone(),
            name: a.name.clone(),
            short_bio: a.short_bio.clone(),
        })
        .collect();

    state.sessions.lock().await.insert(session_id, session);

    Ok(Json(GameCreated {
        session_id: session_id.to_string(),
        topic: topic.question,
        actors: actor_infos,
        liar_count: liar_count,
    }))
}

/// SSE 回應型別別名
type SseStream = Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>;

/// 開場發言：每位演員各說一輪（SSE 逐條推送）
async fn opening_round(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> Result<SseStream, (StatusCode, Json<serde_json::Value>)> {
    info!("POST /api/game/{}/opening — 開場發言", session_id);

    // 先驗證 session 存在
    {
        let sessions = state.sessions.lock().await;
        if !sessions.contains_key(&session_id) {
            return Err(api_err(StatusCode::NOT_FOUND, "找不到遊戲"));
        }
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);

    tokio::spawn(async move {
        let order = {
            let sessions = state.sessions.lock().await;
            let session = sessions.get(&session_id).unwrap();
            session.speaking_order()
        };

        for actor in &order {
            info!("  演員 {} 正在回應...", actor.name);
            let msg = {
                let mut sessions = state.sessions.lock().await;
                let session = sessions.get_mut(&session_id).unwrap();
                match session.actor_respond(&actor.id, &state.model).await {
                    Ok(content) => {
                        info!("  演員 {} 回應完成（{} 字）", actor.name, content.len());
                        MessageInfo {
                            speaker: actor.name.clone(),
                            content,
                        }
                    }
                    Err(e) => {
                        warn!("  演員 {} 回應失敗：{}", actor.name, e);
                        MessageInfo {
                            speaker: "系統".into(),
                            content: format!("{} 回應失敗：{}", actor.name, e),
                        }
                    }
                }
            };

            let json = serde_json::to_string(&msg).unwrap();
            let event = Event::default().event("message").data(json);
            if tx.send(Ok(event)).await.is_err() {
                break; // client disconnected
            }
        }

        // 送出完成事件
        let _ = tx
            .send(Ok(Event::default().event("done").data("ok")))
            .await;
        info!("開場發言 SSE 完成");
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(Sse::new(Box::pin(stream) as Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>)
        .keep_alive(KeepAlive::default()))
}

/// 玩家發言 — 只緩衝訊息，立即回應（不等 LLM）
async fn say(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Json(req): Json<ChatReq>,
) -> ApiResult<serde_json::Value> {
    if req.content.is_empty() {
        return Ok(Json(serde_json::json!({ "ok": true })));
    }
    info!(
        "POST /api/game/{}/say — 緩衝玩家訊息：{}",
        session_id,
        req.content.chars().take(30).collect::<String>()
    );
    // 只鎖 pending_messages（快，不與 LLM 呼叫競爭）
    state
        .pending_messages
        .lock()
        .await
        .entry(session_id)
        .or_default()
        .push(req.content);
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// 觸發 NPC 回應 — 先排入所有待處理玩家訊息，再逐位演員 SSE 推送
async fn respond(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> Result<SseStream, (StatusCode, Json<serde_json::Value>)> {
    info!("POST /api/game/{}/respond — 觸發 NPC 回應", session_id);

    // 1. 取出所有待處理玩家訊息並寫入 transcript
    {
        let msgs = state
            .pending_messages
            .lock()
            .await
            .remove(&session_id)
            .unwrap_or_default();
        let mut sessions = state.sessions.lock().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "找不到遊戲"))?;
        for msg in msgs {
            session.student_says(msg);
        }
    }

    // 2. SSE 逐位演員回應
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);

    tokio::spawn(async move {
        let order = {
            let sessions = state.sessions.lock().await;
            let session = sessions.get(&session_id).unwrap();
            session.speaking_order()
        };

        for actor in &order {
            // 每位演員回應前，先把期間新到的玩家訊息也排入
            {
                let msgs = state
                    .pending_messages
                    .lock()
                    .await
                    .remove(&session_id)
                    .unwrap_or_default();
                if !msgs.is_empty() {
                    let mut sessions = state.sessions.lock().await;
                    let session = sessions.get_mut(&session_id).unwrap();
                    for msg in msgs {
                        session.student_says(msg);
                    }
                }
            }

            let msg = {
                let mut sessions = state.sessions.lock().await;
                let session = sessions.get_mut(&session_id).unwrap();
                match session.actor_respond(&actor.id, &state.model).await {
                    Ok(content) => MessageInfo {
                        speaker: actor.name.clone(),
                        content,
                    },
                    Err(e) => MessageInfo {
                        speaker: "系統".into(),
                        content: format!("{} 回應失敗：{}", actor.name, e),
                    },
                }
            };

            let json = serde_json::to_string(&msg).unwrap();
            let event = Event::default().event("message").data(json);
            if tx.send(Ok(event)).await.is_err() {
                break;
            }
        }

        let _ = tx
            .send(Ok(Event::default().event("done").data("ok")))
            .await;
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(Sse::new(Box::pin(stream) as Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>)
        .keep_alive(KeepAlive::default()))
}

/// 指控演員是騙子（支援多人指控）
async fn accuse(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Json(req): Json<AccuseReq>,
) -> ApiResult<AccuseResponse> {
    let sessions = state.sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "找不到遊戲"))?;

    if req.actor_indices.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "請至少指控一位成員"));
    }

    let mut accused_ids = Vec::new();
    for &idx in &req.actor_indices {
        if idx < 1 || idx > session.actors.len() {
            return Err(api_err(StatusCode::BAD_REQUEST, format!("無效的成員編號：{}", idx)));
        }
        accused_ids.push(session.actors[idx - 1].id.clone());
    }

    let (verdict, feedback) = session.score_multi(&accused_ids, &req.reason);

    let verdict_str = match verdict {
        shared_types::Verdict::Green => "green",
        shared_types::Verdict::Yellow => "yellow",
        shared_types::Verdict::Red => "red",
    };

    Ok(Json(AccuseResponse {
        verdict: verdict_str.into(),
        feedback,
        correct_answer: session.topic.correct_answer.clone(),
        liar_count: session.liar_ids.len(),
    }))
}

// ── 主程式 ──────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "infolit_web=info,infolit_game=warn,llm_gateway=warn".into()),
        )
        .init();

    // 載入內容
    let actors = load_actors_from_dir(&args.content_dir.join("actors"))?;
    let catalog = load_deceptions_from_dir(&args.content_dir.join("deception-patterns"))?;
    let topics = load_topics_from_dir(&args.content_dir.join("topics"))?;
    info!(
        "載入 {} 位演員，{} 個騙術，{} 題題目",
        actors.len(),
        catalog.len(),
        topics.len()
    );

    // 建立 LLM provider
    let thinking = if args.think {
        ThinkingMode::On
    } else {
        ThinkingMode::Off
    };
    let provider: Arc<dyn LlmProvider> =
        Arc::new(OllamaProvider::new(&args.ollama_url, &args.model).with_thinking(thinking));

    let state = Arc::new(AppState {
        sessions: Mutex::new(HashMap::new()),
        pending_messages: Mutex::new(HashMap::new()),
        actors,
        catalog,
        topics,
        provider,
        model: args.model.clone(),
    });

    let app = Router::new()
        .route("/", get(index_page))
        .route("/api/health", get(health))
        .route("/api/game/new", post(create_game))
        .route("/api/game/:id/opening", post(opening_round))
        .route("/api/game/:id/say", post(say))
        .route("/api/game/:id/respond", post(respond))
        .route("/api/game/:id/accuse", post(accuse))
        .fallback(fallback)
        .with_state(state);

    let addr = format!("{}:{}", args.host, args.port);
    info!("InfoLit Web 啟動於 http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
