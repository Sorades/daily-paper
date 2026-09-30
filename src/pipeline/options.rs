use crate::models::run::StageName;

/// Options controlling a pipeline execution run.
#[derive(Debug, Clone, Default)]
pub struct PipelineOptions {
    pub date: Option<String>,
    pub stages: Vec<StageName>,
    pub from_run: Option<String>,
    pub dry_run: bool,
    pub send_email: bool,
    pub no_email: bool,
    pub force_zotero_sync: bool,
    pub force_embedding: bool,
    pub force_rerank: bool,
    pub force_read: bool,
    pub force_send: bool,
    pub max_candidates: Option<usize>,
}
