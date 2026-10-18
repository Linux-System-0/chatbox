//! File attachments and pre-signed uploads.

use std::time::Duration;

use sha2::{Digest, Sha256};

use super::{AuthContext, Services};
use crate::error::{Error, Result};
use crate::model::Attachment;

/// The information a client needs to upload a file directly to storage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PresignedUpload {
    /// The attachment id the client must echo back on completion.
    pub attachment_id: i64,
    /// The object storage key.
    pub storage_key: String,
    /// The pre-signed upload URL.
    pub upload_url: String,
    /// URL lifetime in seconds.
    pub expires_in: u64,
}

const PRESIGN_TTL_SECS: u64 = 15 * 60;

fn storage_key(owner_id: i64, attachment_id: i64) -> String {
    format!("attachments/{owner_id}/{attachment_id}")
}

fn validate_filename(filename: &str) -> Result<()> {
    let trimmed = filename.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 255 {
        return Err(Error::Invalid("invalid filename".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(Error::Invalid(
            "filename must not contain path separators".into(),
        ));
    }
    Ok(())
}

impl Services {
    /// Reject uploads above the configured limit.
    fn check_upload_size(&self, size: u64) -> Result<()> {
        match self.config.max_upload_bytes {
            Some(0) => Err(Error::Forbidden("uploads are disabled".into())),
            Some(max) if size > max => Err(Error::PayloadTooLarge(format!(
                "file exceeds the {max} byte upload limit"
            ))),
            _ => Ok(()),
        }
    }

    /// Reserve an attachment id and pre-signed upload URL.
    pub async fn presign_attachment(
        &self,
        ctx: &AuthContext,
        filename: &str,
        content_type: &str,
        size: u64,
    ) -> Result<PresignedUpload> {
        validate_filename(filename)?;
        self.check_upload_size(size)?;
        let attachment_id = self.new_id();
        let key = storage_key(ctx.user_id, attachment_id);
        let ttl = Duration::from_secs(PRESIGN_TTL_SECS);
        let upload_url = self
            .infra
            .objects
            .presign_put(&key, content_type, ttl)
            .await?;
        Ok(PresignedUpload {
            attachment_id,
            storage_key: key,
            upload_url,
            expires_in: PRESIGN_TTL_SECS,
        })
    }

    /// Store an uploaded object. Called by the local upload endpoint used when
    /// the storage backend does not accept direct uploads.
    pub async fn store_upload(
        &self,
        ctx: &AuthContext,
        attachment_id: i64,
        bytes: &[u8],
    ) -> Result<()> {
        self.check_upload_size(bytes.len() as u64)?;
        let key = storage_key(ctx.user_id, attachment_id);
        self.infra.objects.put(&key, bytes).await?;
        Ok(())
    }

    /// Confirm an upload and create the attachment metadata row.
    pub async fn complete_attachment(
        &self,
        ctx: &AuthContext,
        attachment_id: i64,
        filename: &str,
        content_type: &str,
    ) -> Result<Attachment> {
        validate_filename(filename)?;
        let key = storage_key(ctx.user_id, attachment_id);
        let bytes = self
            .infra
            .objects
            .get(&key)
            .await?
            .ok_or_else(|| Error::Invalid("uploaded object not found".into()))?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let attachment = Attachment {
            id: attachment_id,
            owner_id: ctx.user_id,
            filename: filename.trim().to_string(),
            size: bytes.len() as u64,
            mime: content_type.to_string(),
            sha256: format!("{:x}", hasher.finalize()),
            storage_key: key,
            created_at: self.now(),
        };
        self.stores.attachments.create(&attachment).await?;
        Ok(attachment)
    }

    /// Fetch attachment metadata and a time-limited download URL.
    pub async fn get_attachment(
        &self,
        _ctx: &AuthContext,
        attachment_id: i64,
    ) -> Result<(Attachment, String)> {
        let attachment = self
            .stores
            .attachments
            .get(attachment_id)
            .await?
            .ok_or(Error::NotFound)?;
        let url = self
            .infra
            .objects
            .presign_get(
                &attachment.storage_key,
                Duration::from_secs(PRESIGN_TTL_SECS),
            )
            .await?;
        Ok((attachment, url))
    }
}
