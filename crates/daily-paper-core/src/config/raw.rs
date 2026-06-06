use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawConfig {
    pub zotero: ZoteroConfig,
    pub sources: Vec<SourceConfig>,
    pub embedding: EmbeddingConfig,
    pub reranker: RerankerConfig,
    pub reader: ReaderConfig,
    pub pdf: PdfConfig,
    pub email: EmailConfig,
    pub web: Option<WebConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ZoteroConfig {
    pub user_id: String,
    pub api_key: String,
    pub max_snapshot_age_hours: Option<u64>,
    pub filters: Option<Vec<ZoteroFilterConfig>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ZoteroFilterConfig {
    pub path: String,
    pub weight: Option<f32>,
    pub exclude: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceConfig {
    pub kind: String,
    pub categories: Vec<String>,
    pub include_cross_list: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    pub kind: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub batch_size: Option<usize>,
    pub timeout_secs: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_concurrency: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RerankerConfig {
    pub kind: String,
    pub top_k_library_matches: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReaderConfig {
    pub kind: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub top_n: Option<usize>,
    pub language: Option<String>,
    pub require_full_text: Option<bool>,
    pub on_read_failure: Option<String>,
    pub timeout_secs: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_concurrency: Option<usize>,
    pub max_input_tokens: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PdfConfig {
    pub extractor: String,
    pub timeout_secs: Option<u64>,
    pub max_pdf_mb: Option<u64>,
    pub max_text_chars: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub sender: String,
    pub receiver: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebConfig {
    pub port: Option<u16>,
    pub ui_path: Option<String>,
}
