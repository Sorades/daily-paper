use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::DateWindow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: String,
    pub parent_run_id: Option<String>,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub config_hash: String,
    pub cli_overrides: Vec<CliOverride>,
    pub date_window: DateWindow,
    pub stages: Vec<StageRecord>,
    pub warnings: Vec<WarningRecord>,
    pub error: Option<ErrorRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliOverride {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRecord {
    pub stage: StageName,
    pub status: StageStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub cache_hit: bool,
    pub input_hash: Option<String>,
    pub output_ref: Option<String>,
    pub error: Option<ErrorRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StageName {
    ZoteroSync,
    SourceFetch,
    Deduplicate,
    Embedding,
    Rerank,
    PdfFetch,
    TextExtract,
    MetadataFetch,
    DeepRead,
    Render,
    Send,
}

impl StageName {
    /// Convert from kebab-case string (e.g. "zotero-sync" → ZoteroSync).
    pub fn from_kebab(s: &str) -> Option<Self> {
        match s {
            "zotero-sync" => Some(Self::ZoteroSync),
            "source-fetch" => Some(Self::SourceFetch),
            "deduplicate" => Some(Self::Deduplicate),
            "embedding" => Some(Self::Embedding),
            "rerank" => Some(Self::Rerank),
            "pdf-fetch" => Some(Self::PdfFetch),
            "text-extract" => Some(Self::TextExtract),
            "metadata-fetch" => Some(Self::MetadataFetch),
            "deep-read" => Some(Self::DeepRead),
            "render" => Some(Self::Render),
            "send" => Some(Self::Send),
            _ => None,
        }
    }

    /// Convert to kebab-case string.
    pub fn to_kebab(&self) -> &'static str {
        match self {
            Self::ZoteroSync => "zotero-sync",
            Self::SourceFetch => "source-fetch",
            Self::Deduplicate => "deduplicate",
            Self::Embedding => "embedding",
            Self::Rerank => "rerank",
            Self::PdfFetch => "pdf-fetch",
            Self::TextExtract => "text-extract",
            Self::MetadataFetch => "metadata-fetch",
            Self::DeepRead => "deep-read",
            Self::Render => "render",
            Self::Send => "send",
        }
    }

    /// All stages in pipeline execution order.
    pub fn all() -> &'static [StageName] {
        &[
            Self::ZoteroSync,
            Self::SourceFetch,
            Self::Deduplicate,
            Self::Embedding,
            Self::Rerank,
            Self::DeepRead,
            Self::Render,
            Self::Send,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StageStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    pub kind: ErrorKind,
    pub message: String,
    pub retryable: bool,
    pub context: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorKind {
    Config,
    Auth,
    RateLimited,
    RetryableNetwork,
    SourceUnavailable,
    BadSourceData,
    Storage,
    PdfDownload,
    PdfExtract,
    Embedding,
    Llm,
    Render,
    Delivery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarningRecord {
    pub kind: String,
    pub message: String,
    pub context: serde_json::Value,
}

/// Real-time pipeline event for SSE streaming.
#[derive(Debug, Clone, Serialize)]
pub enum PipelineEvent {
    Started {
        run_id: String,
    },
    StageStart {
        run_id: String,
        stage: StageName,
    },
    StageEnd {
        run_id: String,
        stage: StageName,
        status: StageStatus,
        cache_hit: bool,
        duration_ms: u64,
    },
    Ended {
        run_id: String,
        status: RunStatus,
    },
}
