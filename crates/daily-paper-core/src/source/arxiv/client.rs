use chrono::{DateTime, Utc};
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::models::candidate::CandidatePaper;

use super::convert::arxmliv_entry_to_candidate;

const ARXIV_API_BASE: &str = "http://export.arxiv.org/api/query";

pub struct ArxivClient {
    client: Client,
    base_url: String,
    categories: Vec<String>,
    include_cross_list: bool,
    _max_results: usize,
}

impl ArxivClient {
    pub fn new(categories: Vec<String>, include_cross_list: bool) -> Self {
        Self {
            client: Client::new(),
            base_url: ARXIV_API_BASE.to_string(),
            categories,
            include_cross_list,
            _max_results: 2000,
        }
    }

    /// Override the base URL (for testing with mock servers).
    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.trim_end_matches('/').to_string();
        self
    }

    /// Fetch papers published in the given date window.
    pub async fn fetch(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CandidatePaper>> {
        let mut all_papers = Vec::new();

        for category in &self.categories {
            let papers = self.fetch_category(category, start, end).await?;
            all_papers.extend(papers);
        }

        // Dedup by paper_id within the result set
        all_papers.sort_by(|a, b| a.paper_id.cmp(&b.paper_id));
        all_papers.dedup_by(|a, b| a.paper_id == b.paper_id);

        debug!(count = all_papers.len(), "fetched arXiv papers");
        Ok(all_papers)
    }

    async fn fetch_category(
        &self,
        category: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CandidatePaper>> {
        let mut all_papers = Vec::new();
        let batch_size = 100usize;
        let mut offset = 0usize;

        loop {
            let query = if self.include_cross_list {
                format!("cat:{}", category)
            } else {
                format!(
                    "cat:{} AND submittedDate:[{} TO {}]",
                    category,
                    start.format("%Y%m%d%H%M"),
                    end.format("%Y%m%d%H%M")
                )
            };

            let url = format!(
                "{}?search_query={}&start={}&max_results={}&sortBy=submittedDate&sortOrder=descending",
                self.base_url,
                urlencoding::encode(&query),
                offset,
                batch_size
            );

            debug!(url = %url, "fetching arXiv papers");

            let xml = self.fetch_with_retry(&url).await?;
            let entries = parse_arxiv_feed(&xml)?;

            let is_last = entries.len() < batch_size;
            for entry in &entries {
                if let Some(paper) = arxmliv_entry_to_candidate(entry) {
                    all_papers.push(paper);
                }
            }

            if is_last {
                break;
            }
            offset += batch_size;

            // Respect arXiv rate limit (between requests)
            tokio::time::sleep(Duration::from_secs(3)).await;
        }

        Ok(all_papers)
    }

    async fn fetch_with_retry(&self, url: &str) -> Result<String> {
        let max_retries = 3;
        let mut last_err = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay = Duration::from_secs(5 * 2u64.pow(attempt));
                warn!(
                    attempt,
                    delay_secs = delay.as_secs(),
                    "retrying arXiv request"
                );
                tokio::time::sleep(delay).await;
            }

            let resp = self
                .client
                .get(url)
                .timeout(Duration::from_secs(30))
                .send()
                .await
                .map_err(|e| Error::RetryableNetwork(e.to_string()))?;

            if resp.status().as_u16() == 503 {
                // arXiv uses 503 for rate limiting with Retry-After
                let retry_after = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(30);
                warn!(retry_after, "arXiv rate limited (503)");
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                last_err = Some(Error::RateLimited {
                    retry_after_secs: retry_after,
                });
                continue;
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(Error::SourceUnavailable(format!(
                    "arXiv API returned {}: {}",
                    status, body
                )));
            }

            return resp.text().await.map_err(|e| {
                Error::RetryableNetwork(format!("failed to read arXiv response: {}", e))
            });
        }

        Err(last_err.unwrap_or_else(|| Error::RetryableNetwork("max retries exceeded".into())))
    }
}

/// Parse arXiv Atom feed XML into raw entry data.
fn parse_arxiv_feed(xml: &str) -> Result<Vec<serde_json::Value>> {
    // We parse the XML and convert to a simplified JSON structure
    // This uses quick-xml for parsing
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut current_entry: Option<serde_json::Map<String, serde_json::Value>> = None;
    let mut _current_tag = String::new();
    let mut current_text = String::new();
    let mut in_entry = false;
    let mut authors: Vec<serde_json::Value> = Vec::new();
    let mut in_author = false;
    let mut in_name = false;
    let mut in_affiliation = false;
    let mut author_name = String::new();
    let mut author_affiliation = String::new();
    let mut categories: Vec<String> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "entry" => {
                        in_entry = true;
                        current_entry = Some(serde_json::Map::new());
                        authors = Vec::new();
                        categories = Vec::new();
                    }
                    "author" if in_entry => {
                        in_author = true;
                        author_name = String::new();
                        author_affiliation = String::new();
                    }
                    "name" if in_author => {
                        in_name = true;
                        current_text = String::new();
                    }
                    "affiliation" if in_author => {
                        in_affiliation = true;
                        current_text = String::new();
                    }
                    _ if in_entry => {
                        _current_tag = tag;
                        current_text = String::new();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                current_text = e.unescape().unwrap_or_default().to_string();
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "entry" => {
                        if let Some(ref mut entry) = current_entry {
                            entry.insert(
                                "authors".into(),
                                serde_json::Value::Array(authors.clone()),
                            );
                            entry.insert(
                                "categories".into(),
                                serde_json::Value::Array(
                                    categories
                                        .iter()
                                        .map(|c| serde_json::Value::String(c.clone()))
                                        .collect(),
                                ),
                            );
                        }
                        if let Some(entry) = current_entry.take() {
                            entries.push(serde_json::Value::Object(entry));
                        }
                        in_entry = false;
                    }
                    "author" if in_entry => {
                        in_author = false;
                        in_name = false;
                        in_affiliation = false;
                        if !author_name.is_empty() {
                            let mut author_json = serde_json::json!({"name": author_name});
                            if !author_affiliation.is_empty() {
                                author_json["affiliation"] =
                                    serde_json::Value::String(author_affiliation.clone());
                            }
                            authors.push(author_json);
                        }
                    }
                    "name" if in_author => {
                        in_name = false;
                    }
                    "affiliation" if in_author => {
                        in_affiliation = false;
                    }
                    "category" if in_entry => {
                        // Extract from attributes
                    }
                    _ if in_entry && !in_author => {
                        if let Some(ref mut entry) = current_entry {
                            entry.insert(
                                tag.into(),
                                serde_json::Value::String(current_text.clone()),
                            );
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if in_entry {
                    match tag.as_str() {
                        "category" => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "term" {
                                    categories.push(val);
                                }
                            }
                        }
                        "link" => {
                            let mut href = String::new();
                            let mut rel = String::new();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "href" => href = val,
                                    "rel" => rel = val,
                                    _ => {}
                                }
                            }
                            if !href.is_empty() {
                                if let Some(ref mut entry) = current_entry {
                                    // Store as "link_alternate" or "link_related"
                                    let field = format!("link_{}", rel);
                                    entry.insert(field, serde_json::Value::String(href));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }

        // Handle text accumulation for name and affiliation
        if in_name && !current_text.is_empty() && in_entry {
            author_name.push_str(&current_text);
        }
        if in_affiliation && !current_text.is_empty() && in_entry {
            if !author_affiliation.is_empty() {
                author_affiliation.push(' ');
            }
            author_affiliation.push_str(&current_text);
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2301.12345v1</id>
    <title>Test Paper Title</title>
    <summary>A test abstract about machine learning.</summary>
    <author><name>Alice Smith</name></author>
    <author><name>Bob Jones</name></author>
    <published>2023-01-30T00:00:00Z</published>
    <updated>2023-01-31T00:00:00Z</updated>
    <category term="cs.AI"/>
    <category term="cs.LG"/>
    <link href="http://arxiv.org/abs/2301.12345v1" rel="alternate"/>
    <link title="pdf" href="http://arxiv.org/pdf/2301.12345v1" rel="related"/>
  </entry>
</feed>"#;

        let entries = parse_arxiv_feed(xml).unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry["title"], "Test Paper Title");
        assert_eq!(entry["summary"], "A test abstract about machine learning.");
    }

    #[test]
    fn parse_authors_correctly() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2301.12345v1</id>
    <title>Test</title>
    <summary>Abstract</summary>
    <author><name>Alice Smith</name><arxiv:affiliation>MIT</arxiv:affiliation></author>
    <author><name>Bob Jones</name></author>
  </entry>
</feed>"#;

        let entries = parse_arxiv_feed(xml).unwrap();
        let entry = &entries[0];
        let authors = entry["authors"].as_array().unwrap();
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0]["name"], "Alice Smith");
        assert_eq!(authors[1]["name"], "Bob Jones");
        // Affiliation should NOT be included in the name
        assert!(!authors[0]["name"].as_str().unwrap().contains("MIT"));
    }
}
