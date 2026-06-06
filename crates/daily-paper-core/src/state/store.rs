use super::atomic::{atomic_write, atomic_write_json};
use super::lock::RunLock;
use super::path::StatePath;
use crate::error::Result;
use std::path::{Path, PathBuf};

/// File-system based state store.
///
/// All file access goes through `StatePath` to prevent path traversal.
pub struct FileStateStore {
    root: PathBuf,
}

impl FileStateStore {
    /// Create a new FileStateStore rooted at the given directory.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Get the root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ensure the root and standard subdirectories exist.
    pub fn ensure_dirs(&self) -> Result<()> {
        for subdir in &[
            "runs",
            "cache/arxiv",
            "cache/zotero/snapshots",
            "cache/embeddings",
            "cache/models",
            "cache/rerank",
            "cache/papers",
            "dates",
            "reports",
            "deliveries/history",
        ] {
            let dir = self.root.join(subdir);
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Ensure a date-specific directory exists with all stage subdirectories.
    pub fn ensure_date_dir(&self, date: &str) -> Result<()> {
        let date_dir = self.root.join("dates").join(date);
        for subdir in &["", "read", "report"] {
            let dir = date_dir.join(subdir);
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Get the path to a date directory.
    pub fn date_dir(&self, date: &str) -> PathBuf {
        self.root.join("dates").join(date)
    }

    /// Acquire a run lock.
    pub fn acquire_lock(&self, run_id: &str, stale_after: std::time::Duration) -> Result<RunLock> {
        std::fs::create_dir_all(&self.root)?;
        RunLock::acquire(&self.root, run_id, stale_after)
    }

    /// Read a JSON file at the given state path.
    pub fn read_json<T: serde::de::DeserializeOwned>(&self, path: &StatePath) -> Result<Option<T>> {
        let full_path = path.resolve(&self.root);
        if !full_path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&full_path)?;
        let value = serde_json::from_slice(&data)?;
        Ok(Some(value))
    }

    /// Write a JSON file atomically at the given state path.
    pub fn write_json<T: serde::Serialize>(&self, path: &StatePath, value: &T) -> Result<()> {
        let full_path = path.resolve(&self.root);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write_json(&full_path, value)
    }

    /// Check if a file exists at the given state path.
    pub fn exists(&self, path: &StatePath) -> Result<bool> {
        let full_path = path.resolve(&self.root);
        Ok(full_path.exists())
    }

    /// Read raw bytes from the given state path.
    pub fn read_bytes(&self, path: &StatePath) -> Result<Option<Vec<u8>>> {
        let full_path = path.resolve(&self.root);
        if !full_path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&full_path)?;
        Ok(Some(data))
    }

    /// Write raw bytes atomically at the given state path.
    pub fn write_bytes(&self, path: &StatePath, data: &[u8]) -> Result<()> {
        let full_path = path.resolve(&self.root);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&full_path, data)
    }

    /// Read a file as UTF-8 string.
    pub fn read_string(&self, path: &StatePath) -> Result<Option<String>> {
        let full_path = path.resolve(&self.root);
        if !full_path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&full_path)?;
        Ok(Some(data))
    }

    /// Write a string atomically at the given state path.
    pub fn write_string(&self, path: &StatePath, data: &str) -> Result<()> {
        let full_path = path.resolve(&self.root);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&full_path, data.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: u32,
    }

    fn make_store() -> (TempDir, FileStateStore) {
        let dir = TempDir::new().unwrap();
        let store = FileStateStore::new(dir.path().to_path_buf());
        (dir, store)
    }

    #[test]
    fn read_write_json() {
        let (_dir, store) = make_store();
        let path = StatePath::new("test/data.json").unwrap();

        let data = TestData {
            name: "hello".into(),
            value: 42,
        };
        store.write_json(&path, &data).unwrap();

        let loaded: Option<TestData> = store.read_json(&path).unwrap();
        assert_eq!(loaded, Some(data));
    }

    #[test]
    fn read_nonexistent_returns_none() {
        let (_dir, store) = make_store();
        let path = StatePath::new("nonexistent.json").unwrap();
        let loaded: Option<TestData> = store.read_json(&path).unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn read_write_bytes() {
        let (_dir, store) = make_store();
        let path = StatePath::new("cache/data.bin").unwrap();

        store.write_bytes(&path, b"binary data").unwrap();
        let loaded = store.read_bytes(&path).unwrap();
        assert_eq!(loaded, Some(b"binary data".to_vec()));
    }

    #[test]
    fn exists_check() {
        let (_dir, store) = make_store();
        let path = StatePath::new("test/file.json").unwrap();

        assert!(!store.exists(&path).unwrap());
        store.write_json(&path, &"value").unwrap();
        assert!(store.exists(&path).unwrap());
    }

    #[test]
    fn ensure_dirs_creates_structure() {
        let (_dir, store) = make_store();
        store.ensure_dirs().unwrap();

        assert!(store.root().join("runs").is_dir());
        assert!(store.root().join("cache/embeddings").is_dir());
        assert!(store.root().join("reports").is_dir());
    }
}
