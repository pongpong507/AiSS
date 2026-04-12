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
    response::{Html, IntoResponse, Json},
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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
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
}

#[derive(Serialize)]
struct ActorInfo {
    index: usize,
    id: String,
    name: String,
    short_bio: String,
}

#[derive(Serialize)]
struct MessagesResponse {
    messages: Vec<MessageInfo>,
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
    actor_index: usize,
    reason: String,
}

#[derive(Serialize)]
struct AccuseResponse {
    verdict: String,
    feedback: String,
    correct_answer: String,
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

/// 建立新遊戲：隨機選題、組裝演員陣容
async fn create_game(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateGameReq>,
) -> ApiResult<GameCreated> {
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
    }))
}

/// 開場發言：每位演員各說一輪
async fn opening_round(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> ApiResult<MessagesResponse> {
    let mut sessions = state.sessions.lock().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "找不到遊戲"))?;

    let order = session.speaking_order();
    let mut messages = Vec::new();

    for actor in &order {
        match session.actor_respond(&actor.id, &state.model).await {
            Ok(content) => messages.push(MessageInfo {
                speaker: actor.name.clone(),
                content,
            }),
            Err(e) => messages.push(MessageInfo {
                speaker: "系統".into(),
                content: format!("{} 回應失敗：{}", actor.name, e),
            }),
        }
    }

    Ok(Json(MessagesResponse { messages }))
}

/// 玩家發言 → 所有演員回應
async fn chat(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Json(req): Json<ChatReq>,
) -> ApiResult<MessagesResponse> {
    let mut sessions = state.sessions.lock().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "找不到遊戲"))?;

    if !req.content.is_empty() {
        session.student_says(req.content);
    }

    let order = session.speaking_order();
    let mut messages = Vec::new();

    for actor in &order {
        match session.actor_respond(&actor.id, &state.model).await {
            Ok(content) => messages.push(MessageInfo {
                speaker: actor.name.clone(),
                content,
            }),
            Err(e) => messages.push(MessageInfo {
                speaker: "系統".into(),
                content: format!("{} 回應失敗：{}", actor.name, e),
            }),
        }
    }

    Ok(Json(MessagesResponse { messages }))
}

/// 指控某位演員是騙子
async fn accuse(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Json(req): Json<AccuseReq>,
) -> ApiResult<AccuseResponse> {
    let sessions = state.sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "找不到遊戲"))?;

    if req.actor_index < 1 || req.actor_index > session.actors.len() {
        return Err(api_err(StatusCode::BAD_REQUEST, "無效的成員編號"));
    }

    let accused = &session.actors[req.actor_index - 1];
    let (verdict, feedback) = session.score(&accused.id, &req.reason);

    let verdict_str = match verdict {
        shared_types::Verdict::Green => "green",
        shared_types::Verdict::Yellow => "yellow",
        shared_types::Verdict::Red => "red",
    };

    Ok(Json(AccuseResponse {
        verdict: verdict_str.into(),
        feedback,
        correct_answer: session.topic.correct_answer.clone(),
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
        actors,
        catalog,
        topics,
        provider,
        model: args.model.clone(),
    });

    let app = Router::new()
        .route("/", get(index_page))
        .route("/api/health", get(health))
        .route("/api/game", post(create_game))
        .route("/api/game/{id}/opening", post(opening_round))
        .route("/api/game/{id}/chat", post(chat))
        .route("/api/game/{id}/accuse", post(accuse))
        .with_state(state);

    let addr = format!("{}:{}", args.host, args.port);
    info!("InfoLit Web 啟動於 http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
