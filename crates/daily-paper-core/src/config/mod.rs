mod loader;
mod raw;
mod resolved;
mod validation;

pub use loader::{default_config_path, default_state_dir, is_project_mode, load_config};
pub use raw::{RawConfig, StateConfig};
pub use resolved::{
    ResolvedConfig, ResolvedEmailConfig, ResolvedEmbeddingConfig, ResolvedPdfConfig,
    ResolvedReaderConfig, ResolvedRerankerConfig, ResolvedSourceConfig, ResolvedWebConfig,
    ResolvedZoteroConfig, ResolvedZoteroFilter,
};
