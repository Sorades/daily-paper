use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Paper metadata gathered before deep reading.
///
/// MVP version: only extracts from arXiv API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperMetadata {
    pub metadata_key: String,
    pub paper_id: String,
    pub fetched_at: DateTime<Utc>,
    pub institutions: Vec<String>,
    pub notable_authors: Vec<String>,
    pub project_url: Option<String>,
    pub code_url: Option<String>,
    pub homepage_urls: Vec<String>,
    pub warnings: Vec<String>,
}

/// Compute metadata cache key.
pub fn compute_metadata_key(
    paper_id: &str,
    source_metadata_hash: &str,
    text_extract_key: &str,
    metadata_config_hash: &str,
) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(paper_id.as_bytes());
    hasher.update(source_metadata_hash.as_bytes());
    hasher.update(text_extract_key.as_bytes());
    hasher.update(metadata_config_hash.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extract metadata from arXiv source metadata.
///
/// For MVP, we only extract what's already available in the arXiv API response.
/// No PDF parsing, no institution detection, no code link scraping.
pub fn extract_from_source(
    paper_id: &str,
    source_metadata: &serde_json::Value,
) -> PaperMetadata {
    let mut institutions = Vec::new();
    let mut notable_authors = Vec::new();
    let mut homepage_urls = Vec::new();
    let warnings = Vec::new();

    // Extract affiliations from source metadata if present
    if let Some(affiliations) = source_metadata.get("affiliations").and_then(|v| v.as_array()) {
        for aff in affiliations {
            if let Some(s) = aff.as_str() {
                institutions.push(s.to_string());
            }
        }
    }

    // Extract author info
    if let Some(authors) = source_metadata.get("authors").and_then(|v| v.as_array()) {
        for author in authors {
            if let Some(name) = author.get("name").and_then(|v| v.as_str()) {
                notable_authors.push(name.to_string());
            }
            if let Some(url) = author.get("homepage").and_then(|v| v.as_str()) {
                homepage_urls.push(url.to_string());
            }
        }
    }

    let metadata_key = compute_metadata_key(
        paper_id,
        &hash_json(source_metadata),
        "", // no text extract yet
        "v1",
    );

    PaperMetadata {
        metadata_key,
        paper_id: paper_id.to_string(),
        fetched_at: Utc::now(),
        institutions,
        notable_authors,
        project_url: None,
        code_url: None,
        homepage_urls,
        warnings,
    }
}

fn hash_json(value: &serde_json::Value) -> String {
    use sha2::Digest;
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    hex::encode(sha2::Sha256::digest(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_basic_metadata() {
        let source = json!({
            "authors": [
                {"name": "Alice Smith"},
                {"name": "Bob Jones"}
            ]
        });

        let meta = extract_from_source("arxiv:2301.12345", &source);
        assert_eq!(meta.paper_id, "arxiv:2301.12345");
        assert_eq!(meta.notable_authors.len(), 2);
        assert!(!meta.metadata_key.is_empty());
    }

    #[test]
    fn extract_empty_source() {
        let source = json!({});
        let meta = extract_from_source("arxiv:2301.12345", &source);
        assert!(meta.institutions.is_empty());
        assert!(meta.notable_authors.is_empty());
    }
}
