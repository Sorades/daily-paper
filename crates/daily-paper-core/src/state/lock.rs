use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use crate::error::{Error, Result};

#[derive(Debug, Serialize, Deserialize)]
pub struct LockInfo {
    pub pid: u32,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
}

/// A held lock. Dropping this struct releases the lock.
pub struct RunLock {
    lock_path: PathBuf,
}

impl RunLock {
    /// Try to acquire the lock at the given path.
    ///
    /// If the lock file already exists, checks if it's stale (older than
    /// `stale_after`). If stale, removes it and retries. If not stale,
    /// returns an error.
    pub fn acquire(lock_dir: &Path, run_id: &str, stale_after: Duration) -> Result<Self> {
        let lock_path = lock_dir.join("lock");

        if lock_path.exists() {
            // Try to read existing lock info
            match read_lock_info(&lock_path) {
                Ok(info) => {
                    let age = file_age(&lock_path).unwrap_or(Duration::MAX);
                    if age > stale_after {
                        tracing::warn!(
                            pid = info.pid,
                            run_id = %info.run_id,
                            age_secs = age.as_secs(),
                            "removing stale lock"
                        );
                        std::fs::remove_file(&lock_path)?;
                    } else {
                        return Err(Error::Config(format!(
                            "another run is active (pid={}, run_id={}, age={}s)",
                            info.pid,
                            info.run_id,
                            age.as_secs()
                        )));
                    }
                }
                Err(_) => {
                    // Can't read lock file, try to check age and remove
                    let age = file_age(&lock_path).unwrap_or(Duration::MAX);
                    if age > stale_after {
                        tracing::warn!(age_secs = age.as_secs(), "removing unreadable stale lock");
                        std::fs::remove_file(&lock_path)?;
                    } else {
                        return Err(Error::Config(
                            "lock file exists but cannot be read, and is not stale".into(),
                        ));
                    }
                }
            }
        }

        // Try to create lock file atomically
        let lock_info = LockInfo {
            pid: std::process::id(),
            run_id: run_id.to_string(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_vec_pretty(&lock_info)?;

        // Use create_new semantics: fail if file already exists
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(&json)?;
                file.flush()?;
                Ok(RunLock { lock_path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(Error::Config(
                    "failed to acquire lock: another process acquired it first".into(),
                ))
            }
            Err(e) => Err(Error::Storage(e)),
        }
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        if self.lock_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.lock_path) {
                tracing::warn!(error = %e, "failed to remove lock file on drop");
            }
        }
    }
}

fn read_lock_info(path: &Path) -> Result<LockInfo> {
    let data = std::fs::read(path)?;
    let info: LockInfo = serde_json::from_slice(&data)?;
    Ok(info)
}

fn file_age(path: &Path) -> Option<Duration> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    modified.elapsed().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn acquire_and_release() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path();

        let lock = RunLock::acquire(lock_dir, "run-1", Duration::from_secs(3600)).unwrap();
        assert!(lock_dir.join("lock").exists());

        drop(lock);
        assert!(!lock_dir.join("lock").exists());
    }

    #[test]
    fn second_acquire_fails() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path();

        let _lock = RunLock::acquire(lock_dir, "run-1", Duration::from_secs(3600)).unwrap();
        let result = RunLock::acquire(lock_dir, "run-2", Duration::from_secs(3600));
        assert!(result.is_err());
    }

    #[test]
    fn stale_lock_is_removed() {
        let dir = TempDir::new().unwrap();
        let lock_dir = dir.path();

        // Create a lock with zero stale_after (immediately stale)
        let lock = RunLock::acquire(lock_dir, "run-1", Duration::from_secs(3600)).unwrap();

        // Manually drop and recreate as a stale lock
        drop(lock);

        // Write a lock file manually (simulating a crashed process)
        let lock_info = LockInfo {
            pid: 99999,
            run_id: "old-run".to_string(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_vec_pretty(&lock_info).unwrap();
        std::fs::write(lock_dir.join("lock"), &json).unwrap();

        // Should succeed with zero stale_after
        let lock = RunLock::acquire(lock_dir, "run-2", Duration::ZERO).unwrap();
        assert!(lock_dir.join("lock").exists());
        drop(lock);
    }
}
