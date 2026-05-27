use reqwest::Client;
use std::time::Duration;
use tracing::{debug, warn};

use crate::error::{Error, Result};

const ZOTERO_API_BASE: &str = "https://api.zotero.org";

pub struct ZoteroClient {
    client: Client,
    user_id: String,
    api_key: String,
    max_retries: u32,
}

impl ZoteroClient {
    pub fn new(user_id: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            user_id,
            api_key,
            max_retries: 3,
        }
    }

    fn base_url(&self) -> String {
        format!("{}/users/{}", ZOTERO_API_BASE, self.user_id)
    }

    async fn get_with_retry<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let mut last_err = None;
        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = Duration::from_secs(2u64.pow(attempt));
                warn!(attempt, delay_secs = delay.as_secs(), "retrying Zotero request");
                tokio::time::sleep(delay).await;
            }

            let resp = self
                .client
                .get(url)
                .header("Zotero-API-Key", &self.api_key)
                .header("Zotero-API-Version", "3")
                .send()
                .await
                .map_err(|e| Error::RetryableNetwork(e.to_string()))?;

            if resp.status().as_u16() == 429 {
                let retry_after = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(30);
                warn!(retry_after, "Zotero rate limited");
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
                    "Zotero API returned {}: {}",
                    status, body
                )));
            }

            let data = resp.json::<T>().await?;
            return Ok(data);
        }

        Err(last_err.unwrap_or_else(|| Error::RetryableNetwork("max retries exceeded".into())))
    }

    /// Get the current library version (for incremental sync).
    pub async fn get_library_version(&self) -> Result<Option<u64>> {
        let url = format!("{}/items?limit=1&format=keys", self.base_url());
        let resp = self
            .client
            .get(&url)
            .header("Zotero-API-Key", &self.api_key)
            .header("Zotero-API-Version", "3")
            .send()
            .await
            .map_err(|e| Error::RetryableNetwork(e.to_string()))?;

        let version = resp
            .headers()
            .get("Last-Modified-Version")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        Ok(version)
    }

    /// Fetch all items with pagination, optionally since a given version.
    pub async fn fetch_items(&self, since_version: Option<u64>) -> Result<Vec<serde_json::Value>> {
        let mut all_items = Vec::new();
        let mut start: u32 = 0;
        let limit: u32 = 100;

        loop {
            let mut url = format!(
                "{}/items?limit={}&start={}&format=json",
                self.base_url(),
                limit,
                start
            );
            if let Some(version) = since_version {
                url.push_str(&format!("&since={}", version));
            }

            debug!(url = %url, "fetching Zotero items");

            let items: Vec<serde_json::Value> = self.get_with_retry(&url).await?;

            let is_last = items.len() < limit as usize;
            all_items.extend(items);

            if is_last {
                break;
            }
            start += limit;
        }

        debug!(count = all_items.len(), "fetched Zotero items");
        Ok(all_items)
    }

    /// Fetch all collections with pagination.
    pub async fn fetch_collections(&self) -> Result<Vec<serde_json::Value>> {
        let mut all_collections = Vec::new();
        let mut start: u32 = 0;
        let limit: u32 = 100;

        loop {
            let url = format!(
                "{}/collections?limit={}&start={}&format=json",
                self.base_url(),
                limit,
                start
            );

            let collections: Vec<serde_json::Value> = self.get_with_retry(&url).await?;

            let is_last = collections.len() < limit as usize;
            all_collections.extend(collections);

            if is_last {
                break;
            }
            start += limit;
        }

        debug!(count = all_collections.len(), "fetched Zotero collections");
        Ok(all_collections)
    }
}
