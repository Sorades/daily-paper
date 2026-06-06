use crate::error::{Error, Result};

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

/// Send an email via SMTP.
pub struct SmtpEmail<'a> {
    pub smtp_server: &'a str,
    pub smtp_port: u16,
    pub username: &'a str,
    pub password: &'a str,
    pub sender: &'a str,
    pub receiver: &'a str,
    pub subject: &'a str,
    pub html_body: &'a str,
    pub text_body: &'a str,
    pub message_id: &'a str,
}

pub fn send_email(req: &SmtpEmail<'_>) -> Result<String> {
    let email = Message::builder()
        .from(
            req.sender
                .parse()
                .map_err(|e| Error::Delivery(format!("invalid sender '{}': {}", req.sender, e)))?,
        )
        .to(req
            .receiver
            .parse()
            .map_err(|e| Error::Delivery(format!("invalid receiver '{}': {}", req.receiver, e)))?)
        .subject(req.subject)
        .header(ContentType::TEXT_HTML)
        .message_id(Some(req.message_id.to_string()))
        .multipart(
            lettre::message::MultiPart::alternative()
                .singlepart(
                    lettre::message::SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(req.text_body.to_string()),
                )
                .singlepart(
                    lettre::message::SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(req.html_body.to_string()),
                ),
        )
        .map_err(|e| Error::Delivery(format!("failed to build email: {}", e)))?;

    let creds = Credentials::new(req.username.to_string(), req.password.to_string());

    let transport = SmtpTransport::relay(req.smtp_server)
        .map_err(|e| Error::Delivery(format!("SMTP relay error: {}", e)))?
        .port(req.smtp_port)
        .credentials(creds)
        .build();

    let response = transport
        .send(&email)
        .map_err(|e| Error::Delivery(format!("SMTP send failed: {}", e)))?;

    Ok(format!("{:?}", response))
}
