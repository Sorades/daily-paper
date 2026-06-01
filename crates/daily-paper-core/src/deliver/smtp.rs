use crate::error::{Error, Result};

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

/// Send an email via SMTP.
pub fn send_email(
    smtp_server: &str,
    smtp_port: u16,
    username: &str,
    password: &str,
    sender: &str,
    receiver: &str,
    subject: &str,
    html_body: &str,
    text_body: &str,
    message_id: &str,
) -> Result<String> {
    let email = Message::builder()
        .from(
            sender
                .parse()
                .map_err(|e| Error::Delivery(format!("invalid sender '{}': {}", sender, e)))?,
        )
        .to(receiver
            .parse()
            .map_err(|e| Error::Delivery(format!("invalid receiver '{}': {}", receiver, e)))?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .message_id(Some(message_id.to_string()))
        .multipart(
            lettre::message::MultiPart::alternative()
                .singlepart(
                    lettre::message::SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(text_body.to_string()),
                )
                .singlepart(
                    lettre::message::SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html_body.to_string()),
                ),
        )
        .map_err(|e| Error::Delivery(format!("failed to build email: {}", e)))?;

    let creds = Credentials::new(username.to_string(), password.to_string());

    let transport = SmtpTransport::relay(smtp_server)
        .map_err(|e| Error::Delivery(format!("SMTP relay error: {}", e)))?
        .port(smtp_port)
        .credentials(creds)
        .build();

    let response = transport
        .send(&email)
        .map_err(|e| Error::Delivery(format!("SMTP send failed: {}", e)))?;

    Ok(format!("{:?}", response))
}
