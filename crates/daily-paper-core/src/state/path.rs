use std::path::{Path, PathBuf};
use crate::error::{Error, Result};

/// A validated, relative path within the state store.
///
/// Rejects absolute paths, `..` segments, and empty paths to prevent
/// path traversal attacks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatePath(PathBuf);

impl StatePath {
    /// Create a new StatePath from path segments.
    ///
    /// Returns an error if any segment is `..`, the path is absolute,
    /// or the path is empty.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if path.is_absolute() {
            return Err(Error::Storage(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("state path must be relative, got: {}", path.display()),
            )));
        }

        for component in path.components() {
            match component {
                std::path::Component::Normal(_) => {}
                std::path::Component::ParentDir => {
                    return Err(Error::Storage(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("state path must not contain '..', got: {}", path.display()),
                    )));
                }
                _ => {
                    return Err(Error::Storage(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("invalid path component in: {}", path.display()),
                    )));
                }
            }
        }

        if path.as_os_str().is_empty() {
            return Err(Error::Storage(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "state path must not be empty",
            )));
        }

        Ok(Self(path.to_path_buf()))
    }

    /// Resolve this state path against a root directory.
    pub fn resolve(&self, root: &Path) -> PathBuf {
        root.join(&self.0)
    }

    /// Get the inner path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for StatePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for StatePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_relative_path() {
        let p = StatePath::new("state/runs/abc.json").unwrap();
        assert_eq!(p.as_path(), Path::new("state/runs/abc.json"));
    }

    #[test]
    fn reject_absolute_path() {
        assert!(StatePath::new("/etc/passwd").is_err());
    }

    #[test]
    fn reject_parent_dir() {
        assert!(StatePath::new("state/../secret").is_err());
    }

    #[test]
    fn reject_leading_parent_dir() {
        assert!(StatePath::new("../secret").is_err());
    }

    #[test]
    fn reject_empty_path() {
        assert!(StatePath::new("").is_err());
    }

    #[test]
    fn resolve_against_root() {
        let p = StatePath::new("cache/embeddings/abc.json").unwrap();
        let resolved = p.resolve(Path::new("/home/user/.local/share/daily-paper"));
        assert_eq!(
            resolved,
            PathBuf::from("/home/user/.local/share/daily-paper/cache/embeddings/abc.json")
        );
    }

    #[test]
    fn reject_dot() {
        // "." is not a normal component, should be rejected
        assert!(StatePath::new(".").is_err());
    }
}
