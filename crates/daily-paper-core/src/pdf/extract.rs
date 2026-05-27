use sha2::Digest;
use std::path::Path;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::models::pdf::{ExtractedText, compute_text_extract_key};

use super::section::parse_sections;

/// Extract text from a PDF using the external `pdftotext` command.
pub async fn extract_text(
    paper_id: &str,
    pdf_path: &Path,
    pdf_sha256: &str,
    output_dir: &Path,
    timeout_secs: u64,
    max_chars: usize,
) -> Result<ExtractedText> {
    // Check pdftotext is available
    let check = tokio::process::Command::new("pdftotext")
        .arg("-v")
        .output()
        .await;
    if check.is_err() {
        return Err(Error::PdfExtract(
            "pdftotext not found. Install poppler-utils (apt) or poppler (brew).".into(),
        ));
    }

    // Run pdftotext
    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        tokio::process::Command::new("pdftotext")
            .args(["-enc", "UTF-8", "-layout"])
            .arg(pdf_path)
            .arg("-")
            .output(),
    )
    .await
    .map_err(|_| Error::PdfExtract("pdftotext timed out".into()))?
    .map_err(|e| Error::PdfExtract(format!("pdftotext failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::PdfExtract(format!(
            "pdftotext exited with {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let mut text = String::from_utf8_lossy(&output.stdout).to_string();

    // Check for empty or too-short text
    let trimmed = text.trim();
    if trimmed.len() < 50 {
        return Err(Error::PdfExtract(format!(
            "extracted text too short ({} chars), likely a scan or error",
            trimmed.len()
        )));
    }

    // Truncate if needed
    if text.len() > max_chars {
        text.truncate(max_chars);
    }

    // Parse sections
    let sections = parse_sections(&text);

    // Compute text SHA-256
    let text_hash = sha2::Sha256::digest(text.as_bytes());
    let text_sha256 = hex::encode(text_hash);

    // Save extracted text
    let safe_name = paper_id.replace([':', '/'], "_");
    let text_file = output_dir.join(format!("{}.txt", safe_name));
    std::fs::create_dir_all(output_dir)?;
    std::fs::write(&text_file, &text)?;

    let text_extract_key = compute_text_extract_key(
        pdf_sha256,
        "pdftotext",
        "layout",
        "v1",
    );

    Ok(ExtractedText {
        text_extract_key,
        paper_id: paper_id.to_string(),
        pdf_sha256: pdf_sha256.to_string(),
        extractor: "pdftotext".into(),
        extractor_config_hash: "layout".into(),
        section_parser_version: "v1".into(),
        extracted_at: chrono::Utc::now(),
        text_path: text_file.to_string_lossy().to_string(),
        text_sha256,
        sections,
        warnings: Vec::new(),
    })
}
