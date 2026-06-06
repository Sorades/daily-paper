use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::Path;
use tracing::info;

use crate::error::{Error, Result};

/// Local embedding client using fastembed (no network required).
pub struct LocalEmbeddingClient {
    model: TextEmbedding,
    batch_size: usize,
}

impl LocalEmbeddingClient {
    /// Create a new local embedding client.
    ///
    /// Downloads the model on first run (~50MB for bge-small-en-v1.5).
    /// `cache_dir` is the parent directory (e.g. `state/cache/`); models are stored in `cache_dir/models/`.
    pub fn new(model_name: &str, batch_size: usize, cache_dir: Option<&Path>) -> Result<Self> {
        info!(model = model_name, "initializing local embedding model");

        let embedding_model = match model_name {
            "BAAI/bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
            "BAAI/bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
            "Xenova/all-MiniLM-L6-v2" => EmbeddingModel::AllMiniLML6V2,
            _ => {
                return Err(Error::Config(format!(
                    "unsupported fastembed model: '{}'",
                    model_name
                )));
            }
        };

        let mut options = InitOptions::new(embedding_model).with_show_download_progress(true);

        // Set custom cache directory if provided
        if let Some(dir) = cache_dir {
            let model_cache = dir.join("models");
            std::fs::create_dir_all(&model_cache)?;
            info!(cache = %model_cache.display(), "using custom model cache");
            options = options.with_cache_dir(model_cache);
        }

        let model = TextEmbedding::try_new(options)
            .map_err(|e| Error::Embedding(format!("failed to load model: {}", e)))?;

        info!("local embedding model loaded");
        Ok(Self { model, batch_size })
    }

    /// Embed a batch of texts, returning vectors in the same order.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let embeddings = self
            .model
            .embed(texts.to_vec(), Some(self.batch_size))
            .map_err(|e| Error::Embedding(format!("local embedding failed: {}", e)))?;

        Ok(embeddings)
    }

    /// Embed a single text.
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let result = self.embed_batch(&[text.to_string()])?;
        result
            .into_iter()
            .next()
            .ok_or_else(|| Error::Embedding("empty result from local embedding".into()))
    }
}
