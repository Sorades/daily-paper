use std::path::Path;

use anyhow::Result;

/// List all archive dates.
pub fn list(data_dir: &Path) -> Result<()> {
    let archive_dir = data_dir.join("archive");
    if !archive_dir.exists() {
        println!(
            "Archive directory does not exist: {}",
            archive_dir.display()
        );
        return Ok(());
    }

    let mut dates: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&archive_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                // Only include directories that look like dates (YYYY-MM-DD)
                if name.len() == 10
                    && name.chars().nth(4) == Some('-')
                    && name.chars().nth(7) == Some('-')
                {
                    dates.push(name.to_string());
                }
            }
        }
    }

    dates.sort();
    dates.reverse();

    if dates.is_empty() {
        println!("No archives found");
    } else {
        println!("Archives:\n");
        for date in &dates {
            let date_dir = archive_dir.join(date);
            let has_report = date_dir.join("report").exists();
            let has_candidates = date_dir.join("candidates.json").exists();
            let status = if has_report {
                "✓ complete"
            } else if has_candidates {
                "○ partial"
            } else {
                "· empty"
            };
            println!("  {} {}", date, status);
        }
    }

    Ok(())
}

/// Show archive details for a specific date.
pub fn show(data_dir: &Path, date: &str) -> Result<()> {
    let archive_dir = data_dir.join("archive").join(date);
    if !archive_dir.exists() {
        anyhow::bail!("archive for date '{}' not found", date);
    }

    println!("Archive: {}\n", date);

    // Check for various files
    let files = vec![
        ("candidates.json", "Candidates"),
        ("dedup.json", "Deduplication"),
        ("embeddings.json", "Embeddings"),
        ("rerank.json", "Rerank"),
        ("read/", "Deep Read"),
        ("report/", "Report"),
    ];

    for (file, desc) in files {
        let path = archive_dir.join(file);
        if path.exists() {
            if path.is_dir() {
                let count = std::fs::read_dir(&path)?.count();
                println!("  ✓ {} ({} files)", desc, count);
            } else {
                let size = std::fs::metadata(&path)?.len();
                println!("  ✓ {} ({} bytes)", desc, size);
            }
        } else {
            println!("  ✗ {}", desc);
        }
    }

    // Show candidates if available
    let candidates_path = archive_dir.join("candidates.json");
    if candidates_path.exists() {
        let content = std::fs::read_to_string(&candidates_path)?;
        let candidates: serde_json::Value = serde_json::from_str(&content)?;
        if let Some(arr) = candidates.as_array() {
            println!("\nCandidates: {} papers", arr.len());
        }
    }

    // Show rerank if available
    let rerank_path = archive_dir.join("rerank.json");
    if rerank_path.exists() {
        let content = std::fs::read_to_string(&rerank_path)?;
        let rerank: serde_json::Value = serde_json::from_str(&content)?;
        if let Some(arr) = rerank.get("selected").and_then(|v| v.as_array()) {
            println!("Selected for reading: {} papers", arr.len());
        }
    }

    Ok(())
}
