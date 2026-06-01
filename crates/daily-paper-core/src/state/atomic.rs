use crate::error::Result;
use std::path::Path;

/// Atomically write bytes to a file.
///
/// Writes to a temporary file in the same directory, flushes, then renames
/// to the target path. This prevents partial writes on crash.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp_path = dir.to_path_buf();

    let pid = std::process::id();
    let random = fastrand::u64(..);
    temp_path.push(format!(".tmp.{}.{}", pid, random));

    std::fs::write(&temp_path, data)?;
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Atomically serialize a value as pretty JSON and write it to a file.
pub fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_vec_pretty(value)?;
    atomic_write(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn atomic_write_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");

        atomic_write(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn atomic_write_json_serializes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");

        let data = serde_json::json!({"key": "value"});
        atomic_write_json(&path, &data).unwrap();

        let content: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(content, data);
    }

    #[test]
    fn atomic_write_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");

        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn atomic_write_no_temp_left_on_success() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");

        atomic_write(&path, b"hello").unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        // Only the target file should exist, no temp files
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), "test.json");
    }
}
