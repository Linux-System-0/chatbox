//! Local filesystem object storage.
//!
//! Development backend that stores uploaded objects under a directory. The
//! pre-signed URLs point back at the API's upload/download endpoints. In a
//! production deployment this adapter is replaced by an S3-compatible one
//! behind the same `ObjectStorage` port.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};
use chatbox_core::infra::ObjectStorage;

/// Object storage rooted at a local directory.
#[derive(Clone)]
pub struct LocalObjectStorage {
    root: PathBuf,
    base_url: String,
}

impl LocalObjectStorage {
    /// Create the storage root if needed.
    pub fn new(root: impl Into<PathBuf>, base_url: impl Into<String>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(Error::storage)?;
        Ok(Self {
            root,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    fn path_for(&self, key: &str) -> Result<PathBuf> {
        if key.contains("..") || key.starts_with('/') {
            return Err(Error::Invalid("invalid object key".into()));
        }
        Ok(self.root.join(key))
    }

    fn id_of(key: &str) -> &str {
        key.rsplit('/').next().unwrap_or_default()
    }
}

#[async_trait]
impl ObjectStorage for LocalObjectStorage {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path_for(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(Error::storage)?;
        }
        tokio::fs::write(&path, bytes)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path_for(key)?;
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::storage(e)),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.path_for(key)?;
        Ok(Path::new(&path).exists())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.path_for(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::storage(e)),
        }
    }

    async fn presign_put(&self, key: &str, _content_type: &str, _ttl: Duration) -> Result<String> {
        Ok(format!(
            "{}/api/v1/files/upload/{}",
            self.base_url,
            Self::id_of(key)
        ))
    }

    async fn presign_get(&self, key: &str, _ttl: Duration) -> Result<String> {
        Ok(format!(
            "{}/api/v1/files/{}/content",
            self.base_url,
            Self::id_of(key)
        ))
    }
}
