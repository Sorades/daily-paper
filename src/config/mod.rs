pub mod loader;
pub mod model;

pub use loader::{default_data_dir, load_config, parse_config_str, validate_config_str};
pub use model::{
    Config, EmailConfig, EmbeddingConfig, PdfConfig, ReaderConfig, RerankerConfig,
    ResolvedConfig, ResolvedEmailConfig, ResolvedEmbeddingConfig, ResolvedPdfConfig,
    ResolvedReaderConfig, ResolvedRerankerConfig, ResolvedScheduleConfig, ResolvedSourceConfig,
    ResolvedWebConfig, ResolvedZoteroConfig, ResolvedZoteroFilter, ScheduleConfig, SourceConfig,
    WebConfig, ZoteroConfig, ZoteroFilterConfig,
};

// RawConfig alias for zero-breakage compatibility
pub type RawConfig = Config;
