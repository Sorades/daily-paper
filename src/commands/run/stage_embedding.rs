use std::collections::HashMap;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::config::{ResolvedConfig, ResolvedZoteroFilter};
use crate::embedding::openai::EmbeddingClient;
use crate::embedding::{compute_input_hash, make_input_text};
use crate::models::candidate::CandidatePaper;
use crate::models::common::sha256_hex;
use crate::models::interest::{InterestPaperRef, InterestProfile};
use crate::models::run::*;
use crate::models::zotero::ZoteroSnapshot;
use crate::state::keys::StorageKey;
use crate::state::path::StatePath;
use crate::state::store::FileStateStore;
use crate::zotero::profile::compute_rules_hash;

use crate::progress::StageProgress;

use super::context::{bytes_to_vec, record_stage, vec_to_bytes, ExecutionContext};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EmbeddingIndex {
    pub(crate) candidate_hashes: Vec<(String, String)>,
    pub(crate) library_hashes: Vec<(String, f32, String)>,
}

#[derive(Clone)]
struct PendingEmbedding {
    text: String,
    vec_path: StatePath,
}

pub(crate) async fn stage_embedding(
    cx: &mut ExecutionContext<'_>,
    candidates: &[CandidatePaper],
    snapshot: &ZoteroSnapshot,
    force: bool,
) -> anyhow::Result<(Vec<(String, Vec<f32>)>, Vec<(String, Vec<f32>, f32)>)> {
    let date = cx.date().to_string();
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::Embedding,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let provider_id = "openai";
    let config_hash = sha256_hex(cx.config.embedding.model.as_bytes());

    let mut candidate_embs = Vec::new();
    let mut candidate_hashes: Vec<(String, String)> = Vec::new();
    let mut pending_candidates: Vec<(usize, PendingEmbedding)> = Vec::new();

    for (i, candidate) in candidates.iter().enumerate() {
        let input_text = make_input_text(&candidate.title, &candidate.abstract_text);
        let input_hash = compute_input_hash(
            provider_id,
            &cx.config.embedding.model,
            &config_hash,
            &input_text,
        );
        let key = StorageKey::EmbeddingVector { hash: &input_hash };
        let vec_path = key.to_state_path()?;

        let cached_bytes = if force { None } else { cx.store.get_bytes(&key)? };
        if let Some(bytes) = cached_bytes {
            candidate_embs.push((candidate.paper_id.clone(), bytes_to_vec(&bytes)));
            candidate_hashes.push((candidate.paper_id.clone(), input_hash));
        } else {
            pending_candidates.push((
                i,
                PendingEmbedding {
                    text: input_text,
                    vec_path,
                },
            ));
        }
    }

    let profile = build_interest_profile(snapshot, &cx.config.zotero.filters);
    let mut library_embs = Vec::new();
    let mut library_hashes: Vec<(String, f32, String)> = Vec::new();
    let mut pending_library: Vec<(String, f32, PendingEmbedding)> = Vec::new();

    for pref in &profile.papers {
        if let Some(lib_paper) = snapshot
            .items
            .iter()
            .find(|p| p.library_id == pref.library_id)
        {
            let input_text = match (&lib_paper.title, &lib_paper.abstract_text) {
                (t, Some(a)) if !a.is_empty() => make_input_text(t, a),
                _ => continue,
            };
            let input_hash = compute_input_hash(
                provider_id,
                &cx.config.embedding.model,
                &config_hash,
                &input_text,
            );
            let key = StorageKey::EmbeddingVector { hash: &input_hash };
            let vec_path = key.to_state_path()?;

            let cached_bytes = if force { None } else { cx.store.get_bytes(&key)? };
            if let Some(bytes) = cached_bytes {
                library_embs.push((pref.library_id.clone(), bytes_to_vec(&bytes), pref.weight));
                library_hashes.push((pref.library_id.clone(), pref.weight, input_hash));
            } else {
                pending_library.push((
                    pref.library_id.clone(),
                    pref.weight,
                    PendingEmbedding {
                        text: input_text,
                        vec_path,
                    },
                ));
            }
        }
    }

    let pending_total = pending_candidates.len() + pending_library.len();
    let progress = StageProgress::new(StageName::Embedding, pending_total, cx.event_tx, cx.run_id);

    if !pending_candidates.is_empty() {
        info!(
            count = pending_candidates.len(),
            model = %cx.config.embedding.model,
            "embedding uncached candidate papers via API"
        );
        let pending: Vec<PendingEmbedding> = pending_candidates
            .iter()
            .map(|(_, pending)| pending.clone())
            .collect();
        let embeddings =
            embed_and_cache_pending(cx.store, cx.config, &progress, &pending, "candidates")
                .await?;

        for (idx, emb) in embeddings.into_iter().enumerate() {
            let (candidate_idx, pending) = &pending_candidates[idx];
            let candidate = &candidates[*candidate_idx];
            candidate_embs.push((candidate.paper_id.clone(), emb));
            candidate_hashes.push((
                candidate.paper_id.clone(),
                input_hash_from_vec_path(&pending.vec_path)?,
            ));
        }
    }

    if !pending_library.is_empty() {
        info!(
            count = pending_library.len(),
            model = %cx.config.embedding.model,
            "embedding uncached library papers via API"
        );
        let pending: Vec<PendingEmbedding> = pending_library
            .iter()
            .map(|(_, _, pending)| pending.clone())
            .collect();
        let embeddings =
            embed_and_cache_pending(cx.store, cx.config, &progress, &pending, "library")
                .await?;

        for (idx, emb) in embeddings.into_iter().enumerate() {
            let (lib_id, weight, pending) = &pending_library[idx];
            library_embs.push((lib_id.clone(), emb, *weight));
            library_hashes.push((
                lib_id.clone(),
                *weight,
                input_hash_from_vec_path(&pending.vec_path)?,
            ));
        }
    }

    let index_map: HashMap<&str, usize> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (c.paper_id.as_str(), i))
        .collect();
    candidate_embs.sort_by_key(|(paper_id, _)| {
        index_map
            .get(paper_id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    candidate_hashes.sort_by_key(|(paper_id, _)| {
        index_map
            .get(paper_id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });

    info!(
        candidates = candidate_embs.len(),
        library = library_embs.len(),
        "embedding complete"
    );
    progress.finish(&format!(
        "{} candidates, {} library",
        candidate_embs.len(),
        library_embs.len()
    ));

    let index = EmbeddingIndex {
        candidate_hashes,
        library_hashes,
    };
    cx.store.put_json(&StorageKey::ArchiveEmbeddings { date: &date }, &index)?;

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record_stage(cx.manifest, record);

    Ok((candidate_embs, library_embs))
}

async fn embed_and_cache_pending(
    store: &FileStateStore,
    config: &ResolvedConfig,
    progress: &StageProgress,
    pending: &[PendingEmbedding],
    label: &str,
) -> anyhow::Result<Vec<Vec<f32>>> {
    if pending.is_empty() {
        return Ok(Vec::new());
    }

    let batch_size = config.embedding.batch_size.max(1);

    let client = EmbeddingClient::new(
        config.embedding.base_url.clone().unwrap_or_default(),
        config.embedding.api_key.clone().unwrap_or_default(),
        config.embedding.model.clone(),
        batch_size,
        config.embedding.timeout_secs,
        config.embedding.max_retries,
        config.embedding.max_concurrency,
    );
    let mut all_embeddings = Vec::with_capacity(pending.len());
    let mut saved = 0usize;

    for chunk in pending.chunks(batch_size) {
        progress.inc(&format!("{} embedding batch ({} pending)", label, pending.len() - saved));
        let texts: Vec<String> = chunk.iter().map(|item| item.text.clone()).collect();
        let embeddings = client.embed_batch(&texts).await?;
        validate_embedding_count(chunk.len(), embeddings.len())?;

        for (item, embedding) in chunk.iter().zip(embeddings.iter()) {
            store.write_bytes(&item.vec_path, &vec_to_bytes(embedding))?;
            saved += 1;
            progress.inc(&format!("{} {}/{}", label, saved, pending.len()));
        }

        all_embeddings.extend(embeddings);
    }

    Ok(all_embeddings)
}

fn validate_embedding_count(expected: usize, actual: usize) -> anyhow::Result<()> {
    if actual != expected {
        anyhow::bail!(
            "embedding API returned {} vectors for {} inputs",
            actual,
            expected
        );
    }
    Ok(())
}

fn input_hash_from_vec_path(vec_path: &StatePath) -> anyhow::Result<String> {
    vec_path
        .as_path()
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid embedding vector path: {}", vec_path))
}

pub(crate) fn build_interest_profile(
    snapshot: &ZoteroSnapshot,
    filters: &[ResolvedZoteroFilter],
) -> InterestProfile {
    let mut papers = Vec::new();
    let mut total_weight = 0.0f32;

    for item in &snapshot.items {
        if item.is_trashed {
            continue;
        }

        let mut weight = 0.0f32;
        let mut matched_rules = Vec::new();
        let mut excluded = false;

        for filter in filters {
            let matches = item
                .collections
                .iter()
                .any(|c| path_matches_filter(&c.path, &filter.path));
            if matches {
                if filter.exclude {
                    excluded = true;
                    break;
                }
                weight += filter.weight;
                matched_rules.push(filter.path.clone());
            }
        }

        if excluded {
            continue;
        }

        if weight == 0.0 {
            weight = 1.0;
        }

        papers.push(InterestPaperRef {
            library_id: item.library_id.clone(),
            weight,
            matched_rules,
        });
        total_weight += weight;
    }

    let rules_hash = compute_rules_hash(
        &filters
            .iter()
            .map(|f| (f.path.clone(), f.weight, f.exclude))
            .collect::<Vec<_>>(),
    );

    InterestProfile {
        profile_id: format!("{}-{}", snapshot.snapshot_id, rules_hash),
        zotero_snapshot_id: snapshot.snapshot_id.clone(),
        created_at: Utc::now(),
        rules_hash,
        paper_count: papers.len(),
        total_weight,
        papers,
    }
}

pub(crate) fn path_matches_filter(collection_path: &str, filter_pattern: &str) -> bool {
    let prefix = filter_pattern
        .trim_end_matches("/**")
        .trim_end_matches("/*");
    if prefix == filter_pattern {
        collection_path == filter_pattern
    } else {
        collection_path.starts_with(prefix)
    }
}
