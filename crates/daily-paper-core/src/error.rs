use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(String),

    #[error("auth failed: {0}")]
    Auth(String),

    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("retryable network error: {0}")]
    RetryableNetwork(String),

    #[error("source unavailable: {0}")]
    SourceUnavailable(String),

    #[error("bad source data: {0}")]
    BadSourceData(String),

    #[error("storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("PDF download failed: {0}")]
    PdfDownload(String),

    #[error("PDF extract failed: {0}")]
    PdfExtract(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("render error: {0}")]
    Render(String),

    #[error("delivery error: {0}")]
    Delivery(String),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
