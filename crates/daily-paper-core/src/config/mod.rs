mod loader;
mod raw;
mod resolved;
mod validation;

pub use loader::{load_config, default_config_path, default_state_dir, is_project_mode, init_project_dir};
pub use raw::{RawConfig, StateConfig};
pub use resolved::{
    ResolvedConfig, ResolvedZoteroConfig, ResolvedZoteroFilter,
    ResolvedSourceConfig, ResolvedEmbeddingConfig, ResolvedRerankerConfig,
    ResolvedReaderConfig, ResolvedPdfConfig, ResolvedEmailConfig,
};
