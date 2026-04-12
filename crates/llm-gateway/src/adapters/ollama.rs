//! Ollama provider adapter
//!
//! 連接本地 Ollama 服務（預設 http://localhost:11434）。
//! 支援：chat（非串流 + 串流）、embed。

use crate::error::LlmError;
use crate::provider::{ChatStream, LlmProvider};
use crate::types::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Embedding, ProviderCapabilities, Role,
    TokenUsage,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, instrument};

/// Thinking 模式設定
///
/// 部分模型（gemma4、QwQ 等）支援 thinking mode，會先產生內部推理再輸出最終回答。
/// thinking 與 content 共用 token 預算，因此開啟時需要更多 max_tokens。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingMode {
    /// 關閉 thinking（預設）— 所有 token 都給 content
    Off,
    /// 開啟 thinking — 自動將 max_tokens 放大以容納 thinking + content，
    /// 回應只取 content，thinking 存入 extensions["thinking"] 供除錯
    On,
}

impl Default for ThinkingMode {
    fn default() -> Self {
        Self::Off
    }
}

/// Ollama provider，對應 Ollama REST API v1
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    /// 預設使用的模型（可被 ChatRequest.model 覆寫）
    default_model: String,
    /// Thinking 模式設定
    thinking: ThinkingMode,
}

/// thinking 開啟時，自動放大 max_tokens 的倍數
const THINKING_TOKEN_MULTIPLIER: u32 = 4;
/// thinking 開啟時的最低 max_tokens（確保 thinking + content 都有空間）
const THINKING_MIN_TOKENS: u32 = 2048;

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>, default_model: impl Into<String>) -> Self {
        // 連線 5 秒、整體 5 分鐘（首次推論大模型可能慢）
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(300))
            .build()
            .expect("reqwest client build should not fail");
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            default_model: default_model.into(),
            thinking: ThinkingMode::Off,
        }
    }

    /// 使用預設 localhost:11434
    pub fn local(default_model: impl Into<String>) -> Self {
        Self::new("http://localhost:11434", default_model)
    }

    /// 設定 thinking 模式
    pub fn with_thinking(mut self, mode: ThinkingMode) -> Self {
        self.thinking = mode;
        self
    }

    /// Health check：呼叫 `/api/tags` 確認 Ollama 服務存活
    pub async fn health(&self) -> Result<(), LlmError> {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| LlmError::Provider {
                provider: "ollama".into(),
                message: format!(
                    "無法連線到 Ollama（{}）：{}。請確認 `ollama serve` 已啟動。",
                    self.base_url, e
                ),
            })?
            .error_for_status()
            .map_err(|e| LlmError::Provider {
                provider: "ollama".into(),
                message: e.to_string(),
            })?;
        Ok(())
    }

    /// 列出 Ollama 已下載的模型
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| LlmError::Provider {
                provider: "ollama".into(),
                message: format!("無法連線到 Ollama：{}", e),
            })?
            .error_for_status()
            .map_err(|e| LlmError::Provider {
                provider: "ollama".into(),
                message: e.to_string(),
            })?;
        let tags: OllamaTagsResponse = resp.json().await?;
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagEntry>,
}

#[derive(Deserialize)]
struct OllamaTagEntry {
    name: String,
}

// ─── Ollama API 型別 ───────────────────────────────────────────────────────────

/// Ollama /api/chat 請求（OpenAI 相容格式）
#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    /// 關閉 thinking mode（gemma4 等模型預設會內部推理，佔用大量 token）
    think: bool,
}

#[derive(Serialize)]
struct OllamaMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

/// Ollama /api/chat 非串流回應
#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
    model: String,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    thinking: Option<String>,
}

/// Ollama /api/chat 串流 chunk
#[derive(Deserialize)]
struct OllamaStreamChunk {
    message: OllamaResponseMessage,
    done: bool,
}

/// Ollama /api/embed 請求
#[derive(Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

/// Ollama /api/embed 回應
#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

// ─── 工具函數 ──────────────────────────────────────────────────────────────────

fn role_to_str(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn build_messages<'a>(req: &'a ChatRequest) -> Vec<OllamaMessage<'a>> {
    let mut msgs: Vec<OllamaMessage<'a>> = Vec::with_capacity(req.messages.len() + 1);

    // 若有 system prompt 且訊息列表沒有 system 角色，前置插入
    if let Some(sys) = &req.system {
        if !req.messages.iter().any(|m| m.role == Role::System) {
            msgs.push(OllamaMessage { role: "system", content: sys.as_str() });
        }
    }

    for m in &req.messages {
        msgs.push(OllamaMessage { role: role_to_str(&m.role), content: &m.content });
    }
    msgs
}

// ─── LlmProvider 實作 ─────────────────────────────────────────────────────────

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &'static str {
        "ollama"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            tool_use: false, // Ollama 工具呼叫支援依模型而定，暫設 false
            vision: false,
            json_mode: true,
            embeddings: true,
        }
    }

    #[instrument(skip(self, req), fields(model = %req.model))]
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        let model = if req.model.is_empty() { &self.default_model } else { &req.model };
        let messages = build_messages(&req);
        let think = self.thinking == ThinkingMode::On;

        // thinking 開啟時自動放大 max_tokens，確保 thinking + content 都有空間
        let effective_max_tokens = if think {
            req.max_tokens.map(|t| (t * THINKING_TOKEN_MULTIPLIER).max(THINKING_MIN_TOKENS))
        } else {
            req.max_tokens
        };

        let options = if req.temperature.is_some() || effective_max_tokens.is_some() {
            Some(OllamaOptions { temperature: req.temperature, num_predict: effective_max_tokens })
        } else {
            None
        };

        let body = OllamaChatRequest { model, messages, stream: false, options, think };

        debug!(provider = "ollama", model, think, ?effective_max_tokens, "sending chat request");

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    LlmError::Provider {
                        provider: "ollama".into(),
                        message: format!(
                            "Ollama 連線失敗（{}）：{}。請執行 `ollama serve` 並確認模型已下載（`ollama pull {}`）。",
                            self.base_url, e, model
                        ),
                    }
                } else {
                    LlmError::Http(e)
                }
            })?
            .error_for_status()
            .map_err(|e| LlmError::Provider { provider: "ollama".into(), message: e.to_string() })?;

        let raw_text = resp.text().await?;
        debug!(provider = "ollama", raw_len = raw_text.len(), "raw response received");
        let ollama_resp: OllamaChatResponse = serde_json::from_str(&raw_text)?;

        let thinking = ollama_resp.message.thinking;

        // content 有值就用 content；若 content 為空但有 thinking，fallback 用 thinking
        let content = if ollama_resp.message.content.is_empty() {
            debug!(provider = "ollama", "content is empty, falling back to thinking");
            thinking.clone().unwrap_or_default()
        } else {
            ollama_resp.message.content
        };

        let usage = match (ollama_resp.prompt_eval_count, ollama_resp.eval_count) {
            (Some(p), Some(c)) => Some(TokenUsage {
                prompt_tokens: p,
                completion_tokens: c,
                total_tokens: p + c,
            }),
            _ => None,
        };

        // 把 thinking 存進 extensions 供除錯 / 教師後台使用
        let mut extensions = HashMap::new();
        if let Some(t) = thinking {
            extensions.insert("thinking".into(), serde_json::Value::String(t));
        }

        Ok(ChatResponse {
            content,
            model: ollama_resp.model,
            usage,
            extensions,
        })
    }

    #[instrument(skip(self, req), fields(model = %req.model))]
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, LlmError> {
        let model = if req.model.is_empty() {
            self.default_model.clone()
        } else {
            req.model.clone()
        };
        let messages = build_messages(&req)
            .into_iter()
            .map(|m| ChatMessage {
                role: match m.role {
                    "system" => Role::System,
                    "assistant" => Role::Assistant,
                    "tool" => Role::Tool,
                    _ => Role::User,
                },
                content: m.content.to_string(),
                tool_call_id: None,
            })
            .collect::<Vec<_>>();

        let options = if req.temperature.is_some() || req.max_tokens.is_some() {
            Some(OllamaOptions { temperature: req.temperature, num_predict: req.max_tokens })
        } else {
            None
        };

        let body = serde_json::json!({
            "model": model,
            "messages": messages.iter().map(|m| serde_json::json!({
                "role": role_to_str(&m.role),
                "content": m.content
            })).collect::<Vec<_>>(),
            "stream": true,
            "options": options
        });

        debug!(provider = "ollama", %model, "sending streaming chat request");

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| LlmError::Provider { provider: "ollama".into(), message: e.to_string() })?;

        let byte_stream = resp.bytes_stream();

        let chunk_stream = byte_stream.map(|result| {
            let bytes = result.map_err(LlmError::Http)?;
            let line = std::str::from_utf8(&bytes)
                .map_err(|e| LlmError::Provider {
                    provider: "ollama".into(),
                    message: e.to_string(),
                })?
                .trim()
                .to_string();

            if line.is_empty() {
                return Ok(ChatChunk { delta: String::new(), finished: false });
            }

            let chunk: OllamaStreamChunk = serde_json::from_str(&line)?;
            Ok(ChatChunk { delta: chunk.message.content, finished: chunk.done })
        });

        Ok(Box::pin(chunk_stream))
    }

    #[instrument(skip(self, texts), fields(count = texts.len()))]
    async fn embed(&self, texts: &[String]) -> Result<Vec<Embedding>, LlmError> {
        let body = OllamaEmbedRequest { model: &self.default_model, input: texts };

        let resp = self
            .client
            .post(format!("{}/api/embed", self.base_url))
            .json(&body)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| LlmError::Provider { provider: "ollama".into(), message: e.to_string() })?;

        let embed_resp: OllamaEmbedResponse = resp.json().await?;

        Ok(embed_resp.embeddings.into_iter().map(|v| Embedding { values: v }).collect())
    }
}
