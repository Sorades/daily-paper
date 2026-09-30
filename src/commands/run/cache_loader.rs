use std::collections::HashSet;
use tracing::{info, warn};

use crate::models::candidate::CandidatePaper;
use crate::models::dedup::DedupResult;
use crate::models::read::ReadResult;
use crate::models::run::*;
use crate::models::zotero::ZoteroSnapshot;
use crate::rerank::selection::ReadSelection;
use crate::state::keys::StorageKey;
use crate::state::path::StatePath;
use crate::state::store::FileStateStore;

use super::context::bytes_to_vec;
use super::stage_embedding::EmbeddingIndex;

pub(crate) fn find_source_run(
    store: &FileStateStore,
    from_run: Option<&str>,
    stage_filter: Option<&[StageName]>,
) -> anyhow::Result<String> {
    if let Some(id) = from_run {
        return Ok(id.to_string());
    }
    let runs_dir = store.resolve_key(&StorageKey::RunsDir)?;
    if !runs_dir.exists() {
        anyhow::bail!("no runs found");
    }
    let mut entries: Vec<_> = std::fs::read_dir(&runs_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("manifest.json").exists())
        .collect();
    entries.sort_by(|a, b| {
        b.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .cmp(
                &a.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            )
    });

    for entry in &entries {
        let run_id = entry.file_name().to_string_lossy().to_string();
        if let Ok(Some(manifest)) =
            store.get_json::<RunManifest>(&StorageKey::RunManifest { run_id: &run_id })
        {
            if manifest.stages.is_empty() {
                continue;
            }
            if let Some(filter) = stage_filter {
                let required_stages: Vec<StageName> =
                    filter.iter().flat_map(|s| s.required_stages()).collect();
                let has_all = required_stages
                    .iter()
                    .all(|required| manifest.stages.iter().any(|s| &s.stage == required));
                if !has_all {
                    continue;
                }
            }
            return Ok(run_id);
        }
    }

    anyhow::bail!("no runs found with completed stages")
}

pub(crate) fn stage_output_ref(manifest: Option<&RunManifest>, stage: StageName) -> Option<&str> {
    manifest?
        .stages
        .iter()
        .find(|s| s.stage == stage && s.status == StageStatus::Succeeded)
        .and_then(|s| s.output_ref.as_deref())
}

pub(crate) fn load_cached_snapshot(
    store: &FileStateStore,
    date: &str,
    source: Option<&RunManifest>,
) -> anyhow::Result<ZoteroSnapshot> {
    if let Some(snapshot_id) = stage_output_ref(source, StageName::ZoteroSync) {
        if let Some(snapshot) =
            store.get_json::<ZoteroSnapshot>(&StorageKey::ZoteroSnapshot { snapshot_id })?
        {
            return Ok(snapshot);
        }
        anyhow::bail!("cached snapshot not found: {}", snapshot_id);
    }

    if let Some(snapshot_id) =
        store.get_string(&StorageKey::ArchiveSnapshotId { date })?
    {
        if let Some(snapshot) =
            store.get_json::<ZoteroSnapshot>(&StorageKey::ZoteroSnapshot { snapshot_id: &snapshot_id })?
        {
            return Ok(snapshot);
        }
    }

    let snapshots_dir = store.resolve_key(&StorageKey::ZoteroSnapshotsDir)?;
    if snapshots_dir.exists() {
        let mut entries: Vec<_> = std::fs::read_dir(&snapshots_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "json")
                    .unwrap_or(false)
            })
            .collect();
        entries.sort_by(|a, b| {
            b.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .cmp(
                    &a.metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                )
        });
        if let Some(entry) = entries.first() {
            let relative_path = entry
                .path()
                .strip_prefix(store.root())
                .unwrap_or(&entry.path())
                .to_string_lossy()
                .to_string();
            let path = StatePath::new(relative_path)?;
            if let Some(snapshot) = store.read_json::<ZoteroSnapshot>(&path)? {
                return Ok(snapshot);
            }
        }
    }

    anyhow::bail!("no cached snapshot found for date {}", date)
}

pub(crate) fn load_cached_candidates(store: &FileStateStore, date: &str) -> anyhow::Result<DedupResult> {
    if let Some(dedup) = store.get_json::<DedupResult>(&StorageKey::ArchiveDedup { date })? {
        info!(
            kept = dedup.candidates.len(),
            "loaded dedup result from date cache"
        );
        return Ok(dedup);
    }

    anyhow::bail!(
        "no cached candidates found for date {}; run source-fetch first",
        date
    )
}

#[allow(clippy::type_complexity)]
pub(crate) fn load_cached_embeddings(
    store: &FileStateStore,
    date: &str,
    candidates: &[CandidatePaper],
    _snapshot: &ZoteroSnapshot,
) -> anyhow::Result<(Vec<(String, Vec<f32>)>, Vec<(String, Vec<f32>, f32)>)> {
    if let Some(index) = store.get_json::<EmbeddingIndex>(&StorageKey::ArchiveEmbeddings { date })? {
        let current_ids: HashSet<&str> =
            candidates.iter().map(|c| c.paper_id.as_str()).collect();
        let cached_ids: HashSet<&str> = index
            .candidate_hashes
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        if current_ids != cached_ids {
            warn!(
                current = current_ids.len(),
                cached = cached_ids.len(),
                "cached embeddings candidate set mismatch; consider re-running with --stage embedding"
            );
        }

        let mut candidate_embs = Vec::new();
        for (paper_id, input_hash) in &index.candidate_hashes {
            if let Some(bytes) = store.get_bytes(&StorageKey::EmbeddingVector { hash: input_hash })? {
                let vec = bytes_to_vec(&bytes);
                candidate_embs.push((paper_id.clone(), vec));
            } else {
                anyhow::bail!("embedding .vec file not found: {}", input_hash);
            }
        }

        let mut library_embs = Vec::new();
        for (lib_id, weight, input_hash) in &index.library_hashes {
            if let Some(bytes) = store.get_bytes(&StorageKey::EmbeddingVector { hash: input_hash })? {
                let vec = bytes_to_vec(&bytes);
                library_embs.push((lib_id.clone(), vec, *weight));
            } else {
                anyhow::bail!("embedding .vec file not found: {}", input_hash);
            }
        }

        return Ok((candidate_embs, library_embs));
    }

    anyhow::bail!("no cached embeddings found for date {}", date)
}

pub(crate) fn load_cached_rerank(
    store: &FileStateStore,
    date: &str,
    source: Option<&RunManifest>,
) -> anyhow::Result<(ReadSelection, Vec<(String, f32)>)> {
    if let Some(selection_id) = stage_output_ref(source, StageName::Rerank) {
        if let Some(selection) =
            store.get_json::<ReadSelection>(&StorageKey::RerankSelection { selection_id })?
        {
            let scores = selection.paper_scores.clone();
            return Ok((selection, scores));
        }
        anyhow::bail!("cached rerank not found: {}", selection_id);
    }

    if let Some(rerank) = store.get_json::<ReadSelection>(&StorageKey::ArchiveRerank { date })? {
        let scores = rerank.paper_scores.clone();
        return Ok((rerank, scores));
    }

    anyhow::bail!("no cached rerank found for date {}", date)
}

pub(crate) fn load_cached_read_results(
    store: &FileStateStore,
    date: &str,
    source: Option<&RunManifest>,
) -> anyhow::Result<Vec<ReadResult>> {
    if let Some(read_ref) = stage_output_ref(source, StageName::DeepRead) {
        let mut results = Vec::new();
        for paper_id in read_ref.split(',').filter(|id| !id.is_empty()) {
            if let Some(result) =
                store.get_json::<ReadResult>(&StorageKey::ArchiveReadResult { date, paper_id })?
            {
                results.push(result);
            }
        }
        if !results.is_empty() {
            return Ok(results);
        }
    }

    let read_dir = store.resolve_key(&StorageKey::ArchiveReadDir { date })?;
    if read_dir.exists() {
        let mut results = Vec::new();
        for entry in std::fs::read_dir(&read_dir)?.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|ext| ext == "json").unwrap_or(false) {
                let relative_path = path
                    .strip_prefix(store.root())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                let state_path = StatePath::new(relative_path)?;
                if let Some(r) = store.read_json::<ReadResult>(&state_path)? {
                    results.push(r);
                }
            }
        }
        if !results.is_empty() {
            return Ok(results);
        }
    }

    Ok(Vec::new())
}

pub(crate) fn load_cached_render(
    store: &FileStateStore,
    date: &str,
    source: Option<&RunManifest>,
) -> anyhow::Result<(String, Option<String>)> {
    if let Some(source) = source {
        let html_key = StorageKey::RunReportHtml { run_id: &source.run_id };
        let text_key = StorageKey::RunReportText { run_id: &source.run_id };
        let html_path = store.resolve_key(&html_key)?;
        if html_path.exists() {
            let text_path = store.resolve_key(&text_key)?;
            return Ok((
                html_path.to_string_lossy().to_string(),
                if text_path.exists() {
                    Some(text_path.to_string_lossy().to_string())
                } else {
                    None
                },
            ));
        }
    }

    let archive_html_key = StorageKey::ArchiveReportHtml { date };
    if store.key_exists(&archive_html_key)? {
        let html_path = store.resolve_key(&archive_html_key)?;
        let html = html_path.to_string_lossy().to_string();
        let archive_text_key = StorageKey::ArchiveReportText { date };
        let text = if store.key_exists(&archive_text_key)? {
            Some(
                store
                    .resolve_key(&archive_text_key)?
                    .to_string_lossy()
                    .to_string(),
            )
        } else {
            None
        };
        return Ok((html, text));
    }

    anyhow::bail!("no cached report found for date {}", date)
}
