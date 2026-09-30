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

impl Error {
    /// Whether this error is transient and worth retrying.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Error::RetryableNetwork(_) | Error::RateLimited { .. })
    }

    /// Map domain Error to structural ErrorKind.
    pub fn kind(&self) -> crate::models::run::ErrorKind {
        match self {
            Self::Config(_) | Self::TomlParse(_) => crate::models::run::ErrorKind::Config,
            Self::Auth(_) => crate::models::run::ErrorKind::Auth,
            Self::RateLimited { .. } => crate::models::run::ErrorKind::RateLimited,
            Self::RetryableNetwork(_) => crate::models::run::ErrorKind::RetryableNetwork,
            Self::SourceUnavailable(_) => crate::models::run::ErrorKind::SourceUnavailable,
            Self::BadSourceData(_) => crate::models::run::ErrorKind::BadSourceData,
            Self::Storage(_) => crate::models::run::ErrorKind::Storage,
            Self::PdfDownload(_) => crate::models::run::ErrorKind::PdfDownload,
            Self::PdfExtract(_) => crate::models::run::ErrorKind::PdfExtract,
            Self::Embedding(_) => crate::models::run::ErrorKind::Embedding,
            Self::Llm(_) => crate::models::run::ErrorKind::Llm,
            Self::Render(_) => crate::models::run::ErrorKind::Render,
            Self::Delivery(_) => crate::models::run::ErrorKind::Delivery,
            Self::Http(e) => {
                if e.is_timeout() || e.is_connect() {
                    crate::models::run::ErrorKind::RetryableNetwork
                } else {
                    crate::models::run::ErrorKind::Storage
                }
            }
            Self::Serialization(_) => crate::models::run::ErrorKind::Storage,
        }
    }
}
