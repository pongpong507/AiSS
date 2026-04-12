//! # llm-gateway
//!
//! LLM 統一介面層：所有 LLM 呼叫都經過此 crate，切換 provider 只需換 adapter。
//!
//! **技術文件**：`docs/modules/llm-gateway.md`
//!
//! ## 依賴關係
//! - 被依賴：`infolit-game` / `aiss-npc`
//! - 上游：Ollama / OpenAI / Anthropic (adapter 層)

pub mod error;
pub mod provider;
pub mod types;
pub mod adapters;
