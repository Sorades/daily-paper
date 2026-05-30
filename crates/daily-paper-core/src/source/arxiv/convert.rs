use chrono::{DateTime, Utc};

use crate::models::candidate::{
    make_paper_id, normalize_arxiv_id, CandidatePaper, PaperSourceKind,
};
use crate::models::common::Author;

/// Convert a parsed arXiv entry (as JSON) to our internal CandidatePaper model.
pub fn arxmliv_entry_to_candidate(entry: &serde_json::Value) -> Option<CandidatePaper> {
    let id_url = entry.get("id")?.as_str()?;
    let arxiv_id = extract_arxiv_id_from_atom_id(id_url)?;
    let norm_arxiv_id = normalize_arxiv_id(&arxiv_id);

    let title = entry
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .replace('\n', " ")
        .to_string();

    let abstract_text = entry
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let authors = extract_authors(entry);
    let categories = extract_categories(entry);
    let published_at = parse_datetime(entry, "published");
    let updated_at = parse_datetime(entry, "updated");

    let landing_url = entry.get("link_alternate").and_then(|v| v.as_str()).map(String::from);
    let pdf_url = entry.get("link_related").and_then(|v| v.as_str()).map(String::from);

    // Extract DOI if present
    let doi = entry
        .get("doi")
        .and_then(|v| v.as_str())
        .map(String::from);

    let paper_id = make_paper_id(doi.as_deref(), Some(&norm_arxiv_id), &PaperSourceKind::Arxiv, &arxiv_id);

    Some(CandidatePaper {
        paper_id,
        source: PaperSourceKind::Arxiv,
        source_id: arxiv_id,
        title,
        abstract_text,
        authors,
        published_at,
        updated_at,
        categories,
        doi,
        arxiv_id: Some(norm_arxiv_id),
        landing_url,
        pdf_url,
        source_metadata: serde_json::json!({}),
    })
}

fn extract_arxiv_id_from_atom_id(id_url: &str) -> Option<String> {
    // ID format: http://arxiv.org/abs/2301.12345v1
    let id = id_url.rsplit('/').next()?;
    if id.is_empty() {
        return None;
    }
    Some(id.to_string())
}

fn extract_authors(entry: &serde_json::Value) -> Vec<Author> {
    entry
        .get("authors")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let name = a.get("name")?.as_str()?;
                    Some(Author {
                        name: name.to_string(),
                        normalized_name: None,
                        affiliation: None,
                        url: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_categories(entry: &serde_json::Value) -> Vec<String> {
    entry
        .get("categories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_datetime(entry: &serde_json::Value, field: &str) -> Option<DateTime<Utc>> {
    entry
        .get(field)
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn convert_basic_entry() {
        let entry = json!({
            "id": "http://arxiv.org/abs/2301.12345v1",
            "title": "Test Paper",
            "summary": "An abstract.",
            "authors": [{"name": "Alice Smith"}, {"name": "Bob Jones"}],
            "published": "2023-01-30T00:00:00Z",
            "updated": "2023-01-31T00:00:00Z",
            "categories": ["cs.AI", "cs.LG"],
            "link": "http://arxiv.org/abs/2301.12345v1"
        });

        let paper = arxmliv_entry_to_candidate(&entry).unwrap();
        assert_eq!(paper.paper_id, "arxiv:2301.12345");
        assert_eq!(paper.title, "Test Paper");
        assert_eq!(paper.authors.len(), 2);
        assert_eq!(paper.categories, vec!["cs.AI", "cs.LG"]);
        assert_eq!(paper.arxiv_id, Some("2301.12345".into()));
    }

    #[test]
    fn normalize_strips_version_from_source_id() {
        let entry = json!({
            "id": "http://arxiv.org/abs/2301.12345v3",
            "title": "Paper v3",
            "summary": "Abstract.",
            "authors": [],
            "categories": []
        });

        let paper = arxmliv_entry_to_candidate(&entry).unwrap();
        assert_eq!(paper.arxiv_id, Some("2301.12345".into()));
        // source_id keeps the original
        assert_eq!(paper.source_id, "2301.12345v3");
    }

    #[test]
    fn extract_pdf_url_from_link_related() {
        let entry = json!({
            "id": "http://arxiv.org/abs/2301.12345v1",
            "title": "Paper with PDF",
            "summary": "Abstract.",
            "authors": [],
            "categories": [],
            "link_alternate": "http://arxiv.org/abs/2301.12345v1",
            "link_related": "http://arxiv.org/pdf/2301.12345v1"
        });

        let paper = arxmliv_entry_to_candidate(&entry).unwrap();
        assert_eq!(paper.landing_url, Some("http://arxiv.org/abs/2301.12345v1".into()));
        assert_eq!(paper.pdf_url, Some("http://arxiv.org/pdf/2301.12345v1".into()));
    }

    #[test]
    fn no_pdf_url_when_missing() {
        let entry = json!({
            "id": "http://arxiv.org/abs/2301.12345v1",
            "title": "Paper without PDF",
            "summary": "Abstract.",
            "authors": [],
            "categories": [],
            "link_alternate": "http://arxiv.org/abs/2301.12345v1"
        });

        let paper = arxmliv_entry_to_candidate(&entry).unwrap();
        assert_eq!(paper.landing_url, Some("http://arxiv.org/abs/2301.12345v1".into()));
        assert_eq!(paper.pdf_url, None);
    }
}
