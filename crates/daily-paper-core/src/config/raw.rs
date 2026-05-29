use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct RawConfig {
    pub state: Option<StateConfig>,
    pub zotero: ZoteroConfig,
    pub sources: Vec<SourceConfig>,
    pub embedding: EmbeddingConfig,
    pub reranker: RerankerConfig,
    pub reader: ReaderConfig,
    pub pdf: PdfConfig,
    pub email: EmailConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StateConfig {
    pub dir: Option<String>,
    pub lock: Option<LockConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LockConfig {
    pub stale_after_minutes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZoteroConfig {
    pub user_id: String,
    pub api_key_env: String,
    pub max_snapshot_age_hours: Option<u64>,
    pub filters: Option<Vec<ZoteroFilterConfig>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZoteroFilterConfig {
    pub path: String,
    pub weight: Option<f32>,
    pub exclude: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourceConfig {
    pub kind: String,
    pub categories: Vec<String>,
    pub include_cross_list: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingConfig {
    pub kind: String,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub model: Option<String>,
    pub batch_size: Option<usize>,
    pub timeout_secs: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_concurrency: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RerankerConfig {
    pub kind: String,
    pub top_k_library_matches: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReaderConfig {
    pub kind: String,
    pub base_url: String,
    pub api_key_env: String,
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

#[derive(Debug, Clone, Deserialize)]
pub struct PdfConfig {
    pub extractor: String,
    pub timeout_secs: Option<u64>,
    pub max_pdf_mb: Option<u64>,
    pub max_text_chars: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub sender: String,
    pub receiver: String,
    pub password_env: String,
}
