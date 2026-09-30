use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Time window for fetching candidate papers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub label: String,
}

/// Content hash for cache invalidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentHash {
    pub algorithm: String,
    pub value: String,
}

/// Paper author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub normalized_name: Option<String>,
    pub affiliation: Option<String>,
    pub url: Option<String>,
}

/// Compute a SHA-256 hex digest of the input bytes.
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(data);
    hex::encode(hash)
}
