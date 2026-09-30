use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfAsset {
    pub paper_id: String,
    pub url: String,
    pub file_path: String,
    pub sha256: String,
    pub downloaded_at: DateTime<Utc>,
    pub byte_len: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedText {
    pub text_extract_key: String,
    pub paper_id: String,
    pub pdf_sha256: String,
    pub extractor: String,
    pub extractor_config_hash: String,
    pub section_parser_version: String,
    pub extracted_at: DateTime<Utc>,
    pub text_path: String,
    pub text_sha256: String,
    pub sections: Vec<PaperSection>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaperSection {
    pub title: String,
    pub normalized_title: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Compute text_extract_key from inputs.
pub fn compute_text_extract_key(
    pdf_sha256: &str,
    extractor_id: &str,
    extractor_config_hash: &str,
    section_parser_version: &str,
) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(pdf_sha256.as_bytes());
    hasher.update(extractor_id.as_bytes());
    hasher.update(extractor_config_hash.as_bytes());
    hasher.update(section_parser_version.as_bytes());
    hex::encode(hasher.finalize())
}
