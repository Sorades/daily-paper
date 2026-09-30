use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

fn default_snapshot_age_hours() -> u64 {
    168
}

fn default_weight() -> f32 {
    1.0
}

fn default_max_results_per_page() -> usize {
    1000
}

fn default_max_pages() -> usize {
    3
}

fn default_embedding_kind() -> String {
    "openai".to_string()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_batch_size() -> usize {
    128
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_max_retries() -> u32 {
    3
}

fn default_concurrency() -> usize {
    5
}

fn default_reranker_kind() -> String {
    "cosine".to_string()
}

fn default_top_k() -> usize {
    10
}

fn default_top_n() -> usize {
    5
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_on_read_failure() -> String {
    "retry".to_string()
}

fn default_max_input_tokens() -> usize {
    8000
}

fn default_pdf_extractor() -> String {
    "pdftotext".to_string()
}

fn default_pdf_timeout() -> u64 {
    60
}

fn default_max_pdf_mb() -> u64 {
    50
}

fn default_max_text_chars() -> usize {
    100_000
}

fn default_web_port() -> u16 {
    8991
}

fn default_web_ui_path() -> String {
    "ui/build".to_string()
}

fn default_schedule_enabled() -> bool {
    true
}

fn default_schedule_time() -> String {
    "07:30".to_string()
}

/// Unified configuration model for Daily Paper.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub zotero: ZoteroConfig,
    pub sources: Vec<SourceConfig>,
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub reranker: RerankerConfig,
    pub reader: ReaderConfig,
    #[serde(default)]
    pub pdf: PdfConfig,
    pub email: EmailConfig,
    #[serde(default)]
    pub web: WebConfig,
    #[serde(default)]
    pub schedule: ScheduleConfig,
    #[serde(default)]
    pub report_template_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ZoteroConfig {
    pub user_id: String,
    pub api_key: String,
    #[serde(default = "default_snapshot_age_hours")]
    pub max_snapshot_age_hours: u64,
    #[serde(default)]
    pub filters: Vec<ZoteroFilterConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ZoteroFilterConfig {
    pub path: String,
    #[serde(default = "default_weight")]
    pub weight: f32,
    #[serde(default)]
    pub exclude: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceConfig {
    pub kind: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub include_cross_list: bool,
    #[serde(default = "default_max_results_per_page")]
    pub max_results_per_page: usize,
    #[serde(default = "default_max_pages")]
    pub max_pages: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embedding_kind")]
    pub kind: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_concurrency")]
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RerankerConfig {
    #[serde(default = "default_reranker_kind")]
    pub kind: String,
    #[serde(default = "default_top_k")]
    pub top_k_library_matches: usize,
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            kind: default_reranker_kind(),
            top_k_library_matches: default_top_k(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReaderConfig {
    pub kind: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_top_n")]
    pub top_n: usize,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub require_full_text: bool,
    #[serde(default = "default_on_read_failure")]
    pub on_read_failure: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_max_input_tokens")]
    pub max_input_tokens: usize,
    #[serde(default)]
    pub system_prompt_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PdfConfig {
    #[serde(default = "default_pdf_extractor")]
    pub extractor: String,
    #[serde(default = "default_pdf_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_pdf_mb")]
    pub max_pdf_mb: u64,
    #[serde(default = "default_max_text_chars")]
    pub max_text_chars: usize,
}

impl Default for PdfConfig {
    fn default() -> Self {
        Self {
            extractor: default_pdf_extractor(),
            timeout_secs: default_pdf_timeout(),
            max_pdf_mb: default_max_pdf_mb(),
            max_text_chars: default_max_text_chars(),
        }
    }
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
    #[serde(default = "default_web_port")]
    pub port: u16,
    #[serde(default = "default_web_ui_path")]
    pub ui_path: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            port: default_web_port(),
            ui_path: default_web_ui_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleConfig {
    #[serde(default = "default_schedule_enabled")]
    pub enabled: bool,
    #[serde(default = "default_schedule_time")]
    pub time: String,
    #[serde(default)]
    pub timezone: Option<String>,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: default_schedule_enabled(),
            time: default_schedule_time(),
            timezone: None,
        }
    }
}

impl ScheduleConfig {
    pub fn parse_time(&self) -> (u32, u32) {
        if let Some((h, m)) = self.time.split_once(':') {
            if let (Ok(hour), Ok(min)) = (h.trim().parse::<u32>(), m.trim().parse::<u32>()) {
                if hour < 24 && min < 60 {
                    return (hour, min);
                }
            }
        }
        (7, 30)
    }

    pub fn hour(&self) -> u32 {
        self.parse_time().0
    }

    pub fn minute(&self) -> u32 {
        self.parse_time().1
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.zotero.user_id.trim().is_empty() {
            return Err(Error::Config("zotero.user_id is required".into()));
        }
        if self.zotero.api_key.trim().is_empty() {
            return Err(Error::Config("zotero.api_key is required".into()));
        }
        if self.sources.is_empty() {
            return Err(Error::Config("at least one source is required".into()));
        }
        for source in &self.sources {
            if source.kind.trim().is_empty() {
                return Err(Error::Config("source.kind is required".into()));
            }
            if source.kind == "arxiv" && source.categories.is_empty() {
                return Err(Error::Config(
                    "sources.categories must not be empty for arxiv sources".into(),
                ));
            }
        }
        if self.embedding.kind == "fastembed" {
            return Err(Error::Config(
                "fastembed is deprecated; configure an OpenAI-compatible embedding API".into(),
            ));
        }
        if self.embedding.base_url.as_deref().unwrap_or("").trim().is_empty() {
            return Err(Error::Config("embedding.base_url is required".into()));
        }
        if self.embedding.api_key.as_deref().unwrap_or("").trim().is_empty() {
            return Err(Error::Config("embedding.api_key is required".into()));
        }
        if self.reader.base_url.trim().is_empty() {
            return Err(Error::Config("reader.base_url is required".into()));
        }
        if self.reader.api_key.trim().is_empty() {
            return Err(Error::Config("reader.api_key is required".into()));
        }
        if self.reader.model.trim().is_empty() {
            return Err(Error::Config("reader.model is required".into()));
        }
        if self.email.smtp_server.trim().is_empty() {
            return Err(Error::Config("email.smtp_server is required".into()));
        }
        if self.email.sender.trim().is_empty() {
            return Err(Error::Config("email.sender is required".into()));
        }
        if self.email.receiver.trim().is_empty() {
            return Err(Error::Config("email.receiver is required".into()));
        }

        // Validate schedule time format
        if let Some((h, m)) = self.schedule.time.split_once(':') {
            let hour: Result<u32> = h.trim().parse().map_err(|_| Error::Config("invalid schedule hour".into()));
            let min: Result<u32> = m.trim().parse().map_err(|_| Error::Config("invalid schedule minute".into()));
            let (hour, min) = (hour?, min?);
            if hour >= 24 || min >= 60 {
                return Err(Error::Config("schedule time must be between 00:00 and 23:59".into()));
            }
        } else {
            return Err(Error::Config("schedule time must be in HH:MM format".into()));
        }

        Ok(())
    }
}

// Aliases for smooth compatibility during migration
pub type ResolvedConfig = Config;
pub type ResolvedZoteroConfig = ZoteroConfig;
pub type ResolvedZoteroFilter = ZoteroFilterConfig;
pub type ResolvedSourceConfig = SourceConfig;
pub type ResolvedEmbeddingConfig = EmbeddingConfig;
pub type ResolvedRerankerConfig = RerankerConfig;
pub type ResolvedReaderConfig = ReaderConfig;
pub type ResolvedPdfConfig = PdfConfig;
pub type ResolvedEmailConfig = EmailConfig;
pub type ResolvedWebConfig = WebConfig;

#[derive(Debug, Clone)]
pub struct ResolvedScheduleConfig {
    pub enabled: bool,
    pub hour: u32,
    pub minute: u32,
    pub timezone: Option<String>,
}

impl From<&ScheduleConfig> for ResolvedScheduleConfig {
    fn from(s: &ScheduleConfig) -> Self {
        Self {
            enabled: s.enabled,
            hour: s.hour(),
            minute: s.minute(),
            timezone: s.timezone.clone(),
        }
    }
}
