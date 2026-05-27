use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::Author;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatePaper {
    pub paper_id: String,
    pub source: PaperSourceKind,
    pub source_id: String,
    pub title: String,
    pub abstract_text: String,
    pub authors: Vec<Author>,
    pub published_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub categories: Vec<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub landing_url: Option<String>,
    pub pdf_url: Option<String>,
    pub source_metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PaperSourceKind {
    Arxiv,
    BioRxiv,
    MedRxiv,
    Other(String),
}

impl std::fmt::Display for PaperSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arxiv => write!(f, "arxiv"),
            Self::BioRxiv => write!(f, "biorxiv"),
            Self::MedRxiv => write!(f, "medrxiv"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// Normalize a DOI: strip URL prefix, lowercase, trim trailing slash.
pub fn normalize_doi(doi: &str) -> String {
    let doi = doi.trim();
    let doi = doi
        .strip_prefix("https://doi.org/")
        .or_else(|| doi.strip_prefix("http://doi.org/"))
        .or_else(|| doi.strip_prefix("DOI:"))
        .or_else(|| doi.strip_prefix("doi:"))
        .unwrap_or(doi);
    let doi = doi.trim().trim_end_matches('/');
    doi.to_lowercase()
}

/// Normalize an arXiv ID: strip version suffix (e.g., 2301.12345v3 -> 2301.12345).
pub fn normalize_arxiv_id(id: &str) -> String {
    let id = id.trim();
    // Strip arxiv: prefix if present
    let id = id.strip_prefix("arxiv:").unwrap_or(id);
    // Strip version suffix (v1, v2, etc.)
    if let Some(pos) = id.rfind('v') {
        // Make sure what follows 'v' is a number
        let suffix = &id[pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return id[..pos].to_string();
        }
    }
    id.to_string()
}

/// Generate a stable paper_id from paper metadata.
///
/// Priority: DOI > arXiv ID > source fallback.
pub fn make_paper_id(
    doi: Option<&str>,
    arxiv_id: Option<&str>,
    source: &PaperSourceKind,
    source_id: &str,
) -> String {
    if let Some(doi) = doi {
        let norm = normalize_doi(doi);
        if !norm.is_empty() {
            return format!("doi:{}", norm);
        }
    }
    if let Some(arxiv_id) = arxiv_id {
        let norm = normalize_arxiv_id(arxiv_id);
        if !norm.is_empty() {
            return format!("arxiv:{}", norm);
        }
    }
    format!("source:{}:{}", source, source_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_doi_strips_prefix() {
        assert_eq!(normalize_doi("https://doi.org/10.1234/ABC"), "10.1234/abc");
        assert_eq!(normalize_doi("http://doi.org/10.1234/ABC"), "10.1234/abc");
        assert_eq!(normalize_doi("DOI: 10.1234/ABC"), "10.1234/abc");
        assert_eq!(normalize_doi("10.1234/ABC"), "10.1234/abc");
    }

    #[test]
    fn normalize_doi_trims_slash() {
        assert_eq!(normalize_doi("10.1234/abc/"), "10.1234/abc");
    }

    #[test]
    fn normalize_arxiv_id_strips_version() {
        assert_eq!(normalize_arxiv_id("2301.12345v3"), "2301.12345");
        assert_eq!(normalize_arxiv_id("2301.12345"), "2301.12345");
        assert_eq!(normalize_arxiv_id("arxiv:2301.12345v1"), "2301.12345");
    }

    #[test]
    fn make_paper_id_priority() {
        assert_eq!(
            make_paper_id(Some("10.1234/abc"), Some("2301.12345"), &PaperSourceKind::Arxiv, "1"),
            "doi:10.1234/abc"
        );
        assert_eq!(
            make_paper_id(None, Some("2301.12345"), &PaperSourceKind::Arxiv, "1"),
            "arxiv:2301.12345"
        );
        assert_eq!(
            make_paper_id(None, None, &PaperSourceKind::Arxiv, "12345"),
            "source:arxiv:12345"
        );
    }
}
