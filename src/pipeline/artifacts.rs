use serde::{Deserialize, Serialize};

use crate::models::read::ReadResult;
use crate::rerank::selection::ReadSelection;

/// Strongly-typed artifact produced by Stage 4: Embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingArtifact {
    pub candidate_embeddings: Vec<(String, Vec<f32>)>,
    pub library_embeddings: Vec<(String, Vec<f32>, f32)>,
}

/// Strongly-typed artifact produced by Stage 5: Rerank.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankArtifact {
    pub selection: ReadSelection,
    pub scores: Vec<(String, f32)>,
}

/// Strongly-typed artifact produced by Stage 6-9: DeepRead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepReadArtifact {
    pub results: Vec<ReadResult>,
}

/// Strongly-typed artifact produced by Stage 10: Render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderArtifact {
    pub html_path: String,
    pub text_path: Option<String>,
}
