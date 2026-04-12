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

/// Ollama provider，對應 Ollama REST API v1
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    /// 預設使用的模型（可被 ChatRequest.model 覆寫）
    default_model: String,
}

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
        }
    }

    /// 使用預設 localhost:11434
    pub fn local(default_model: impl Into<String>) -> Self {
        Self::new("http://localhost:11434", default_model)
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
    content: String,
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

        let options = if req.temperature.is_some() || req.max_tokens.is_some() {
            Some(OllamaOptions { temperature: req.temperature, num_predict: req.max_tokens })
        } else {
            None
        };

        let body = OllamaChatRequest { model, messages, stream: false, options };

        debug!(provider = "ollama", model, "sending chat request");

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

        let ollama_resp: OllamaChatResponse = resp.json().await?;

        let usage = match (ollama_resp.prompt_eval_count, ollama_resp.eval_count) {
            (Some(p), Some(c)) => Some(TokenUsage {
                prompt_tokens: p,
                completion_tokens: c,
                total_tokens: p + c,
            }),
            _ => None,
        };

        Ok(ChatResponse {
            content: ollama_resp.message.content,
            model: ollama_resp.model,
            usage,
            extensions: HashMap::new(),
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
