//! # Debug log 模組
//!
//! 把每局 session 的對話內容（LLM 原始輸出 + 切片後的片段 + 學生發言）
//! 寫到本地檔案，方便前期除錯（觀察 LLM 是否吐重複內容、prompt 進去後 GAI 反應如何）。
//!
//! ## 啟用方式
//! 設定環境變數 `INFOLIT_DEBUG_LOG`（任何「真值」都算開啟：`1`、`true`、`yes`、`on`、`y`）；
//! 未設定或為「假值」則整套機制關閉。
//!
//! ## 檔案格式
//! 每個 session 一個檔：`logs/session-<uuid>.md`，Markdown 格式好讀。

use crate::actor::Actor;
use shared_types::Topic;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

const ENV_VAR: &str = "INFOLIT_DEBUG_LOG";
const LOG_DIR: &str = "logs";

fn is_enabled() -> bool {
    match std::env::var(ENV_VAR) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "y"
        ),
        Err(_) => false,
    }
}

fn log_path(session_id: Uuid) -> Option<PathBuf> {
    if !is_enabled() {
        return None;
    }
    let dir_path = PathBuf::from(LOG_DIR);
    if let Err(e) = fs::create_dir_all(&dir_path) {
        eprintln!("[debug_log] 無法建立目錄 {:?}: {}", dir_path, e);
        return None;
    }
    Some(dir_path.join(format!("session-{}.md", session_id)))
}

fn append(path: &PathBuf, content: &str) {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                eprintln!("[debug_log] 寫入失敗 {:?}: {}", path, e);
            }
        }
        Err(e) => eprintln!("[debug_log] 開檔失敗 {:?}: {}", path, e),
    }
}

fn timestamp_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 簡單的 HH:MM:SS，不依賴 chrono；用 UTC 秒轉成 local 近似
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02} UTC", h, m, s)
}

/// 新 session 開檔時寫入：主題、演員清單、騙子名單
pub fn write_session_header(
    session_id: Uuid,
    topic: &Topic,
    actors: &[Actor],
    liar_ids: &[String],
) {
    let Some(path) = log_path(session_id) else { return };
    let liar_names: Vec<&str> = liar_ids
        .iter()
        .filter_map(|id| actors.iter().find(|a| &a.id == id))
        .map(|a| a.name.as_str())
        .collect();
    let actor_names: Vec<&str> = actors.iter().map(|a| a.name.as_str()).collect();

    let header = format!(
        "# Session `{}`\n\
         **Started**: {}\n\
         **Topic**: {}\n\
         **Correct answer**: {}\n\
         **Actors**: {}\n\
         **Liars**: {}\n\n---\n\n",
        session_id,
        timestamp_now(),
        topic.question,
        topic.correct_answer,
        actor_names.join("、"),
        liar_names.join("、"),
    );
    append(&path, &header);
}

/// 每位 NPC 發言時寫入：LLM 原始輸出 + 切完的片段
pub fn write_actor_turn(
    session_id: Uuid,
    actor: &Actor,
    is_liar: bool,
    raw_llm_output: &str,
    fragments: &[String],
) {
    let Some(path) = log_path(session_id) else { return };
    let liar_marker = if is_liar { " 🤥" } else { "" };
    let fragments_block = if fragments.is_empty() {
        "_（無片段）_".to_string()
    } else {
        fragments
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let entry = format!(
        "### `[{}]` {}{} (`{}`)\n\
         **Raw LLM** ({} chars):\n\
         ```\n{}\n```\n\
         **Fragments** ({}):\n{}\n\n---\n\n",
        timestamp_now(),
        actor.name,
        liar_marker,
        actor.id,
        raw_llm_output.chars().count(),
        raw_llm_output,
        fragments.len(),
        fragments_block,
    );
    append(&path, &entry);
}

/// 學生發言時寫入
pub fn write_student_turn(session_id: Uuid, content: &str) {
    let Some(path) = log_path(session_id) else { return };
    let entry = format!(
        "### `[{}]` 👤 學生\n{}\n\n---\n\n",
        timestamp_now(),
        content,
    );
    append(&path, &entry);
}
