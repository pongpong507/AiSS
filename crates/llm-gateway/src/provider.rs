//! LlmProvider trait — 所有 provider adapter 必須實作此介面。
//!
//! 設計原則：切換 provider 只需換 adapter，上層邏輯完全不動。

use crate::error::LlmError;
use crate::types::{ChatChunk, ChatRequest, ChatResponse, Embedding, ProviderCapabilities};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatChunk, LlmError>> + Send>>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider 識別名稱（用於 log、指標）
    fn name(&self) -> &'static str;

    /// 這個 provider 支援哪些能力
    fn capabilities(&self) -> ProviderCapabilities;

    /// 非串流對話（等待完整回應）
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError>;

    /// 串流對話（逐 token 回傳）
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, LlmError>;

    /// 產生文字嵌入向量（RAG / 語義搜尋用）
    async fn embed(&self, texts: &[String]) -> Result<Vec<Embedding>, LlmError>;
}
