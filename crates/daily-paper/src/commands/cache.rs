use std::path::Path;

use anyhow::Result;
use daily_paper_core::state::store::CACHE_KINDS;

/// List cache contents.
pub fn list(data_dir: &Path) -> Result<()> {
    let cache_dir = data_dir.join("cache");
    if !cache_dir.exists() {
        println!("Cache directory does not exist: {}", cache_dir.display());
        return Ok(());
    }

    println!("Cache directory: {}\n", cache_dir.display());
    println!("{:<15} {:<10} {:<30}", "Type", "Files", "Description");
    println!("{}", "-".repeat(55));

    for &(name, subdir, desc) in CACHE_KINDS {
        let dir = data_dir.join(subdir);
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

    if kind == "all" {
        for &(name, subdir, _) in CACHE_KINDS {
            clean_single(data_dir, subdir, name)?;
        }
        return Ok(());
    }

    let entry = CACHE_KINDS.iter().find(|&&(name, _, _)| name == kind);
    match entry {
        Some(&(_, subdir, _)) => clean_single(data_dir, subdir, kind)?,
        None => {
            let valid: Vec<&str> = CACHE_KINDS.iter().map(|&(name, _, _)| name).collect();
            anyhow::bail!(
                "invalid cache kind '{}'; valid kinds: {}",
                kind,
                valid.join(", ")
            );
        }
    }

    Ok(())
}

fn clean_single(data_dir: &Path, subdir: &str, kind: &str) -> Result<()> {
    let dir = data_dir.join(subdir);
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
