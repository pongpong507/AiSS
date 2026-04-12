//! 正規化的訊息型別，provider-agnostic。
//!
//! 設計原則：
//! - 所有 provider 都翻譯成這個格式；provider-specific 參數放 `extensions`
//! - 加新欄位時不用改 adapter，只要加進 extensions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 對話訊息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// 單條對話訊息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// 工具呼叫結果（role=Tool 時使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), tool_call_id: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), tool_call_id: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_call_id: None }
    }
}

/// 工具定義（供 provider 使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// LLM 呼叫請求（正規化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(default)]
    pub tools: Vec<ToolDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Provider-specific 擴充參數（Anthropic cache_control、OpenAI logit_bias 等）
    #[serde(default)]
    pub extensions: HashMap<String, serde_json::Value>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            system: None,
            tools: vec![],
            temperature: None,
            max_tokens: None,
            extensions: HashMap::new(),
        }
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }
}

/// LLM 回應（正規化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    /// Provider-specific 擴充欄位
    #[serde(default)]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Token 使用量（可觀測性用途）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Streaming chunk
#[derive(Debug, Clone)]
pub struct ChatChunk {
    pub delta: String,
    pub finished: bool,
}

/// Embedding 向量
#[derive(Debug, Clone)]
pub struct Embedding {
    pub values: Vec<f32>,
}

/// Provider 支援的能力旗標
#[derive(Debug, Clone, Default)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub tool_use: bool,
    pub vision: bool,
    pub json_mode: bool,
    pub embeddings: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_constructors() {
        let sys = ChatMessage::system("hi");
        assert_eq!(sys.role, Role::System);
        assert_eq!(sys.content, "hi");

        let user = ChatMessage::user("question");
        assert_eq!(user.role, Role::User);

        let assistant = ChatMessage::assistant("answer");
        assert_eq!(assistant.role, Role::Assistant);
    }

    #[test]
    fn chat_request_builder_chain() {
        let req = ChatRequest::new("test-model", vec![ChatMessage::user("hi")])
            .with_system("be helpful")
            .with_temperature(0.7)
            .with_max_tokens(100);
        assert_eq!(req.model, "test-model");
        assert_eq!(req.system.as_deref(), Some("be helpful"));
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.max_tokens, Some(100));
    }

    #[test]
    fn chat_request_serializes_round_trip() {
        let req = ChatRequest::new("model-x", vec![
            ChatMessage::system("sys"),
            ChatMessage::user("hello"),
        ])
        .with_temperature(0.5);

        let json = serde_json::to_string(&req).unwrap();
        let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, "model-x");
        assert_eq!(parsed.messages.len(), 2);
        assert_eq!(parsed.temperature, Some(0.5));
    }

    #[test]
    fn extensions_field_preserves_provider_specific_data() {
        let mut req = ChatRequest::new("m", vec![]);
        req.extensions.insert(
            "anthropic_cache_control".into(),
            serde_json::json!({"type": "ephemeral"}),
        );
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
        assert!(parsed.extensions.contains_key("anthropic_cache_control"));
    }

    #[test]
    fn role_serializes_lowercase() {
        let json = serde_json::to_string(&Role::Assistant).unwrap();
        assert_eq!(json, r#""assistant""#);
    }
}
