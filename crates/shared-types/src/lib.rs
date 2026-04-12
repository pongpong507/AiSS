//! # shared-types
//!
//! 跨 crate 共用的基礎型別定義。
//!
//! **技術文件**：`docs/modules/shared-types.md`

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 使用者上下文（從 OAuth session 取得）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: Uuid,
    pub display_name: String,
}

/// 遊戲結果（回報給 HostEnvironment）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameResult {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub score: u32,
    pub verdict: Verdict,
    pub liar_ids_found: Vec<String>,
    pub correct: bool,
}

/// 評分等級（對應手冊紅/黃/綠警報系統）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Verdict {
    /// 🟢 找對騙子且理由正確
    Green,
    /// 🟡 找對但理由模糊
    Yellow,
    /// 🔴 找錯，需重新挑戰
    Red,
}

/// InfoLit 題目主題
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    pub id: String,
    pub question: String,
    pub correct_answer: String,
    pub difficulty: Difficulty,
    pub tags: Vec<String>,
}

/// 難度等級
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}
