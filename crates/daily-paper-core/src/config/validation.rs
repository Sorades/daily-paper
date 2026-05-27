use crate::error::{Error, Result};
use super::raw::RawConfig;

/// Validate required fields in the raw config.
pub fn _validate(raw: &RawConfig) -> Result<()> {
    if raw.zotero.user_id.is_empty() {
        return Err(Error::Config("zotero.user_id is required".into()));
    }
    if raw.zotero.api_key_env.is_empty() {
        return Err(Error::Config("zotero.api_key_env is required".into()));
    }
    if raw.sources.is_empty() {
        return Err(Error::Config("at least one source is required".into()));
    }
    if raw.embedding.base_url.is_empty() {
        return Err(Error::Config("embedding.base_url is required".into()));
    }
    if raw.embedding.api_key_env.is_empty() {
        return Err(Error::Config("embedding.api_key_env is required".into()));
    }
    if raw.embedding.model.is_empty() {
        return Err(Error::Config("embedding.model is required".into()));
    }
    if raw.reader.base_url.is_empty() {
        return Err(Error::Config("reader.base_url is required".into()));
    }
    if raw.reader.api_key_env.is_empty() {
        return Err(Error::Config("reader.api_key_env is required".into()));
    }
    if raw.reader.model.is_empty() {
        return Err(Error::Config("reader.model is required".into()));
    }
    if raw.email.smtp_server.is_empty() {
        return Err(Error::Config("email.smtp_server is required".into()));
    }
    if raw.email.sender.is_empty() {
        return Err(Error::Config("email.sender is required".into()));
    }
    if raw.email.receiver.is_empty() {
        return Err(Error::Config("email.receiver is required".into()));
    }
    Ok(())
}
