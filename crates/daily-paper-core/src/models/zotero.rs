use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::Author;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroSnapshot {
    pub snapshot_id: String,
    pub user_id: String,
    pub library_version: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub item_count: usize,
    pub items: Vec<LibraryPaper>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroSyncState {
    pub user_id: String,
    pub library_version: Option<u64>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_snapshot_id: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryPaper {
    pub library_id: String,
    pub library_type: LibraryType,
    pub zotero_key: String,
    pub version: Option<u64>,
    pub item_type: String,
    pub parent_item: Option<String>,
    pub title: String,
    pub abstract_text: Option<String>,
    pub authors: Vec<Author>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub url: Option<String>,
    pub collection_keys: Vec<String>,
    pub collections: Vec<CollectionPath>,
    pub tags: Vec<String>,
    pub is_trashed: bool,
    pub date_added: Option<DateTime<Utc>>,
    pub date_modified: Option<DateTime<Utc>>,
    pub attachments: Vec<ZoteroAttachmentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LibraryType {
    User,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionPath {
    pub key: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroAttachmentRef {
    pub key: String,
    pub title: Option<String>,
    pub content_type: Option<String>,
    pub url: Option<String>,
}
