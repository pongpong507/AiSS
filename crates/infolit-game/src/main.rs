//! InfoLit CLI Spike — Milestone 0
//!
//! 在終端機中玩一局「誰在胡說八道」，驗證 multi-agent 核心迴圈。
//!
//! 用法：
//!   infolit-cli --content-dir ./content --model qwen2.5:14b
//!   infolit-cli --content-dir ./content --model qwen2.5:14b --ollama-url http://localhost:11434

use anyhow::Context;
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
use llm_gateway::types::{ChatMessage, ChatRequest};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "infolit-cli", about = "InfoLit 資訊判讀遊戲 CLI（Milestone 0 spike）")]
struct Args {
    /// 內容目錄（含 actors/ 和 deception-patterns/ 子目錄）
    #[arg(long, default_value = "./content")]
    content_dir: PathBuf,

    /// Ollama 模型名稱
    #[arg(long, default_value = "qwen2.5:7b")]
    model: String,

    /// Ollama 服務位置
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,

    /// 本局演員數量
    #[arg(long, default_value_t = 3)]
    actors: usize,

    /// 騙子數量
    #[arg(long, default_value_t = 1)]
    liars: usize,

    /// 關閉 pacing 延遲（加速測試用）
    #[arg(long)]
    no_delay: bool,

    /// 啟用 thinking 模式（gemma4 等模型會先推理再回答，品質更好但較慢）
    #[arg(long)]
    think: bool,

    /// 預檢模式：只檢查 Ollama 與內容資料就緒，不進入遊戲
    #[arg(long)]
    doctor: bool,

    /// 自動繼續：玩家閒置 N 秒後，NPC 自動繼續對話（0 = 關閉）
    #[arg(long, default_value_t = 0)]
    auto_timeout: u64,

    /// 顯示詳細 debug 訊息（預設只顯示對話內容）
    #[arg(long)]
    verbose: bool,
}

fn print_divider(label: &str) {
    println!("\n{}", "─".repeat(50));
    if !label.is_empty() {
        println!("  {}", label);
        println!("{}", "─".repeat(50));
    }
}

fn print_turn(speaker: &str, content: &str) {
    println!("\n【{}】{}", speaker, content);
}

/// 預檢：檢查內容、Ollama、模型、最小 chat 來回是否成功
async fn run_doctor(args: &Args) -> anyhow::Result<()> {
    println!("🩺 執行 InfoLit 環境預檢...\n");

    // 1. 內容檢查
    let actors_dir = args.content_dir.join("actors");
    let deceptions_dir = args.content_dir.join("deception-patterns");

    let topics_dir = args.content_dir.join("topics");

    print!("  [1/4] 內容檔案... ");
    io::stdout().flush().ok();
    let actors = load_actors_from_dir(&actors_dir)
        .with_context(|| format!("載入演員失敗：{:?}", actors_dir))?;
    let catalog = load_deceptions_from_dir(&deceptions_dir)
        .with_context(|| format!("載入騙術失敗：{:?}", deceptions_dir))?;
    let topics = load_topics_from_dir(&topics_dir)
        .with_context(|| format!("載入題庫失敗：{:?}", topics_dir))?;
    println!("✅ {} 位演員，{} 個騙術，{} 題題目", actors.len(), catalog.len(), topics.len());

    if actors.len() < args.actors {
        anyhow::bail!(
            "演員不足：需要 {}，content/actors/ 只有 {}",
            args.actors,
            actors.len()
        );
    }

    // 2. Ollama 服務存活
    let thinking = if args.think { ThinkingMode::On } else { ThinkingMode::Off };
    let provider = OllamaProvider::new(&args.ollama_url, &args.model).with_thinking(thinking);
    print!("  [2/4] Ollama 連線（{}）... ", args.ollama_url);
    io::stdout().flush().ok();
    provider
        .health()
        .await
        .with_context(|| "Ollama 健康檢查失敗")?;
    println!("✅");

    // 3. 模型已下載
    print!("  [3/4] 模型 `{}` 已下載... ", args.model);
    io::stdout().flush().ok();
    let models = provider.list_models().await?;
    let model_present = models.iter().any(|m| m == &args.model || m.starts_with(&format!("{}:", args.model)));
    if !model_present {
        println!("❌");
        eprintln!("\n找不到模型 `{}`。已下載的模型：", args.model);
        for m in &models {
            eprintln!("  - {}", m);
        }
        eprintln!("\n請執行：ollama pull {}", args.model);
        anyhow::bail!("model not found");
    }
    println!("✅");

    // 4. 一次完整 chat 來回
    print!("  [4/4] 試跑一次推論... ");
    io::stdout().flush().ok();
    let probe = ChatRequest::new(
        &args.model,
        vec![ChatMessage::user("用一句話說：你好。")],
    )
    .with_temperature(0.3)
    .with_max_tokens(256);
    let resp = provider
        .chat(probe)
        .await
        .with_context(|| "Ollama chat 試跑失敗")?;
    println!("✅");
    println!("\n🤖 模型回應：「{}」", resp.content.trim());

    println!("\n🎉 預檢全數通過。可以執行：");
    println!(
        "    cargo run -p infolit-game -- --model {} --content-dir {}",
        args.model,
        args.content_dir.display()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let default_log = if args.verbose {
        "infolit_game=info,llm_gateway=debug"
    } else {
        "infolit_game=warn,llm_gateway=warn"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| default_log.into()),
        )
        .init();

    if args.doctor {
        return run_doctor(&args).await;
    }

    // ── 載入內容 ──────────────────────────────────────────────────────────────
    let actors_dir = args.content_dir.join("actors");
    let deceptions_dir = args.content_dir.join("deception-patterns");

    let actors = load_actors_from_dir(&actors_dir)
        .with_context(|| format!("載入演員失敗：{:?}", actors_dir))?;
    let catalog = load_deceptions_from_dir(&deceptions_dir)
        .with_context(|| format!("載入騙術失敗：{:?}", deceptions_dir))?;

    info!("載入 {} 位演員，{} 個騙術", actors.len(), catalog.len());

    if actors.len() < args.actors {
        anyhow::bail!(
            "演員不足：需要 {}，content/actors/ 只有 {}。\n請先用 aiss-import 匯入更多演員。",
            args.actors, actors.len()
        );
    }

    // ── 組裝本局陣容 ──────────────────────────────────────────────────────────
    let (selected_actors, liar_ids, deceptions) =
        assemble_session(&actors, &catalog, args.actors, args.liars, &[])?;

    // 從題庫載入並隨機選題
    let topics_dir = args.content_dir.join("topics");
    let topics = load_topics_from_dir(&topics_dir)
        .with_context(|| format!("載入題庫失敗：{:?}", topics_dir))?;
    let topic = topics[rand::random::<usize>() % topics.len()].clone();
    info!("載入 {} 題題目", topics.len());

    // ── 建立 Provider ─────────────────────────────────────────────────────────
    let thinking = if args.think { ThinkingMode::On } else { ThinkingMode::Off };
    let ollama = OllamaProvider::new(&args.ollama_url, &args.model).with_thinking(thinking);
    // 啟動前先 ping 一下，避免在遊戲半途才發現 Ollama 沒開
    ollama
        .health()
        .await
        .with_context(|| format!("Ollama 預檢失敗（{}）。請執行 `ollama serve`，或加 --doctor 看詳細診斷。", args.ollama_url))?;
    let provider: Arc<dyn LlmProvider> = Arc::new(ollama);

    // ── 建立 Session ──────────────────────────────────────────────────────────
    let pacing = if args.no_delay {
        PacingConfig { min_response_delay_ms: 0, max_response_delay_ms: 0, ..Default::default() }
    } else {
        PacingConfig::default()
    };

    let mut session =
        GameSession::new(selected_actors.clone(), liar_ids, deceptions, topic.clone(), pacing, provider);

    // ── 遊戲開始介紹 ──────────────────────────────────────────────────────────
    print_divider("🚀 InfoLit 資訊判讀遊戲（CLI Spike）");
    println!();
    println!("今天的討論主題：「{}」", topic.question);
    println!();
    println!("參與討論的成員：");
    for (i, actor) in selected_actors.iter().enumerate() {
        println!("  {}. {} — {}", i + 1, actor.name, actor.short_bio);
    }
    println!();
    println!("⚠️  其中有人在說謊！請仔細觀察，用「三問追問法」找出騙子：");
    println!("   1️⃣  「你的資料來源是什麼？」（誰說的？）");
    println!("   2️⃣  「這個說法是最近的嗎？」（何時發布？）");
    println!("   3️⃣  「這合理嗎？有沒有反例？」（合不合理？）");
    println!();
    println!("輸入你想說的話，或輸入 /accuseN（例如 /accuse2）指控第 N 位成員說謊。");
    println!("輸入 /quit 結束遊戲。");

    // ── 先讓所有演員各開場一次（隨機順序）─────────────────────────────────────
    print_divider("📢 各成員開場發言");
    let opening_order = session.speaking_order();
    for actor in &opening_order {
        match session.actor_respond(&actor.id, &args.model).await {
            Ok(response) => print_turn(&actor.name, &response),
            Err(e) => {
                eprintln!("⚠️  {} 回應失敗：{}（繼續遊戲）", actor.name, e);
            }
        }
    }

    // ── 互動迴圈 ─────────────────────────────────────────────────────────────
    print_divider("💬 開始提問");
    let mut round = 0u32;
    const MAX_ROUNDS: u32 = 10;

    if args.auto_timeout > 0 {
        println!("（已啟用自動繼續：閒置 {} 秒後 NPC 會自動對話）", args.auto_timeout);
    }

    loop {
        if round >= MAX_ROUNDS {
            println!("\n（已達最大回合數 {}，進入最終指控）", MAX_ROUNDS);
            break;
        }

        print!("\n你 > ");
        io::stdout().flush()?;

        // 讀取玩家輸入（如果啟用 auto_timeout，逾時後自動繼續）
        let input = if args.auto_timeout > 0 {
            let timeout_dur = tokio::time::Duration::from_secs(args.auto_timeout);
            let read_future = tokio::task::spawn_blocking(|| {
                let mut buf = String::new();
                io::stdin().lock().read_line(&mut buf).ok();
                buf
            });
            match tokio::time::timeout(timeout_dur, read_future).await {
                Ok(Ok(buf)) => buf.trim().to_string(),
                _ => {
                    println!("\n（你沉默了一會兒，NPC 們繼續討論...）");
                    String::new()
                }
            }
        } else {
            let mut buf = String::new();
            io::stdin().lock().read_line(&mut buf)?;
            buf.trim().to_string()
        };

        // 空輸入：如果有 auto_timeout，讓 NPC 自動對話；否則等待重新輸入
        if input.is_empty() {
            if args.auto_timeout > 0 {
                round += 1;
                // NPC 自動繼續，不加學生訊息
            } else {
                continue;
            }
        } else {
            // 解析指令
            if input == "/quit" {
                println!("（遊戲結束）");
                return Ok(());
            }

            if let Some(rest) = input.strip_prefix("/accuse") {
                let idx: usize = rest.trim().parse().unwrap_or(0);
                if idx < 1 || idx > selected_actors.len() {
                    println!("請輸入有效的成員編號（1 到 {}）", selected_actors.len());
                    continue;
                }
                let accused = &selected_actors[idx - 1];

                print!("你的理由（按 Enter 跳過）：");
                io::stdout().flush()?;
                let mut reason = String::new();
                io::stdin().lock().read_line(&mut reason)?;
                let reason = reason.trim().to_string();

                print_divider("⚖️  判決");
                let (_verdict, feedback) = session.score(&accused.id, &reason);
                println!("{}", feedback);
                println!();
                println!("【正確答案】{}", topic.correct_answer);
                break;
            }

            // 一般輸入
            session.student_says(input);
            round += 1;
        }

        // 檢查是否有沉默太久的演員，讓其他人 cue 他
        let silent = session.silent_actors();
        let turn_order = session.speaking_order();
        for actor in &turn_order {
            // 如果這個演員要 cue 沉默者，先印提示
            if !silent.is_empty() && !silent.contains(&actor.id) {
                for sid in &silent {
                    if let Some(silent_actor) = selected_actors.iter().find(|a| &a.id == sid) {
                        println!("\n（{} 轉向 {} 說：「你覺得呢？」）", actor.name, silent_actor.name);
                    }
                }
            }
            match session.actor_respond(&actor.id, &args.model).await {
                Ok(response) => print_turn(&actor.name, &response),
                Err(e) => {
                    eprintln!("⚠️  {} 回應失敗：{}", actor.name, e);
                }
            }
        }
    }

    print_divider("🎓 遊戲結束");
    println!("感謝參與！記得下次繼續練習資訊判讀能力。");
    Ok(())
}
