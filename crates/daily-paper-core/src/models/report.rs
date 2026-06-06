use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedReport {
    pub report_hash: String,
    pub report_instance_id: String,
    pub run_id: String,
    pub generated_at: DateTime<Utc>,
    pub title: String,
    pub html_path: String,
    pub text_path: Option<String>,
    pub ranked_paper_ids: Vec<String>,
    pub read_paper_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportIndex {
    pub date: String,
    pub run_id: String,
    pub report_path: String,
    pub html_path: String,
    pub text_path: Option<String>,
    pub generated_at: DateTime<Utc>,
    pub report_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryReceipt {
    pub delivery_key: String,
    pub report_hash: String,
    pub report_instance_id: String,
    pub sink_id: String,
    pub sink_config_hash: String,
    pub sent_at: DateTime<Utc>,
    pub recipient: String,
    pub provider_message_id: Option<String>,
}

/// Compute delivery_key for idempotent sending.
pub fn compute_delivery_key(
    report_hash: &str,
    sink_id: &str,
    sink_config_hash: &str,
    recipient: &str,
) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(report_hash.as_bytes());
    hasher.update(sink_id.as_bytes());
    hasher.update(sink_config_hash.as_bytes());
    hasher.update(recipient.as_bytes());
    hex::encode(hasher.finalize())
}
