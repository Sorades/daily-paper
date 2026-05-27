use crate::models::common::Author;
use crate::models::zotero::{LibraryPaper, LibraryType, ZoteroAttachmentRef};

/// Convert a raw Zotero API JSON item to our internal LibraryPaper model.
pub fn zotero_item_to_library_paper(item: &serde_json::Value) -> Option<LibraryPaper> {
    let data = item.get("data")?;
    let key = data.get("key")?.as_str()?;
    let item_type = data.get("itemType")?.as_str()?;

    // Skip attachment and note items
    if item_type == "attachment" || item_type == "note" {
        return None;
    }

    let version = data.get("version").and_then(|v| v.as_u64());
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let abstract_text = data.get("abstractNote").and_then(|v| v.as_str()).map(String::from);
    let year = data
        .get("date")
        .and_then(|v| v.as_str())
        .and_then(extract_year);
    let doi = data.get("DOI").and_then(|v| v.as_str()).map(String::from);
    let url = data.get("url").and_then(|v| v.as_str()).map(String::from);

    // Extract arXiv ID from various fields
    let arxiv_id = data
        .get("arXiv")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            data.get("url")
                .and_then(|v| v.as_str())
                .and_then(extract_arxiv_id_from_url)
        });

    let authors = extract_authors(data);
    let tags = extract_tags(data);
    let attachments = extract_attachments(data);
    let collection_keys = extract_collection_keys(data);
    let is_trashed = data.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false);
    let parent_item = data
        .get("parentItem")
        .and_then(|v| v.as_str())
        .map(String::from);
    let date_added = data
        .get("dateAdded")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let date_modified = data
        .get("dateModified")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let version_key = item
        .get("version")
        .and_then(|v| v.as_u64())
        .or(version);

    Some(LibraryPaper {
        library_id: key.to_string(),
        library_type: LibraryType::User,
        zotero_key: key.to_string(),
        version: version_key,
        item_type: item_type.to_string(),
        parent_item,
        title,
        abstract_text,
        authors,
        year,
        doi,
        arxiv_id,
        url,
        collection_keys,
        collections: Vec::new(), // filled later by collection path builder
        tags,
        is_trashed,
        date_added,
        date_modified,
        attachments,
    })
}

fn extract_year(date_str: &str) -> Option<i32> {
    // Zotero dates can be "2024", "2024-01-15", "January 2024", etc.
    // Try to find a 4-digit year
    for part in date_str.split(|c: char| !c.is_ascii_digit()) {
        if part.len() == 4 {
            if let Ok(year) = part.parse::<i32>() {
                if 1900 <= year && year <= 2100 {
                    return Some(year);
                }
            }
        }
    }
    None
}

fn extract_authors(data: &serde_json::Value) -> Vec<Author> {
    let Some(creators) = data.get("creators").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    creators
        .iter()
        .filter_map(|c| {
            let creator_type = c.get("creatorType").and_then(|v| v.as_str()).unwrap_or("author");
            if creator_type != "author" && creator_type != "editor" {
                return None;
            }

            let name = if let (Some(first), Some(last)) = (
                c.get("firstName").and_then(|v| v.as_str()),
                c.get("lastName").and_then(|v| v.as_str()),
            ) {
                format!("{} {}", first, last)
            } else if let Some(name) = c.get("name").and_then(|v| v.as_str()) {
                name.to_string()
            } else {
                return None;
            };

            Some(Author {
                name,
                normalized_name: None,
                affiliation: None,
                url: None,
            })
        })
        .collect()
}

fn extract_tags(data: &serde_json::Value) -> Vec<String> {
    data.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("tag").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_attachments(data: &serde_json::Value) -> Vec<ZoteroAttachmentRef> {
    data.get("attachments")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    Some(ZoteroAttachmentRef {
                        key: a.get("key")?.as_str()?.to_string(),
                        title: a.get("title").and_then(|v| v.as_str()).map(String::from),
                        content_type: a
                            .get("contentType")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        url: a.get("url").and_then(|v| v.as_str()).map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_collection_keys(data: &serde_json::Value) -> Vec<String> {
    data.get("collections")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_arxiv_id_from_url(url: &str) -> Option<String> {
    // Handle URLs like https://arxiv.org/abs/2301.12345
    if let Some(pos) = url.find("arxiv.org/abs/") {
        let id_start = pos + "arxiv.org/abs/".len();
        let id = &url[id_start..];
        let id = id.split('/').next().unwrap_or(id);
        let id = id.split('?').next().unwrap_or(id);
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn convert_basic_item() {
        let item = json!({
            "key": "ABC123",
            "version": 42,
            "data": {
                "key": "ABC123",
                "version": 42,
                "itemType": "journalArticle",
                "title": "Test Paper",
                "abstractNote": "A test abstract",
                "date": "2024-01-15",
                "DOI": "10.1234/test",
                "creators": [
                    {"creatorType": "author", "firstName": "Alice", "lastName": "Smith"}
                ],
                "tags": [{"tag": "ml"}, {"tag": "nlp"}],
                "collections": ["COL1"],
                "deleted": false
            }
        });

        let paper = zotero_item_to_library_paper(&item).unwrap();
        assert_eq!(paper.title, "Test Paper");
        assert_eq!(paper.doi, Some("10.1234/test".into()));
        assert_eq!(paper.year, Some(2024));
        assert_eq!(paper.authors.len(), 1);
        assert_eq!(paper.authors[0].name, "Alice Smith");
        assert_eq!(paper.tags, vec!["ml", "nlp"]);
        assert!(!paper.is_trashed);
    }

    #[test]
    fn skip_attachment_items() {
        let item = json!({
            "data": {
                "key": "ATT1",
                "itemType": "attachment",
                "title": "pdf"
            }
        });
        assert!(zotero_item_to_library_paper(&item).is_none());
    }

    #[test]
    fn extract_year_from_various_formats() {
        assert_eq!(extract_year("2024"), Some(2024));
        assert_eq!(extract_year("2024-01-15"), Some(2024));
        assert_eq!(extract_year("January 2024"), Some(2024));
        assert_eq!(extract_year("no year"), None);
    }

    #[test]
    fn extract_arxiv_id_from_url() {
        assert_eq!(
            super::extract_arxiv_id_from_url("https://arxiv.org/abs/2301.12345"),
            Some("2301.12345".into())
        );
        assert_eq!(
            super::extract_arxiv_id_from_url("https://arxiv.org/abs/2301.12345v2"),
            Some("2301.12345v2".into())
        );
        assert_eq!(super::extract_arxiv_id_from_url("https://example.com"), None);
    }
}
