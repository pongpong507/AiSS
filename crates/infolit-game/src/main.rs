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
};
use llm_gateway::adapters::OllamaProvider;
use llm_gateway::provider::LlmProvider;
use llm_gateway::types::{ChatMessage, ChatRequest};
use shared_types::{Difficulty, Topic};
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

    /// 預檢模式：只檢查 Ollama 與內容資料就緒，不進入遊戲
    #[arg(long)]
    doctor: bool,
}

/// 硬編碼的種子題庫（Milestone 0，真實題庫由 aiss-import 匯入）
fn seed_topics() -> Vec<Topic> {
    vec![
        Topic {
            id: "q-001".into(),
            question: "海豚真的是魚類嗎？".into(),
            correct_answer: "不是。海豚是哺乳類動物，需要浮出水面呼吸空氣，用肺呼吸，而非用鰓。".into(),
            difficulty: Difficulty::Easy,
            tags: vec!["生物".into(), "海洋".into()],
        },
        Topic {
            id: "q-002".into(),
            question: "微波爐加熱食物之後，食物會帶有輻射嗎？".into(),
            correct_answer: "不會。微波爐使用的是非電離輻射（電磁波），只是讓水分子振動產生熱，不會讓食物帶有放射性。".into(),
            difficulty: Difficulty::Easy,
            tags: vec!["科學".into(), "日常生活".into()],
        },
        Topic {
            id: "q-003".into(),
            question: "人類只使用了大腦的 10% 嗎？".into(),
            correct_answer: "不是。這是一個流傳已久的迷思。神經科學研究顯示，幾乎所有大腦區域都有功能，長期不使用的腦區會萎縮。".into(),
            difficulty: Difficulty::Medium,
            tags: vec!["神經科學".into(), "迷思".into()],
        },
    ]
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

    print!("  [1/4] 內容檔案... ");
    io::stdout().flush().ok();
    let actors = load_actors_from_dir(&actors_dir)
        .with_context(|| format!("載入演員失敗：{:?}", actors_dir))?;
    let catalog = load_deceptions_from_dir(&deceptions_dir)
        .with_context(|| format!("載入騙術失敗：{:?}", deceptions_dir))?;
    println!("✅ {} 位演員，{} 個騙術", actors.len(), catalog.len());

    if actors.len() < args.actors {
        anyhow::bail!(
            "演員不足：需要 {}，content/actors/ 只有 {}",
            args.actors,
            actors.len()
        );
    }

    // 2. Ollama 服務存活
    let provider = OllamaProvider::new(&args.ollama_url, &args.model);
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
    .with_max_tokens(32);
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

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "infolit_game=info,llm_gateway=debug".into()),
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
        assemble_session(&actors, &catalog, args.actors, args.liars)?;

    // 隨機選題
    let topics = seed_topics();
    let topic = topics[rand::random::<usize>() % topics.len()].clone();

    // ── 建立 Provider ─────────────────────────────────────────────────────────
    let ollama = OllamaProvider::new(&args.ollama_url, &args.model);
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

    // ── 先讓所有演員各開場一次 ───────────────────────────────────────────────
    print_divider("📢 各成員開場發言");
    for actor in &selected_actors.clone() {
        match session.actor_respond(&actor.id, &args.model).await {
            Ok(response) => print_turn(&actor.name, &response),
            Err(e) => {
                eprintln!("⚠️  {} 回應失敗：{}（繼續遊戲）", actor.name, e);
            }
        }
    }

    // ── 互動迴圈 ─────────────────────────────────────────────────────────────
    print_divider("💬 開始提問");
    let stdin = io::stdin();
    let mut round = 0u32;
    const MAX_ROUNDS: u32 = 10;

    loop {
        if round >= MAX_ROUNDS {
            println!("\n（已達最大回合數 {}，進入最終指控）", MAX_ROUNDS);
            break;
        }

        print!("\n你 > ");
        io::stdout().flush()?;

        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        // 解析指令
        if input == "/quit" {
            println!("（遊戲結束）");
            return Ok(());
        }

        if let Some(rest) = input.strip_prefix("/accuse") {
            // 指控某個成員
            let idx: usize = rest.trim().parse().unwrap_or(0);
            if idx < 1 || idx > selected_actors.len() {
                println!("請輸入有效的成員編號（1 到 {}）", selected_actors.len());
                continue;
            }
            let accused = &selected_actors[idx - 1];

            print!("你的理由（按 Enter 跳過）：");
            io::stdout().flush()?;
            let mut reason = String::new();
            stdin.lock().read_line(&mut reason)?;
            let reason = reason.trim().to_string();

            print_divider("⚖️  判決");
            let (_verdict, feedback) = session.score(&accused.id, &reason);
            println!("{}", feedback);
            println!();

            // 揭露答案
            println!("【正確答案】{}", topic.correct_answer);
            break;
        }

        // 一般輸入 → 讓所有演員回應
        session.student_says(input);
        round += 1;

        for actor in &selected_actors.clone() {
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
