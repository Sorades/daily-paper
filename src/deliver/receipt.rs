use std::path::Path;

use crate::error::Result;
use crate::models::report::{compute_delivery_key, DeliveryReceipt};

use super::smtp::{send_email, SmtpEmail};

/// Deliver a report via email.
pub struct EmailDelivery<'a> {
    pub report_hash: &'a str,
    pub report_instance_id: &'a str,
    pub run_id: &'a str,
    pub html_path: &'a Path,
    pub text_path: Option<&'a Path>,
    pub smtp_server: &'a str,
    pub smtp_port: u16,
    pub sender: &'a str,
    pub receiver: &'a str,
    pub password: &'a str,
    pub subject: &'a str,
    pub now: chrono::DateTime<chrono::Utc>,
}

pub fn deliver_email(req: &EmailDelivery<'_>) -> Result<DeliveryReceipt> {
    let html_body = std::fs::read_to_string(req.html_path)?;
    let text_body = match req.text_path {
        Some(p) => std::fs::read_to_string(p).unwrap_or_else(|_| html_body.clone()),
        None => html_body.clone(),
    };

    let sink_config_hash =
        compute_sink_config_hash(req.smtp_server, req.smtp_port, req.sender, req.receiver);
    let delivery_key =
        compute_delivery_key(req.report_hash, "smtp", &sink_config_hash, req.receiver);

    let message_id = format!("<{}.{}@daily-paper>", req.report_hash, req.run_id);

    let provider_message_id = send_email(&SmtpEmail {
        smtp_server: req.smtp_server,
        smtp_port: req.smtp_port,
        username: req.sender,
        password: req.password,
        sender: req.sender,
        receiver: req.receiver,
        subject: req.subject,
        html_body: &html_body,
        text_body: &text_body,
        message_id: &message_id,
    })?;

    Ok(DeliveryReceipt {
        delivery_key,
        report_hash: req.report_hash.to_string(),
        report_instance_id: req.report_instance_id.to_string(),
        sink_id: "smtp".into(),
        sink_config_hash,
        sent_at: req.now,
        recipient: req.receiver.to_string(),
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
