use std::path::Path;

use anyhow::Result;

/// List cache contents.
pub fn list(data_dir: &Path) -> Result<()> {
    let cache_dir = data_dir.join("cache");
    if !cache_dir.exists() {
        println!("Cache directory does not exist: {}", cache_dir.display());
        return Ok(());
    }

    let entries = vec![
        ("arxiv", "ArXiv candidate lists"),
        ("embeddings", "Embedding vectors"),
        ("models", "Fastembed model files"),
        ("papers", "Paper cache (PDFs, text, read results)"),
        ("rerank", "Rerank selections"),
        ("zotero", "Zotero sync state and snapshots"),
        ("deliveries", "Delivery receipts"),
        ("reports", "Generated reports"),
        ("runs", "Run manifests"),
    ];

    println!("Cache directory: {}\n", cache_dir.display());
    println!("{:<15} {:<10} {:<30}", "Type", "Files", "Description");
    println!("{}", "-".repeat(55));

    for (name, desc) in entries {
        let dir = cache_dir.join(name);
        if dir.exists() {
            let count = count_files(&dir)?;
            let size = dir_size(&dir)?;
            println!(
                "{:<15} {:<10} {:<30}",
                name,
                format!("{} files", count),
                format_size(size)
            );
        } else {
            println!("{:<15} {:<10} {:<30}", name, "N/A", desc);
        }
    }

    Ok(())
}

/// Clean cache.
pub fn clean(data_dir: &Path, kind: &str) -> Result<()> {
    let cache_dir = data_dir.join("cache");
    if !cache_dir.exists() {
        println!("Cache directory does not exist");
        return Ok(());
    }

    let valid_kinds = [
        "arxiv",
        "embeddings",
        "models",
        "papers",
        "rerank",
        "zotero",
        "deliveries",
        "reports",
        "runs",
        "all",
    ];

    if !valid_kinds.contains(&kind) {
        anyhow::bail!(
            "invalid cache kind '{}'; valid kinds: {}",
            kind,
            valid_kinds.join(", ")
        );
    }

    if kind == "all" {
        for k in &valid_kinds[..valid_kinds.len() - 1] {
            clean_single(&cache_dir, k)?;
        }
    } else {
        clean_single(&cache_dir, kind)?;
    }

    Ok(())
}

fn clean_single(cache_dir: &Path, kind: &str) -> Result<()> {
    let dir = cache_dir.join(kind);
    if !dir.exists() {
        println!("Cache '{}' does not exist, skipping", kind);
        return Ok(());
    }

    let size = dir_size(&dir)?;
    std::fs::remove_dir_all(&dir)?;
    println!("Cleaned cache '{}', freed {}", kind, format_size(size));
    Ok(())
}

/// Show cache size.
pub fn size(data_dir: &Path) -> Result<()> {
    let cache_dir = data_dir.join("cache");
    if !cache_dir.exists() {
        println!("Cache directory does not exist");
        return Ok(());
    }

    let total_size = dir_size(&cache_dir)?;
    let total_files = count_files(&cache_dir)?;

    println!("Cache directory: {}", cache_dir.display());
    println!("Total size: {}", format_size(total_size));
    println!("Total files: {}", total_files);

    Ok(())
}

fn dir_size(dir: &Path) -> Result<u64> {
    let mut size = 0;
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                size += dir_size(&path)?;
            } else {
                size += entry.metadata()?.len();
            }
        }
    }
    Ok(size)
}

fn count_files(dir: &Path) -> Result<usize> {
    let mut count = 0;
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                count += count_files(&path)?;
            } else {
                count += 1;
            }
        }
    }
    Ok(count)
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
