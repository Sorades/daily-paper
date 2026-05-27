use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestProfile {
    pub profile_id: String,
    pub zotero_snapshot_id: String,
    pub created_at: DateTime<Utc>,
    pub rules_hash: String,
    pub paper_count: usize,
    pub total_weight: f32,
    pub papers: Vec<InterestPaperRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestPaperRef {
    pub library_id: String,
    pub weight: f32,
    pub matched_rules: Vec<String>,
}
