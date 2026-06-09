use chrono::{DateTime, Utc};
use reqwest::Client;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::models::candidate::CandidatePaper;

use super::convert::arxmliv_entry_to_candidate;

const ARXIV_RSS_BASE: &str = "https://rss.arxiv.org/rss";
const ARXIV_EXPORT_BASE: &str = "https://export.arxiv.org/api/query";
const USER_AGENT: &str =
    "daily-paper/0.1 (https://github.com/user/daily-paper; mailto:user@example.com)";
const ARXIV_MIN_INTERVAL: Duration = Duration::from_secs(3);
static ARXIV_LAST_REQUEST: OnceLock<tokio::sync::Mutex<Option<Instant>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArxivBackendKind {
    Rss,
    Export,
}

impl ArxivBackendKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rss" => Some(Self::Rss),
            "export" => Some(Self::Export),
            _ => None,
        }
    }
}

pub struct ArxivClient {
    client: Client,
    rss_base_url: String,
    export_base_url: String,
    backend: ArxivBackendKind,
    categories: Vec<String>,
    include_cross_list: bool,
    max_results_per_page: usize,
    max_pages: usize,
}

impl ArxivClient {
    pub fn new(categories: Vec<String>, include_cross_list: bool) -> Self {
        Self::with_backend(
            ArxivBackendKind::Rss,
            categories,
            include_cross_list,
            1000,
            3,
        )
    }

    pub fn with_backend(
        backend: ArxivBackendKind,
        categories: Vec<String>,
        include_cross_list: bool,
        max_results_per_page: usize,
        max_pages: usize,
    ) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            rss_base_url: ARXIV_RSS_BASE.to_string(),
            export_base_url: ARXIV_EXPORT_BASE.to_string(),
            backend,
            categories,
            include_cross_list,
            max_results_per_page,
            max_pages,
        }
    }

    /// Override the base URL (for testing with mock servers).
    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.rss_base_url = base_url.trim_end_matches('/').to_string();
        self
    }

    /// Override the export API URL (for testing with mock servers).
    pub fn with_export_base_url(mut self, base_url: &str) -> Self {
        self.export_base_url = base_url.to_string();
        self
    }

    /// Fetch papers for the configured backend.
    ///
    /// For the Export backend, `submittedDate` in the arXiv API is submission
    /// date, NOT announcement date. A paper submitted on day X may be announced
    /// on day X+1. To avoid missing papers, Export always fetches the latest
    /// results (no date filter) and lets the caller filter.
    ///
    /// RSS backend filters by `pubDate` (announcement date) directly.
    pub async fn fetch(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CandidatePaper>> {
        match self.backend {
            ArxivBackendKind::Rss => self.fetch_rss(start, end).await,
            // Export: always fetch latest; submittedDate ≠ announcement date
            ArxivBackendKind::Export => self.fetch_export(None).await,
        }
    }

    /// Fetch the latest available items without applying a submittedDate window.
    pub async fn fetch_latest(&self) -> Result<Vec<CandidatePaper>> {
        match self.backend {
            ArxivBackendKind::Rss => {
                let now = Utc::now();
                self.fetch_rss(
                    now - chrono::Duration::days(14),
                    now + chrono::Duration::days(1),
                )
                .await
            }
            ArxivBackendKind::Export => self.fetch_export(None).await,
        }
    }

    async fn fetch_rss(
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
        let url = format!("{}/{}", self.rss_base_url, category);
        debug!(url = %url, "fetching arXiv RSS");

        let xml = self.fetch_with_retry(&url).await?;
        let entries = parse_arxiv_rss_feed(&xml, category, self.include_cross_list, start, end)?;

        Ok(entries
            .iter()
            .filter_map(arxmliv_entry_to_candidate)
            .collect())
    }

    pub async fn fetch_export(
        &self,
        date_window: Option<(DateTime<Utc>, DateTime<Utc>)>,
    ) -> Result<Vec<CandidatePaper>> {
        let mut all_entries = Vec::new();
        let mut start_index = 0usize;
        let date_filtered = date_window.is_some();

        for page in 0..self.max_pages {
            let url = self.build_export_url(date_window, start_index);
            debug!(url = %url, page, "fetching arXiv export API");
            let xml = self.fetch_with_retry(&url).await?;
            let feed = parse_arxiv_atom_feed(&xml)?;
            let raw_count = feed.entries.len();
            let total_results = feed.total_results.unwrap_or(start_index + raw_count);
            all_entries.extend(feed.entries);

            if raw_count == 0 || start_index + raw_count >= total_results {
                break;
            }

            start_index += self.max_results_per_page;
            if page + 1 == self.max_pages {
                if !date_filtered {
                    warn!(
                        max_pages = self.max_pages,
                        total_fetched = all_entries.len(),
                        "arXiv export query hit page limit without date filter; \
                         results may be incomplete"
                    );
                    break;
                }
                return Err(Error::SourceUnavailable(format!(
                    "arXiv export query exceeded configured page limit \
                     (max_pages={}, total_results={}); narrow categories, \
                     reduce the date window, or increase max_pages",
                    self.max_pages, total_results
                )));
            }
        }

        let mut papers: Vec<CandidatePaper> = all_entries
            .iter()
            .filter(|entry| self.export_entry_matches_category(entry))
            .filter_map(arxmliv_entry_to_candidate)
            .collect();
        papers.sort_by(|a, b| a.paper_id.cmp(&b.paper_id));
        papers.dedup_by(|a, b| a.paper_id == b.paper_id);

        debug!(count = papers.len(), "fetched arXiv export papers");
        Ok(papers)
    }

    fn build_export_url(
        &self,
        date_window: Option<(DateTime<Utc>, DateTime<Utc>)>,
        start_index: usize,
    ) -> String {
        let category_query = self
            .categories
            .iter()
            .map(|category| format!("cat:{}", category))
            .collect::<Vec<_>>()
            .join(" OR ");
        let (query, sort_order) = if let Some((start, end)) = date_window {
            (
                format!(
                    "({}) AND submittedDate:[{} TO {}]",
                    category_query,
                    start.format("%Y%m%d%H%M"),
                    end.format("%Y%m%d%H%M")
                ),
                "ascending",
            )
        } else {
            (format!("({})", category_query), "descending")
        };
        format!(
            "{}?search_query={}&sortBy=submittedDate&sortOrder={}&start={}&max_results={}",
            self.export_base_url,
            urlencoding::encode(&query),
            sort_order,
            start_index,
            self.max_results_per_page
        )
    }

    fn export_entry_matches_category(&self, entry: &serde_json::Value) -> bool {
        if self.include_cross_list {
            return true;
        }

        // Prefer primary_category (set by arXiv Atom feed) over categories array
        if let Some(primary) = entry.get("primary_category").and_then(|v| v.as_str()) {
            return self.categories.iter().any(|category| category == primary);
        }

        entry
            .get("categories")
            .and_then(|v| v.as_array())
            .and_then(|categories| categories.first())
            .and_then(|v| v.as_str())
            .map(|primary| self.categories.iter().any(|category| category == primary))
            .unwrap_or(false)
    }

    async fn fetch_with_retry(&self, url: &str) -> Result<String> {
        let max_retries = 5; // More retries for rate limiting
        let mut last_err = None;

        for attempt in 0..=max_retries {
            if attempt > 0 && !matches!(last_err, Some(Error::RateLimited { .. })) {
                // Only apply exponential backoff if not rate limited
                let delay = Duration::from_secs(5 * 2u64.pow(attempt));
                warn!(
                    attempt,
                    delay_secs = delay.as_secs(),
                    "retrying arXiv request"
                );
                tokio::time::sleep(delay).await;
            }

            throttle_arxiv_request().await;

            let resp = self
                .client
                .get(url)
                .timeout(Duration::from_secs(30))
                .send()
                .await
                .map_err(|e| Error::RetryableNetwork(e.to_string()))?;

            let status = resp.status().as_u16();

            // Handle rate limiting: 429 (Too Many Requests) and 503 (Service Unavailable)
            if status == 429 || status == 503 {
                let retry_after = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60); // Default to 60s if header missing
                warn!(status, retry_after, "arXiv rate limited");
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                last_err = Some(Error::RateLimited {
                    retry_after_secs: retry_after,
                });
                continue;
            }

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(Error::SourceUnavailable(format!(
                    "arXiv RSS returned {}: {}",
                    status, body
                )));
            }

            return resp.text().await.map_err(|e| {
                Error::RetryableNetwork(format!("failed to read arXiv RSS response: {}", e))
            });
        }

        Err(last_err.unwrap_or_else(|| Error::RetryableNetwork("max retries exceeded".into())))
    }
}

async fn throttle_arxiv_request() {
    let limiter = ARXIV_LAST_REQUEST.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut last_request = limiter.lock().await;
    if let Some(last) = *last_request {
        let elapsed = last.elapsed();
        if elapsed < ARXIV_MIN_INTERVAL {
            let delay = ARXIV_MIN_INTERVAL - elapsed;
            debug!(
                delay_secs = delay.as_secs_f32(),
                "waiting for arXiv rate limit"
            );
            tokio::time::sleep(delay).await;
        }
    }
    *last_request = Some(Instant::now());
}

/// Parse arXiv RSS XML into raw entry data compatible with the Atom converter.
fn parse_arxiv_rss_feed(
    xml: &str,
    requested_category: &str,
    include_cross_list: bool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<serde_json::Value>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut in_item = false;
    let mut current_tag = String::new();
    let mut item = RssItem::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "item" {
                    in_item = true;
                    current_tag.clear();
                    item = RssItem::default();
                } else if in_item {
                    current_tag = tag;
                }
            }
            Ok(Event::Text(ref e)) if in_item && !current_tag.is_empty() => {
                let text = e.unescape().unwrap_or_default().to_string();
                item.push_text(&current_tag, &text);
            }
            Ok(Event::CData(ref e)) if in_item && !current_tag.is_empty() => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                item.push_text(&current_tag, &text);
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "item" {
                    in_item = false;
                    if let Some(entry) =
                        item.to_atom_like_entry(requested_category, include_cross_list, start, end)
                    {
                        entries.push(entry);
                    }
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    Ok(entries)
}

#[derive(Default)]
struct RssItem {
    title: String,
    link: String,
    description: String,
    guid: String,
    pub_date: String,
    creators: Vec<String>,
    categories: Vec<String>,
    announce_type: String,
}

impl RssItem {
    fn push_text(&mut self, tag: &str, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        match tag {
            "title" => push_joined(&mut self.title, text, " "),
            "link" => push_joined(&mut self.link, text, ""),
            "description" => push_joined(&mut self.description, text, "\n"),
            "guid" => push_joined(&mut self.guid, text, ""),
            "pubDate" => push_joined(&mut self.pub_date, text, " "),
            "dc:creator" => self.creators.push(text.to_string()),
            "category" => self.categories.push(text.to_string()),
            "arxiv:announce_type" => push_joined(&mut self.announce_type, text, ""),
            _ => {}
        }
    }

    fn to_atom_like_entry(
        &self,
        requested_category: &str,
        include_cross_list: bool,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Option<serde_json::Value> {
        let published_at = DateTime::parse_from_rfc2822(&self.pub_date)
            .ok()?
            .with_timezone(&Utc);
        if published_at < start || published_at >= end {
            return None;
        }

        let announce_type = self
            .announce_type()
            .or_else(|| extract_announce_type(&self.description))?;
        if announce_type != "new" {
            return None;
        }

        if include_cross_list {
            if !self.categories.iter().any(|c| c == requested_category) {
                return None;
            }
        } else if self.categories.first().map(String::as_str) != Some(requested_category) {
            return None;
        }

        let arxiv_id = extract_arxiv_id_from_guid(&self.guid)
            .or_else(|| extract_arxiv_id_from_url(&self.link))
            .or_else(|| extract_arxiv_id_from_description(&self.description))?;
        let abstract_text = extract_abstract(&self.description);
        let authors = self
            .creators
            .iter()
            .flat_map(|creator| creator.split(','))
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| serde_json::json!({ "name": name }))
            .collect::<Vec<_>>();

        Some(serde_json::json!({
            "id": format!("http://arxiv.org/abs/{}", arxiv_id),
            "title": self.title,
            "summary": abstract_text,
            "authors": authors,
            "published": published_at.to_rfc3339(),
            "updated": published_at.to_rfc3339(),
            "categories": self.categories,
            "link_alternate": self.link,
            "link_related": format!("https://arxiv.org/pdf/{}", arxiv_id),
            "announce_type": announce_type,
        }))
    }

    fn announce_type(&self) -> Option<String> {
        let ty = self.announce_type.trim();
        if ty.is_empty() {
            None
        } else {
            Some(ty.to_string())
        }
    }
}

fn push_joined(target: &mut String, text: &str, sep: &str) {
    if !target.is_empty() {
        target.push_str(sep);
    }
    target.push_str(text);
}

fn extract_announce_type(description: &str) -> Option<String> {
    let marker = "Announce Type:";
    let rest = description.split(marker).nth(1)?.trim_start();
    rest.split_whitespace().next().map(str::to_string)
}

fn extract_abstract(description: &str) -> String {
    let marker = "Abstract:";
    description
        .split_once(marker)
        .map(|(_, abstract_text)| abstract_text.trim().to_string())
        .unwrap_or_else(|| description.trim().to_string())
}

fn extract_arxiv_id_from_guid(guid: &str) -> Option<String> {
    let id = guid.rsplit(':').next()?.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn extract_arxiv_id_from_url(url: &str) -> Option<String> {
    url.trim_end_matches('/').rsplit('/').next().and_then(|id| {
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    })
}

fn extract_arxiv_id_from_description(description: &str) -> Option<String> {
    let rest = description.strip_prefix("arXiv:")?;
    rest.split_whitespace().next().map(str::to_string)
}

#[derive(Debug, Default)]
struct ArxivAtomFeed {
    entries: Vec<serde_json::Value>,
    total_results: Option<usize>,
}

/// Parse arXiv Atom XML into raw entry data.
fn parse_arxiv_atom_feed(xml: &str) -> Result<ArxivAtomFeed> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut feed = ArxivAtomFeed::default();
    let mut current_entry: Option<serde_json::Map<String, serde_json::Value>> = None;
    let mut current_tag: Option<String> = None;
    let mut in_entry = false;
    let mut authors: Vec<serde_json::Value> = Vec::new();
    let mut in_author = false;
    let mut author_name = String::new();
    let mut author_affiliation = String::new();
    let mut categories: Vec<String> = Vec::new();
    let mut primary_category: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "entry" => {
                        in_entry = true;
                        current_entry = Some(serde_json::Map::new());
                        current_tag = None;
                        authors = Vec::new();
                        categories = Vec::new();
                        primary_category = None;
                    }
                    "author" if in_entry => {
                        in_author = true;
                        author_name = String::new();
                        author_affiliation = String::new();
                    }
                    _ if in_entry => {
                        current_tag = Some(tag);
                    }
                    _ => {
                        current_tag = Some(tag);
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }

                if in_entry {
                    if in_author {
                        match current_tag.as_deref() {
                            Some("name") => push_joined(&mut author_name, text, " "),
                            Some("affiliation") | Some("arxiv:affiliation") => {
                                push_joined(&mut author_affiliation, text, " ")
                            }
                            _ => {}
                        }
                    } else if let Some(ref mut entry) = current_entry {
                        match current_tag.as_deref() {
                            Some("doi") | Some("arxiv:doi") => {
                                push_json_text(entry, "doi", text, " ");
                            }
                            Some(tag) => {
                                push_json_text(entry, tag, text, " ");
                            }
                            None => {}
                        }
                    }
                } else if current_tag.as_deref() == Some("opensearch:totalResults") {
                    feed.total_results = text.parse::<usize>().ok();
                }
            }
            Ok(Event::CData(ref e)) if in_entry => {
                let text = String::from_utf8_lossy(e.as_ref()).trim().to_string();
                if !text.is_empty() {
                    if let Some(ref mut entry) = current_entry {
                        if let Some(tag) = current_tag.as_deref() {
                            push_json_text(entry, tag, &text, "\n");
                        }
                    }
                }
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
                            if let Some(ref pc) = primary_category {
                                entry.insert(
                                    "primary_category".into(),
                                    serde_json::Value::String(pc.clone()),
                                );
                            }
                        }
                        if let Some(entry) = current_entry.take() {
                            feed.entries.push(serde_json::Value::Object(entry));
                        }
                        in_entry = false;
                        current_tag = None;
                    }
                    "author" if in_entry => {
                        in_author = false;
                        if !author_name.is_empty() {
                            let mut author_json = serde_json::json!({"name": author_name});
                            if !author_affiliation.is_empty() {
                                author_json["affiliation"] =
                                    serde_json::Value::String(author_affiliation.clone());
                            }
                            authors.push(author_json);
                        }
                    }
                    _ => current_tag = None,
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
                        "arxiv:primary_category" => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "term" {
                                    primary_category = Some(val);
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
    }

    Ok(feed)
}

fn push_json_text(
    entry: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    text: &str,
    sep: &str,
) {
    if let Some(existing) = entry.get(field).and_then(|value| value.as_str()) {
        let joined = if existing.is_empty() {
            text.to_string()
        } else {
            format!("{}{}{}", existing, sep, text)
        };
        entry.insert(field.to_string(), serde_json::Value::String(joined));
    } else {
        entry.insert(
            field.to_string(),
            serde_json::Value::String(text.to_string()),
        );
    }
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

        let feed = parse_arxiv_atom_feed(xml).unwrap();
        assert_eq!(feed.entries.len(), 1);
        let entry = &feed.entries[0];
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

        let feed = parse_arxiv_atom_feed(xml).unwrap();
        let entry = &feed.entries[0];
        let authors = entry["authors"].as_array().unwrap();
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0]["name"], "Alice Smith");
        assert_eq!(authors[1]["name"], "Bob Jones");
        // Affiliation should NOT be included in the name
        assert!(!authors[0]["name"].as_str().unwrap().contains("MIT"));
    }

    #[test]
    fn parse_rss_filters_new_items_by_date() {
        let xml = include_str!("../../../tests/fixtures/arxiv_rss.xml");
        let start = chrono::NaiveDate::from_ymd_opt(2023, 1, 30)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let end = chrono::NaiveDate::from_ymd_opt(2023, 1, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        let entries = parse_arxiv_rss_feed(xml, "cs.AI", false, start, end).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["summary"], "A test abstract about attention.");
        assert_eq!(entries[0]["announce_type"], "new");
        assert_eq!(entries[0]["authors"].as_array().unwrap().len(), 2);
        assert_eq!(
            entries[0]["link_related"],
            "https://arxiv.org/pdf/2301.12345v1"
        );
    }

    #[test]
    fn parse_rss_honors_cross_list_setting() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss xmlns:dc="http://purl.org/dc/elements/1.1/" version="2.0">
  <channel>
    <item>
      <title>Cross Listed Paper</title>
      <link>https://arxiv.org/abs/2301.11111</link>
      <description>arXiv:2301.11111v1 Announce Type: new
Abstract: Cross listed abstract.</description>
      <guid isPermaLink="false">oai:arXiv.org:2301.11111v1</guid>
      <category>cs.CL</category>
      <category>cs.AI</category>
      <pubDate>Mon, 30 Jan 2023 00:00:00 -0000</pubDate>
      <dc:creator>Alice Smith</dc:creator>
    </item>
  </channel>
</rss>"#;
        let start = chrono::NaiveDate::from_ymd_opt(2023, 1, 30)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let end = chrono::NaiveDate::from_ymd_opt(2023, 1, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        assert!(parse_arxiv_rss_feed(xml, "cs.AI", false, start, end)
            .unwrap()
            .is_empty());
        assert_eq!(
            parse_arxiv_rss_feed(xml, "cs.AI", true, start, end)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn parse_atom_feed_reads_total_results_and_arxiv_doi() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/"
      xmlns:arxiv="http://arxiv.org/schemas/atom">
  <opensearch:totalResults>2</opensearch:totalResults>
  <entry>
    <id>http://arxiv.org/abs/2301.12345v1</id>
    <title>Test Paper</title>
    <summary>Abstract.</summary>
    <author><name>Alice Smith</name><arxiv:affiliation>MIT</arxiv:affiliation></author>
    <published>2023-01-30T00:00:00Z</published>
    <updated>2023-01-31T00:00:00Z</updated>
    <category term="cs.AI"/>
    <link href="http://arxiv.org/abs/2301.12345v1" rel="alternate"/>
    <link title="pdf" href="http://arxiv.org/pdf/2301.12345v1" rel="related"/>
    <arxiv:doi>10.48550/arxiv.2301.12345</arxiv:doi>
  </entry>
</feed>"#;

        let feed = parse_arxiv_atom_feed(xml).unwrap();

        assert_eq!(feed.total_results, Some(2));
        assert_eq!(feed.entries.len(), 1);
        assert_eq!(feed.entries[0]["doi"], "10.48550/arxiv.2301.12345");
        assert_eq!(feed.entries[0]["authors"][0]["affiliation"], "MIT");
    }

    #[test]
    fn export_url_combines_categories_and_date_window() {
        let client = ArxivClient::with_backend(
            ArxivBackendKind::Export,
            vec!["cs.AI".into(), "cs.LG".into()],
            true,
            1000,
            3,
        )
        .with_export_base_url("https://example.test/api/query");
        let start = chrono::NaiveDate::from_ymd_opt(2023, 1, 30)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let end = chrono::NaiveDate::from_ymd_opt(2023, 1, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        let url = client.build_export_url(Some((start, end)), 1000);

        assert!(url.contains("search_query="));
        assert!(url.contains("submittedDate%3A%5B202301300000%20TO%20202301310000%5D"));
        assert!(url.contains("sortOrder=ascending"));
        assert!(url.contains("start=1000"));
        assert!(url.contains("max_results=1000"));
    }

    #[test]
    fn export_url_without_date_fetches_latest() {
        let client = ArxivClient::with_backend(
            ArxivBackendKind::Export,
            vec!["cs.AI".into(), "cs.LG".into()],
            true,
            1000,
            3,
        )
        .with_export_base_url("https://example.test/api/query");

        let url = client.build_export_url(None, 0);

        assert!(url.contains("search_query="));
        assert!(!url.contains("submittedDate%3A"));
        assert!(url.contains("sortOrder=descending"));
        assert!(url.contains("cat%3Acs.AI%20OR%20cat%3Acs.LG"));
    }
}
