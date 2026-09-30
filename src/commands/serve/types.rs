use axum::http::StatusCode;
use axum::response::Json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(crate) struct RunRequest {
    pub(crate) stages: Option<Vec<String>>,
    pub(crate) from_run: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) dry_run: Option<bool>,
    pub(crate) force_zotero_sync: Option<bool>,
    pub(crate) force_embedding: Option<bool>,
    pub(crate) force_rerank: Option<bool>,
    pub(crate) force_read: Option<bool>,
    pub(crate) force_send: Option<bool>,
    pub(crate) max_candidates: Option<usize>,
    pub(crate) no_email: Option<bool>,
    pub(crate) send_email: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiMessage {
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct RunStartResponse {
    pub(crate) run_id: String,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct StatsDay {
    pub(crate) day: u32,
    pub(crate) total: usize,
    pub(crate) success: usize,
    pub(crate) failed: usize,
}

#[derive(Serialize)]
pub(crate) struct StatsResponse {
    pub(crate) year: i32,
    pub(crate) month: u32,
    pub(crate) days: Vec<StatsDay>,
}

#[derive(Serialize)]
pub(crate) struct RunSummary {
    pub(crate) run_id: String,
    pub(crate) status: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) report_exists: bool,
    pub(crate) error: Option<ErrorSummary>,
}

#[derive(Serialize)]
pub(crate) struct ErrorSummary {
    pub(crate) kind: String,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct DateResponse {
    pub(crate) date: String,
    pub(crate) runs: Vec<RunSummary>,
}

#[derive(Serialize)]
pub(crate) struct ConfigResponse {
    pub(crate) path: String,
    pub(crate) content: String,
}

#[derive(Deserialize)]
pub(crate) struct ConfigUpdateRequest {
    pub(crate) content: String,
}

#[derive(Serialize)]
pub(crate) struct CacheInfo {
    pub(crate) kind: String,
    pub(crate) size_bytes: u64,
    pub(crate) size_display: String,
    pub(crate) file_count: usize,
}

#[derive(Serialize)]
pub(crate) struct CacheResponse {
    pub(crate) caches: Vec<CacheInfo>,
}

#[derive(Serialize)]
pub(crate) struct CacheCleanResponse {
    pub(crate) message: String,
    pub(crate) kind: String,
    pub(crate) freed_bytes: u64,
}

#[derive(Deserialize)]
pub(crate) struct BulkDeleteRequest {
    pub(crate) run_ids: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct BulkDeleteResponse {
    pub(crate) message: String,
    pub(crate) count: usize,
}

#[derive(Serialize)]
pub(crate) struct StatusResponse {
    pub(crate) status: String,
    pub(crate) pipeline_running: bool,
}

pub(crate) type ApiError = (StatusCode, Json<ApiMessage>);

pub(crate) fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ApiMessage {
            message: message.into(),
        }),
    )
}
