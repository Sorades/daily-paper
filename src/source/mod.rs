pub mod arxiv;

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use crate::error::Result;
use crate::models::candidate::CandidatePaper;

/// Declares high-level capabilities supported by a paper source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCapability {
    /// Supports querying explicit time windows (e.g. arXiv RSS/Export, BioRxiv)
    TimeWindowFetch,
    /// Yields latest real-time feeds / snapshots without date filtering
    LatestSnapshotOnly,
    /// Yields full text / structured metadata directly
    FullMetadata,
}

/// Runtime context passed to a paper source during fetch.
#[derive(Debug, Clone)]
pub struct FetchContext {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub report_date: String,
    pub cursor: Option<String>,
}

/// Output returned by a PaperSource execution.
#[derive(Debug, Clone, Default)]
pub struct FetchOutput {
    pub candidates: Vec<CandidatePaper>,
    pub next_cursor: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Trait defining a pluggable scientific paper provider.
///
/// Any source (arXiv, HuggingFace Daily, BioRxiv, OpenReview, RSS) implements this trait.
pub trait PaperSource: Send + Sync {
    /// Identifier of the source (e.g. "arxiv", "huggingface", "biorxiv")
    fn id(&self) -> &'static str;

    /// Supported capabilities of this source
    fn capabilities(&self) -> &[SourceCapability];

    /// Fetch candidate papers within the given context
    fn fetch<'a>(
        &'a self,
        ctx: &'a FetchContext,
    ) -> Pin<Box<dyn Future<Output = Result<FetchOutput>> + Send + 'a>>;
}
