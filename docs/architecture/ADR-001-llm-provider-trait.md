---
tags: [adr, llm-gateway, architecture]
related:
  - "[[llm-gateway]]"
  - "[[infolit-session]]"
status: active
last_updated: 2026-04-11
---

# ADR-001：LlmProvider Trait 設計

## 狀態

**已接受（Accepted）**

## 背景

系統需要呼叫 LLM 服務（Ollama 本地、OpenAI、Anthropic 等）。
使用者曾因不同 provider 資料格式差異而「翻車」，需要防止重蹈覆轍。

## 決策

設計 `LlmProvider` async trait，所有 LLM 呼叫都經過此介面。

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> ProviderCapabilities;
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError>;
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, LlmError>;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Embedding>, LlmError>;
}
```

**正規化訊息格式**：`ChatRequest` / `ChatResponse` 是 provider-agnostic，provider-specific 參數放 `extensions: HashMap<String, Value>`。

**Provider 優先序**：Ollama（本地自架）> OpenAI > Anthropic（依需要新增 adapter）。

## 後果

- 切換 provider 只需替換 adapter 實作，上層邏輯完全不動
- `extensions` 欄位讓各家特殊參數（cache_control、logit_bias）有容身之處
- 缺點：需要維護每個 provider 的 adapter，但這是可接受的代價

## 原始碼位置

`crates/llm-gateway/src/provider.rs`、`crates/llm-gateway/src/adapters/ollama.rs`
