use std::path::Path;

use crate::error::Result;
use crate::models::report::{DeliveryReceipt, compute_delivery_key};

use super::smtp::send_email;

/// Deliver a report via email.
pub fn deliver_email(
    report_hash: &str,
    report_instance_id: &str,
    run_id: &str,
    html_path: &Path,
    text_path: Option<&Path>,
    smtp_server: &str,
    smtp_port: u16,
    sender: &str,
    receiver: &str,
    password: &str,
    subject: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<DeliveryReceipt> {
    let html_body = std::fs::read_to_string(html_path)?;
    let text_body = match text_path {
        Some(p) => std::fs::read_to_string(p).unwrap_or_else(|_| html_body.clone()),
        None => html_body.clone(),
    };

    let sink_config_hash = compute_sink_config_hash(smtp_server, smtp_port, sender, receiver);
    let delivery_key = compute_delivery_key(report_hash, "smtp", &sink_config_hash, receiver);

    let message_id = format!("<{}.{}@daily-paper>", report_hash, run_id);

    let provider_message_id = send_email(
        smtp_server,
        smtp_port,
        sender,
        password,
        sender,
        receiver,
        subject,
        &html_body,
        &text_body,
        &message_id,
    )?;

    Ok(DeliveryReceipt {
        delivery_key,
        report_hash: report_hash.to_string(),
        report_instance_id: report_instance_id.to_string(),
        sink_id: "smtp".into(),
        sink_config_hash,
        sent_at: now,
        recipient: receiver.to_string(),
        provider_message_id: Some(provider_message_id),
    })
}

fn compute_sink_config_hash(server: &str, port: u16, sender: &str, receiver: &str) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(server.as_bytes());
    hasher.update(port.to_le_bytes());
    hasher.update(sender.as_bytes());
    hasher.update(receiver.as_bytes());
    hex::encode(hasher.finalize())
}
