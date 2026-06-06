use reqwest::Client;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::models::read::TokenUsage;

/// OpenAI-compatible chat/completions client for deep reading.
pub struct ReaderClient {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    timeout: Duration,
    max_retries: u32,
    semaphore: Semaphore,
    #[allow(dead_code)]
    max_input_tokens: usize,
}

impl ReaderClient {
    pub fn new(
        base_url: String,
        api_key: String,
        model: String,
        timeout_secs: u64,
        max_retries: u32,
        max_concurrency: usize,
        max_input_tokens: usize,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            timeout: Duration::from_secs(timeout_secs),
            max_retries,
            semaphore: Semaphore::new(max_concurrency),
            max_input_tokens,
        }
    }

    /// Call the chat/completions endpoint.
    pub async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<(String, Option<TokenUsage>)> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| Error::Llm(format!("semaphore closed: {}", e)))?;

        self.complete_with_retry(system_prompt, user_prompt).await
    }

    async fn complete_with_retry(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<(String, Option<TokenUsage>)> {
        let mut last_err = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = Duration::from_secs(2u64.pow(attempt));
                warn!(
                    attempt,
                    delay_secs = delay.as_secs(),
                    "retrying LLM request"
                );
                tokio::time::sleep(delay).await;
            }

            match self.call_api(system_prompt, user_prompt).await {
                Ok(result) => return Ok(result),
                Err(Error::RateLimited { retry_after_secs }) => {
                    warn!(retry_after_secs, "LLM rate limited");
                    tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
                    last_err = Some(Error::RateLimited { retry_after_secs });
                }
                Err(e) if e.is_retryable() => {
                    warn!(error = %e, "retryable LLM error");
                    last_err = Some(e);
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or_else(|| Error::Llm("max retries exceeded".into())))
    }

    async fn call_api(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<(String, Option<TokenUsage>)> {
        let url = format!("{}/chat/completions", self.base_url);

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.3,
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
            return Err(Error::Llm(format!("API returned {}: {}", status, body)));
        }

        let resp_body: serde_json::Value = resp.json().await?;

        let content = resp_body
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Llm("missing content in response".into()))?
            .to_string();

        let usage = resp_body.get("usage").map(|u| TokenUsage {
            input_tokens: u
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            output_tokens: u
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            total_tokens: u
                .get("total_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        });

        debug!(content_len = content.len(), "got LLM response");
        Ok((content, usage))
    }
}
