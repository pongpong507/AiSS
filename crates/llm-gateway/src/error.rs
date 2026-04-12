use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Provider error [{provider}]: {message}")]
    Provider { provider: String, message: String },

    #[error("Stream ended unexpectedly")]
    StreamEnded,

    #[error("Unsupported capability: {0}")]
    UnsupportedCapability(String),

    #[error("Context too long: {tokens} tokens exceeds limit {limit}")]
    ContextTooLong { tokens: usize, limit: usize },
}
