//! # aiss-import CLI
//!
//! 批次匯入工具：從 YAML / CSV / Excel 匯入演員、騙術等內容資料。
//!
//! Milestone 0 範圍：僅支援演員（actors）與騙術（deceptions）。
//!
//! 用法：
//!   aiss-import actors     ./data/actors.yaml     --out ./content/actors/
//!   aiss-import actors     ./data/actors.csv      --out ./content/actors/ --dry-run
//!   aiss-import deceptions ./data/deceptions.yaml --out ./content/deception-patterns/

mod importers;

use anyhow::Context;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aiss-import", about = "AiSS 批次內容匯入工具（Milestone 0：演員與騙術）")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 匯入演員資料
    Actors {
        /// 來源檔案（.yaml / .csv）
        input: PathBuf,
        /// 輸出目錄（每筆資料存為獨立 .yaml 檔）
        #[arg(long, default_value = "./content/actors")]
        out: PathBuf,
        /// 只驗證，不實際寫入
        #[arg(long)]
        dry_run: bool,
    },
    /// 匯入騙術資料
    Deceptions {
        /// 來源檔案（.yaml / .csv）
        input: PathBuf,
        /// 輸出目錄
        #[arg(long, default_value = "./content/deception-patterns")]
        out: PathBuf,
        /// 只驗證，不實際寫入
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "aiss_import=info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Actors { input, out, dry_run } => {
            importers::actors::run(&input, &out, dry_run)
                .with_context(|| format!("匯入演員失敗：{:?}", input))?;
        }
        Commands::Deceptions { input, out, dry_run } => {
            importers::deceptions::run(&input, &out, dry_run)
                .with_context(|| format!("匯入騙術失敗：{:?}", input))?;
        }
    }

    Ok(())
}
