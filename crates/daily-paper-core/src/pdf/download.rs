use sha2::Digest;
use std::path::Path;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::models::pdf::PdfAsset;

/// Download a PDF from the given URL and save it to the target path.
///
/// Returns a PdfAsset with metadata about the downloaded file.
pub async fn download_pdf(
    paper_id: &str,
    url: &str,
    target_dir: &Path,
    timeout_secs: u64,
    max_mb: u64,
) -> Result<PdfAsset> {
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .await
        .map_err(|e| Error::PdfDownload(format!("request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(Error::PdfDownload(format!("HTTP {}", status)));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| Error::PdfDownload(format!("failed to read response: {}", e)))?;

    let byte_len = bytes.len() as u64;
    let max_bytes = max_mb * 1024 * 1024;
    if byte_len > max_bytes {
        return Err(Error::PdfDownload(format!(
            "PDF too large: {} bytes (max {} MB)",
            byte_len, max_mb
        )));
    }

    // Compute SHA-256
    let hash = sha2::Sha256::digest(&bytes);
    let sha256 = hex::encode(hash);

    // Generate filename from paper_id
    let safe_name = paper_id.replace([':', '/'], "_");
    let file_name = format!("{}.pdf", safe_name);
    let file_path = target_dir.join(&file_name);

    std::fs::create_dir_all(target_dir)?;
    std::fs::write(&file_path, &bytes)?;

    Ok(PdfAsset {
        paper_id: paper_id.to_string(),
        url: url.to_string(),
        file_path: file_path.to_string_lossy().to_string(),
        sha256,
        downloaded_at: chrono::Utc::now(),
        byte_len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_asset_serializes() {
        let asset = PdfAsset {
            paper_id: "arxiv:2301.12345".into(),
            url: "https://arxiv.org/pdf/2301.12345".into(),
            file_path: "/tmp/paper.pdf".into(),
            sha256: "abc123".into(),
            downloaded_at: chrono::Utc::now(),
            byte_len: 12345,
        };
        let json = serde_json::to_string(&asset).unwrap();
        assert!(json.contains("arxiv:2301.12345"));
    }
}
