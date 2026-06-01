use reqwest::Client;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use crate::error::{Error, Result};

/// OpenAI-compatible embedding client.
pub struct EmbeddingClient {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    batch_size: usize,
    timeout: Duration,
    max_retries: u32,
    semaphore: Semaphore,
}

impl EmbeddingClient {
    pub fn new(
        base_url: String,
        api_key: String,
        model: String,
        batch_size: usize,
        timeout_secs: u64,
        max_retries: u32,
        max_concurrency: usize,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            batch_size,
            timeout: Duration::from_secs(timeout_secs),
            max_retries,
            semaphore: Semaphore::new(max_concurrency),
        }
    }

    /// Compute embedding input text from title and abstract.
    pub fn make_input_text(title: &str, abstract_text: &str) -> String {
        format!("{}\n\n{}", title, abstract_text)
    }

    /// Compute the input hash for cache key.
    pub fn compute_input_hash(
        provider_id: &str,
        model_id: &str,
        config_hash: &str,
        input_text: &str,
    ) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(provider_id.as_bytes());
        hasher.update(model_id.as_bytes());
        hasher.update(config_hash.as_bytes());
        hasher.update(input_text.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Embed a batch of texts, returning vectors in the same order.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(self.batch_size) {
            let _permit = self
                .semaphore
                .acquire()
                .await
                .map_err(|e| Error::Embedding(format!("semaphore closed: {}", e)))?;

            let embeddings = self.call_with_retry(chunk).await?;
            all_embeddings.extend(embeddings);
        }

        Ok(all_embeddings)
    }

    /// Embed a single text.
    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| Error::Embedding(format!("semaphore closed: {}", e)))?;

        let result = self.call_with_retry(&[text.to_string()]).await?;
        result
            .into_iter()
            .next()
            .ok_or_else(|| Error::Embedding("empty response from embedding API".into()))
    }

    async fn call_with_retry(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut last_err = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = Duration::from_secs(2u64.pow(attempt));
                warn!(
                    attempt,
                    delay_secs = delay.as_secs(),
                    "retrying embedding request"
                );
                tokio::time::sleep(delay).await;
            }

            match self.call_api(texts).await {
                Ok(embeddings) => return Ok(embeddings),
                Err(Error::RateLimited { retry_after_secs }) => {
                    warn!(retry_after_secs, "embedding rate limited");
                    tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
                    last_err = Some(Error::RateLimited { retry_after_secs });
                }
                Err(e) if is_retryable(&e) => {
                    warn!(error = %e, "retryable embedding error");
                    last_err = Some(e);
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or_else(|| Error::Embedding("max retries exceeded".into())))
    }

    async fn call_api(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.base_url);

        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .timeout(self.timeout)
            .json(&body)
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
            return Err(Error::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Embedding(format!(
                "API returned {}: {}",
                status, body
            )));
        }

        let resp_body: serde_json::Value = resp.json().await?;

        let data = resp_body
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Embedding("missing 'data' in response".into()))?;

        let mut embeddings = Vec::with_capacity(data.len());
        for item in data {
            let vector = item
                .get("embedding")
                .and_then(|v| v.as_array())
                .ok_or_else(|| Error::Embedding("missing 'embedding' in response item".into()))?
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect::<Vec<f32>>();
            embeddings.push(vector);
        }

        debug!(
            count = embeddings.len(),
            dim = embeddings.first().map(|v| v.len()).unwrap_or(0),
            "got embeddings"
        );
        Ok(embeddings)
    }
}

fn is_retryable(err: &Error) -> bool {
    matches!(err, Error::RetryableNetwork(_) | Error::RateLimited { .. })
}
