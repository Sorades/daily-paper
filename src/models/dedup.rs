use serde::{Deserialize, Serialize};

use super::candidate::CandidatePaper;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupResult {
    pub run_id: String,
    pub candidates: Vec<CandidatePaper>,
    pub duplicates: Vec<DuplicateRecord>,
    pub skipped_existing: Vec<ExistingLibraryMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateRecord {
    pub kept_paper_id: String,
    pub dropped_paper_id: String,
    pub reason: DuplicateReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingLibraryMatch {
    pub candidate_paper_id: String,
    pub library_id: String,
    pub reason: DuplicateReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DuplicateReason {
    SameDoi,
    SameArxivId,
    SameSourceId,
    SameNormalizedTitle,
}
