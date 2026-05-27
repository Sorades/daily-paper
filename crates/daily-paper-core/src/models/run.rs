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
