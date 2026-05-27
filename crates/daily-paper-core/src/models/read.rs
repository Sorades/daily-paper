use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadTask {
    pub run_id: String,
    pub paper_id: String,
    pub status: ReadTaskStatus,
    pub attempt_count: u32,
    pub max_attempts: u32,
    pub next_retry_after: Option<DateTime<Utc>>,
    pub cache_key: String,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReadTaskStatus {
    Pending,
    PdfFetched,
    TextExtracted,
    MetadataFetched,
    LlmDone,
    RetryableFailed,
    PermanentFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    pub paper_id: String,
    pub cache_key: String,
    pub generated_at: DateTime<Utc>,
    pub model_id: String,
    pub reader_template_hash: String,
    pub language: String,
    pub summary: String,
    pub metadata: PaperMetadataSummary,
    pub token_usage: Option<TokenUsage>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperMetadataSummary {
    pub institutions: Vec<String>,
    pub notable_authors: Vec<String>,
    pub project_url: Option<String>,
    pub code_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// Compute read-result cache key.
pub fn compute_read_cache_key(
    paper_id: &str,
    pdf_sha256: &str,
    text_extract_key: &str,
    metadata_key: &str,
    reader_template_hash: &str,
    llm_model_id: &str,
    language: &str,
) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(paper_id.as_bytes());
    hasher.update(pdf_sha256.as_bytes());
    hasher.update(text_extract_key.as_bytes());
    hasher.update(metadata_key.as_bytes());
    hasher.update(reader_template_hash.as_bytes());
    hasher.update(llm_model_id.as_bytes());
    hasher.update(language.as_bytes());
    hex::encode(hasher.finalize())
}
