use std::path::Path;

use anyhow::Context;
use chrono::{Duration, Local, NaiveDate, TimeZone, Utc};
use tracing::{debug, info, warn};

use crate::cli::RunArgs;
use daily_paper_core::config::load_config;
use daily_paper_core::config::ResolvedConfig;
use daily_paper_core::embedding::openai::EmbeddingClient;
use daily_paper_core::metadata::fetcher::extract_from_source;
use daily_paper_core::models::candidate::CandidatePaper;
use daily_paper_core::models::common::{sha256_hex, DateWindow};
use daily_paper_core::models::dedup::{DedupResult, DuplicateReason, ExistingLibraryMatch};
use daily_paper_core::models::interest::{InterestPaperRef, InterestProfile};
use daily_paper_core::models::read::{compute_read_cache_key, PaperMetadataSummary, ReadResult};
use daily_paper_core::models::report::{compute_delivery_key, RenderedReport};
use daily_paper_core::models::run::*;
use daily_paper_core::models::zotero::ZoteroSnapshot;
use daily_paper_core::pdf::download::download_pdf;
use daily_paper_core::pdf::extract::extract_text;
use daily_paper_core::pdf::section::{parse_sections, select_sections_for_reading};
use daily_paper_core::reader::openai::ReaderClient;
use daily_paper_core::reader::template::{
    build_system_prompt, build_user_prompt, compute_template_hash, trim_to_token_budget,
};
use daily_paper_core::render::html::{render_html, ReportPaper};
use daily_paper_core::render::text::render_text;
use daily_paper_core::rerank::cosine::rerank;
use daily_paper_core::rerank::selection::{
    compute_candidate_set_hash, compute_rerank_cache_key, select_top_n,
};
use daily_paper_core::source::arxiv::client::ArxivClient;
use daily_paper_core::state::path::StatePath;
use daily_paper_core::state::store::FileStateStore;
use daily_paper_core::zotero::client::ZoteroClient;
use daily_paper_core::zotero::convert::zotero_item_to_library_paper;
use daily_paper_core::zotero::profile::{
    build_collection_paths, compute_rules_hash, compute_snapshot_id,
};

pub async fn execute(config_path: &Path, state_dir: &Path, args: RunArgs) -> anyhow::Result<()> {
    let (_raw, resolved) = load_config(config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;

    info!(config = %config_path.display(), "config loaded");
    info!(state_dir = %state_dir.display(), "state directory");

    let store = FileStateStore::new(state_dir.to_path_buf());
    store.ensure_dirs()?;

    // Parse --stage filter
    let stage_filter: Option<Vec<StageName>> = if args.stages.is_empty() {
        None
    } else {
        let mut stages = Vec::new();
        for s in &args.stages {
            let stage = StageName::from_kebab(s).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown stage '{}'. Valid: zotero-sync, source-fetch, deduplicate, embedding, rerank, deep-read, render, send",
                    s
                )
            })?;
            stages.push(stage);
        }
        Some(stages)
    };

    // If --stage is specified, load source run manifest for cached data
    let source_manifest: Option<RunManifest> = if stage_filter.is_some() {
        let source_run_id = find_source_run(&store, args.from_run.as_deref())?;
        info!(source_run = %source_run_id, "loading source run for cached data");
        let manifest_path = StatePath::new(format!("runs/{}/manifest.json", source_run_id))?;
        let m: Option<RunManifest> = store.read_json(&manifest_path)?;
        if m.is_none() {
            warn!("source run manifest not found, stages will need to re-fetch data");
        }
        m
    } else {
        None
    };

    let run_id = generate_run_id();
    info!(run_id = %run_id, "starting pipeline run");

    let date_window = compute_date_window(args.date.as_deref())?;
    info!(
        start = %date_window.start,
        end = %date_window.end,
        label = %date_window.label,
        "date window"
    );

    let mut manifest = RunManifest {
        run_id: run_id.clone(),
        parent_run_id: source_manifest.as_ref().map(|m| m.run_id.clone()),
        status: RunStatus::Running,
        started_at: Utc::now(),
        finished_at: None,
        config_hash: sha256_hex(config_path.to_string_lossy().as_bytes()),
        cli_overrides: build_cli_overrides(&args),
        date_window: date_window.clone(),
        stages: Vec::new(),
        warnings: Vec::new(),
        error: None,
    };

    let manifest_path = StatePath::new(format!("runs/{}/manifest.json", run_id))?;
    store.write_json(&manifest_path, &manifest)?;

    let _lock = store
        .acquire_lock(&run_id, std::time::Duration::from_secs(3600))
        .context("failed to acquire run lock")?;

    let result = run_pipeline(
        &store,
        &resolved,
        &mut manifest,
        &args,
        &run_id,
        &date_window,
        stage_filter.as_deref(),
        source_manifest.as_ref(),
        None,
        &manifest_path,
    )
    .await;

    manifest.finished_at = Some(Utc::now());
    match &result {
        Ok(()) => {
            manifest.status = RunStatus::Succeeded;
            info!(run_id = %run_id, "pipeline completed successfully");
        }
        Err(e) => {
            manifest.status = RunStatus::Failed;
            manifest.error = Some(ErrorRecord {
                kind: classify_error(e),
                message: e.to_string(),
                retryable: false,
                context: serde_json::Value::Null,
                occurred_at: Utc::now(),
            });
            warn!(run_id = %run_id, error = %e, "pipeline failed");
        }
    }

    let _ = store.write_json(&manifest_path, &manifest);
    result
}

#[allow(clippy::too_many_arguments)]
pub async fn run_pipeline(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    args: &RunArgs,
    run_id: &str,
    date_window: &DateWindow,
    stage_filter: Option<&[StageName]>,
    source_manifest: Option<&RunManifest>,
    event_tx: Option<&tokio::sync::broadcast::Sender<PipelineEvent>>,
    manifest_path: &StatePath,
) -> anyhow::Result<()> {
    let should_run =
        |stage: &StageName| -> bool { stage_filter.map(|f| f.contains(stage)).unwrap_or(true) };

    // Stage 1: Zotero sync
    let snapshot = if should_run(&StageName::ZoteroSync) {
        emit_event(event_tx, run_id, StageName::ZoteroSync, true);
        let r = stage_zotero_sync(store, config, manifest, args.force_zotero_sync).await;
        emit_stage_end(event_tx, manifest, run_id, StageName::ZoteroSync, &r);
        let _ = store.write_json(manifest_path, manifest);
        r?
    } else {
        load_cached_snapshot(store, source_manifest)?
    };

    // Stage 2: Source fetch
    let candidates = if should_run(&StageName::SourceFetch) {
        emit_event(event_tx, run_id, StageName::SourceFetch, true);
        let r = stage_source_fetch(store, config, manifest, date_window).await;
        emit_stage_end(event_tx, manifest, run_id, StageName::SourceFetch, &r);
        let _ = store.write_json(manifest_path, manifest);
        r?
    } else {
        load_cached_candidates(store, source_manifest)?
    };

    if candidates.is_empty() {
        warn!("no candidate papers found for date window");
        add_warning(manifest, "source", "no candidate papers found");
        return Ok(());
    }

    // Stage 3: Deduplicate
    let mut dedup = if should_run(&StageName::Deduplicate) {
        emit_event(event_tx, run_id, StageName::Deduplicate, true);
        let r = stage_deduplicate(store, manifest, run_id, &candidates, &snapshot).await;
        emit_stage_end(event_tx, manifest, run_id, StageName::Deduplicate, &r);
        let _ = store.write_json(manifest_path, manifest);
        r?
    } else {
        load_cached_dedup(store, source_manifest, &candidates, &snapshot)?
    };

    if dedup.candidates.is_empty() {
        warn!("all candidates were duplicates");
        add_warning(manifest, "dedup", "all candidates were duplicates");
        return Ok(());
    }

    // Limit candidates if --max-candidates is set
    if let Some(max) = args.max_candidates {
        if dedup.candidates.len() > max {
            info!(
                limit = max,
                total = dedup.candidates.len(),
                "limiting candidates"
            );
            dedup.candidates.truncate(max);
        }
    }

    // Stage 4: Embedding
    let (candidate_embs, library_embs) = if should_run(&StageName::Embedding) {
        emit_event(event_tx, run_id, StageName::Embedding, true);
        let r = stage_embedding(store, config, manifest, &dedup.candidates, &snapshot).await;
        emit_stage_end(event_tx, manifest, run_id, StageName::Embedding, &r);
        let _ = store.write_json(manifest_path, manifest);
        r?
    } else {
        load_cached_embeddings(store, source_manifest, &dedup.candidates, &snapshot)?
    };

    // Stage 5: Rerank + selection
    let (selection, rerank_scores) = if should_run(&StageName::Rerank) {
        emit_event(event_tx, run_id, StageName::Rerank, true);
        let r = stage_rerank(
            store,
            config,
            manifest,
            &dedup.candidates,
            &candidate_embs,
            &library_embs,
            &snapshot,
        )
        .await;
        emit_stage_end(event_tx, manifest, run_id, StageName::Rerank, &r);
        let _ = store.write_json(manifest_path, manifest);
        r?
    } else {
        load_cached_rerank(store, source_manifest)?
    };

    if selection.selected_paper_ids.is_empty() {
        warn!("no papers selected after rerank");
        add_warning(manifest, "rerank", "no papers selected");
        return Ok(());
    }

    // Stage 6-9: Deep read (PDF + extract + metadata + LLM)
    let read_results = if should_run(&StageName::DeepRead) {
        emit_event(event_tx, run_id, StageName::DeepRead, true);
        let r = stage_deep_read(
            store,
            config,
            manifest,
            &selection.selected_paper_ids,
            &dedup.candidates,
            args.force_read,
        )
        .await;
        emit_stage_end(event_tx, manifest, run_id, StageName::DeepRead, &r);
        let _ = store.write_json(manifest_path, manifest);
        r?
    } else {
        load_cached_read_results(store, source_manifest)?
    };

    // Stage 10: Render
    let (html_path, text_path) = if should_run(&StageName::Render) {
        emit_event(event_tx, run_id, StageName::Render, true);
        let r = stage_render(
            store,
            manifest,
            run_id,
            &selection.selected_paper_ids,
            &dedup.candidates,
            &read_results,
            &rerank_scores,
        )
        .await;
        emit_stage_end(event_tx, manifest, run_id, StageName::Render, &r);
        let _ = store.write_json(manifest_path, manifest);
        r?
    } else {
        load_cached_render(store, source_manifest, run_id)?
    };

    // Stage 11: Send
    // Dev (debug): skip email unless --send-email
    // Release: send email unless --no-email or --dry-run
    if should_run(&StageName::Send) {
        let should_send = if args.dry_run || args.no_email {
            false
        } else if args.send_email {
            true
        } else if cfg!(debug_assertions) {
            false // dev default: don't send
        } else {
            true // release default: send
        };

        if !should_send {
            if args.dry_run {
                info!("dry-run: skipping email send");
            } else {
                info!("email skipped (use --send-email to deliver in dev)");
            }
            skip_stage(manifest, StageName::Send);
        } else {
            emit_event(event_tx, run_id, StageName::Send, true);
            let r = stage_send(
                store,
                config,
                manifest,
                run_id,
                &html_path,
                text_path.as_deref(),
                args.force_send,
            )
            .await;
            emit_stage_end(event_tx, manifest, run_id, StageName::Send, &r);
            let _ = store.write_json(manifest_path, manifest);
            r?
        }
    }

    Ok(())
}

// ── Stage implementations ───────────────────────────────────────────

async fn stage_zotero_sync(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    force: bool,
) -> anyhow::Result<ZoteroSnapshot> {
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::ZoteroSync,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    // Check for cached snapshot
    let sync_state_path = StatePath::new("cache/zotero/sync-state.json")?;
    let sync_state: Option<daily_paper_core::models::zotero::ZoteroSyncState> =
        store.read_json(&sync_state_path)?;

    if !force {
        if let Some(ref state) = sync_state {
            if let (Some(snapshot_id), Some(last_success)) =
                (&state.last_snapshot_id, state.last_success_at)
            {
                let age = Utc::now() - last_success;
                if age < Duration::hours(config.zotero.max_snapshot_age_hours as i64) {
                    let snap_path =
                        StatePath::new(format!("cache/zotero/snapshots/{}.json", snapshot_id))?;
                    if let Some(snapshot) = store.read_json::<ZoteroSnapshot>(&snap_path)? {
                        info!(
                            snapshot_id = %snapshot_id,
                            age_hours = age.num_hours(),
                            "using cached Zotero snapshot"
                        );
                        record.status = StageStatus::Succeeded;
                        record.cache_hit = true;
                        record.finished_at = Some(Utc::now());
                        record.output_ref = Some(snapshot_id.clone());
                        manifest.stages.push(record);
                        return Ok(snapshot);
                    }
                }
            }
        }
    }

    info!("syncing Zotero library...");
    let client = ZoteroClient::new(config.zotero.user_id.clone(), config.zotero.api_key.clone());

    let library_version = client
        .get_library_version()
        .await
        .context("failed to get Zotero library version")?;

    let since_version = if force {
        None
    } else {
        sync_state.as_ref().and_then(|s| s.library_version)
    };

    let items = client
        .fetch_items(since_version)
        .await
        .context("failed to fetch Zotero items")?;

    let collections = client
        .fetch_collections()
        .await
        .context("failed to fetch Zotero collections")?;

    let collection_paths = build_collection_paths(&collections);

    let mut papers: Vec<daily_paper_core::models::zotero::LibraryPaper> = items
        .iter()
        .filter_map(|item| {
            let mut paper = zotero_item_to_library_paper(item)?;
            // Fill in collection paths
            for ck in &paper.collection_keys {
                if let Some(path) = collection_paths.get(ck) {
                    paper
                        .collections
                        .push(daily_paper_core::models::zotero::CollectionPath {
                            key: ck.clone(),
                            path: path.clone(),
                        });
                }
            }
            Some(paper)
        })
        .filter(|p| !p.is_trashed)
        .collect();

    papers.sort_by(|a, b| a.library_id.cmp(&b.library_id));

    let snapshot_id = compute_snapshot_id(&config.zotero.user_id, library_version, &papers);

    let snapshot = ZoteroSnapshot {
        snapshot_id: snapshot_id.clone(),
        user_id: config.zotero.user_id.clone(),
        library_version,
        created_at: Utc::now(),
        item_count: papers.len(),
        items: papers,
    };

    // Save snapshot
    let snap_path = StatePath::new(format!("cache/zotero/snapshots/{}.json", snapshot_id))?;
    store.write_json(&snap_path, &snapshot)?;

    // Update sync state
    let new_sync_state = daily_paper_core::models::zotero::ZoteroSyncState {
        user_id: config.zotero.user_id.clone(),
        library_version,
        last_success_at: Some(Utc::now()),
        last_snapshot_id: Some(snapshot_id.clone()),
        last_error: None,
    };
    store.write_json(&sync_state_path, &new_sync_state)?;

    info!(
        snapshot_id = %snapshot_id,
        paper_count = snapshot.item_count,
        "Zotero sync complete"
    );

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record.output_ref = Some(snapshot_id);
    manifest.stages.push(record);

    Ok(snapshot)
}

async fn stage_source_fetch(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    date_window: &DateWindow,
) -> anyhow::Result<Vec<CandidatePaper>> {
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::SourceFetch,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    // Compute cache key from date window + source config
    let cache_key = {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(date_window.label.as_bytes());
        for source in &config.sources {
            hasher.update(source.kind.as_bytes());
            for cat in &source.categories {
                hasher.update(cat.as_bytes());
            }
            hasher.update([source.include_cross_list as u8]);
        }
        hex::encode(hasher.finalize())
    };
    let cache_path = StatePath::new(format!("cache/arxiv/{}.json", cache_key))?;

    // Check cache
    if let Some(candidates) = store.read_json::<Vec<CandidatePaper>>(&cache_path)? {
        info!(count = candidates.len(), "using cached arXiv results");
        record.status = StageStatus::Succeeded;
        record.cache_hit = true;
        record.finished_at = Some(Utc::now());
        manifest.stages.push(record);
        return Ok(candidates);
    }

    let mut all_candidates = Vec::new();

    for source in &config.sources {
        if source.kind == "arxiv" {
            info!(
                categories = ?source.categories,
                include_cross_list = source.include_cross_list,
                "fetching from arXiv"
            );
            let client = ArxivClient::new(source.categories.clone(), source.include_cross_list);
            let candidates = client
                .fetch(date_window.start, date_window.end)
                .await
                .context("failed to fetch from arXiv")?;
            all_candidates.extend(candidates);
        } else {
            warn!(kind = %source.kind, "unsupported source kind, skipping");
        }
    }

    // Deduplicate by paper_id
    all_candidates.sort_by(|a, b| a.paper_id.cmp(&b.paper_id));
    all_candidates.dedup_by(|a, b| a.paper_id == b.paper_id);

    info!(count = all_candidates.len(), "fetched candidate papers");

    // Cache results
    store.write_json(&cache_path, &all_candidates)?;

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok(all_candidates)
}

async fn stage_deduplicate(
    _store: &FileStateStore,
    manifest: &mut RunManifest,
    run_id: &str,
    candidates: &[CandidatePaper],
    snapshot: &ZoteroSnapshot,
) -> anyhow::Result<DedupResult> {
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::Deduplicate,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let mut kept: Vec<CandidatePaper> = Vec::new();
    let mut skipped_existing = Vec::new();
    let mut seen_dois: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut seen_arxiv: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut duplicates = Vec::new();

    // Build lookup sets from library
    let lib_dois: std::collections::HashSet<String> = snapshot
        .items
        .iter()
        .filter_map(|p| {
            p.doi
                .as_ref()
                .map(|d| daily_paper_core::models::candidate::normalize_doi(d))
        })
        .collect();
    let lib_arxiv: std::collections::HashSet<String> = snapshot
        .items
        .iter()
        .filter_map(|p| {
            p.arxiv_id
                .as_ref()
                .map(|a| daily_paper_core::models::candidate::normalize_arxiv_id(a))
        })
        .collect();

    for candidate in candidates {
        // Check against library
        if let Some(ref doi) = candidate.doi {
            let norm = daily_paper_core::models::candidate::normalize_doi(doi);
            if lib_dois.contains(&norm) {
                if let Some(lib_paper) = snapshot.items.iter().find(|p| {
                    p.doi
                        .as_ref()
                        .map(|d| daily_paper_core::models::candidate::normalize_doi(d))
                        == Some(norm.clone())
                }) {
                    skipped_existing.push(ExistingLibraryMatch {
                        candidate_paper_id: candidate.paper_id.clone(),
                        library_id: lib_paper.library_id.clone(),
                        reason: DuplicateReason::SameDoi,
                    });
                    continue;
                }
            }
        }

        if let Some(ref arxiv_id) = candidate.arxiv_id {
            let norm = daily_paper_core::models::candidate::normalize_arxiv_id(arxiv_id);
            if lib_arxiv.contains(&norm) {
                if let Some(lib_paper) = snapshot.items.iter().find(|p| {
                    p.arxiv_id
                        .as_ref()
                        .map(|a| daily_paper_core::models::candidate::normalize_arxiv_id(a))
                        == Some(norm.clone())
                }) {
                    skipped_existing.push(ExistingLibraryMatch {
                        candidate_paper_id: candidate.paper_id.clone(),
                        library_id: lib_paper.library_id.clone(),
                        reason: DuplicateReason::SameArxivId,
                    });
                    continue;
                }
            }
        }

        // Check within-candidate dedup
        let mut is_dup = false;
        if let Some(ref doi) = candidate.doi {
            let norm = daily_paper_core::models::candidate::normalize_doi(doi);
            if let Some(&existing_idx) = seen_dois.get(&norm) {
                duplicates.push(daily_paper_core::models::dedup::DuplicateRecord {
                    kept_paper_id: kept[existing_idx].paper_id.clone(),
                    dropped_paper_id: candidate.paper_id.clone(),
                    reason: DuplicateReason::SameDoi,
                });
                is_dup = true;
            } else {
                seen_dois.insert(norm, kept.len());
            }
        }
        if !is_dup {
            if let Some(ref arxiv_id) = candidate.arxiv_id {
                let norm = daily_paper_core::models::candidate::normalize_arxiv_id(arxiv_id);
                if let Some(&existing_idx) = seen_arxiv.get(&norm) {
                    duplicates.push(daily_paper_core::models::dedup::DuplicateRecord {
                        kept_paper_id: kept[existing_idx].paper_id.clone(),
                        dropped_paper_id: candidate.paper_id.clone(),
                        reason: DuplicateReason::SameArxivId,
                    });
                    is_dup = true;
                } else {
                    seen_arxiv.insert(norm, kept.len());
                }
            }
        }

        if !is_dup {
            kept.push(candidate.clone());
        }
    }

    let dedup = DedupResult {
        run_id: run_id.to_string(),
        candidates: kept,
        duplicates,
        skipped_existing,
    };

    info!(
        input = candidates.len(),
        kept = dedup.candidates.len(),
        library_matches = dedup.skipped_existing.len(),
        within_dupes = dedup.duplicates.len(),
        "deduplication complete"
    );

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok(dedup)
}

async fn stage_embedding(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    candidates: &[CandidatePaper],
    snapshot: &ZoteroSnapshot,
) -> anyhow::Result<(Vec<(String, Vec<f32>)>, Vec<(String, Vec<f32>, f32)>)> {
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

    let is_local = config.embedding.kind == "fastembed";
    let provider_id = if is_local { "fastembed" } else { "openai" };
    let config_hash = sha256_hex(config.embedding.model.as_bytes());

    // Collect all texts that need embedding (candidates + library)
    let mut all_texts: Vec<String> = Vec::new();
    let mut candidate_indices: Vec<usize> = Vec::new(); // index into candidates
    let mut candidate_vec_paths: Vec<StatePath> = Vec::new();

    // Check cache for candidates
    for (i, candidate) in candidates.iter().enumerate() {
        let input_text =
            EmbeddingClient::make_input_text(&candidate.title, &candidate.abstract_text);
        let input_hash = EmbeddingClient::compute_input_hash(
            provider_id,
            &config.embedding.model,
            &config_hash,
            &input_text,
        );
        let vec_path = StatePath::new(format!("cache/embeddings/{}.vec", input_hash))?;

        if let Some(bytes) = store.read_bytes(&vec_path)? {
            // Already cached, skip
            let _ = (i, bytes);
        } else {
            all_texts.push(input_text);
            candidate_indices.push(i);
            candidate_vec_paths.push(vec_path);
        }
    }

    // Embed all texts at once using local model
    let all_embeddings = if is_local && !all_texts.is_empty() {
        info!(count = all_texts.len(), "embedding with local model");
        let model_name = config.embedding.model.clone();
        let batch_size = config.embedding.batch_size;
        let cache_dir = store.root().join("cache").to_path_buf();
        let texts = all_texts.clone();
        Some(
            tokio::task::spawn_blocking(move || {
                let client = daily_paper_core::embedding::fastembed::LocalEmbeddingClient::new(
                    &model_name,
                    batch_size,
                    Some(&cache_dir),
                )?;
                client.embed_batch(&texts)
            })
            .await??,
        )
    } else if !is_local && !all_texts.is_empty() {
        info!(count = all_texts.len(), "embedding with remote API");
        let client = EmbeddingClient::new(
            config.embedding.base_url.clone().unwrap_or_default(),
            config.embedding.api_key.clone().unwrap_or_default(),
            config.embedding.model.clone(),
            config.embedding.batch_size,
            config.embedding.timeout_secs,
            config.embedding.max_retries,
            config.embedding.max_concurrency,
        );
        Some(client.embed_batch(&all_texts).await?)
    } else {
        None
    };

    // Build candidate embeddings
    let mut candidate_embs = Vec::new();

    // Add cached candidates
    for candidate in candidates {
        let input_text =
            EmbeddingClient::make_input_text(&candidate.title, &candidate.abstract_text);
        let input_hash = EmbeddingClient::compute_input_hash(
            provider_id,
            &config.embedding.model,
            &config_hash,
            &input_text,
        );
        let vec_path = StatePath::new(format!("cache/embeddings/{}.vec", input_hash))?;

        if let Some(bytes) = store.read_bytes(&vec_path)? {
            let vec = bytes_to_vec(&bytes);
            candidate_embs.push((candidate.paper_id.clone(), vec));
        }
    }

    // Add newly embedded candidates
    if let Some(ref embeddings) = all_embeddings {
        for (idx, &cand_idx) in candidate_indices.iter().enumerate() {
            let candidate = &candidates[cand_idx];
            let vec_path = &candidate_vec_paths[idx];
            store.write_bytes(vec_path, &vec_to_bytes(&embeddings[idx]))?;
            candidate_embs.push((candidate.paper_id.clone(), embeddings[idx].clone()));
        }
    }

    // Sort by original order
    candidate_embs.sort_by(|a, b| {
        let ia = candidates
            .iter()
            .position(|c| c.paper_id == a.0)
            .unwrap_or(0);
        let ib = candidates
            .iter()
            .position(|c| c.paper_id == b.0)
            .unwrap_or(0);
        ia.cmp(&ib)
    });

    // Build interest profile and embed library
    let profile = build_interest_profile(snapshot, &config.zotero.filters);
    let mut library_embs = Vec::new();
    let mut uncached_library: Vec<(String, f32, String, StatePath)> = Vec::new(); // (lib_id, weight, text, vec_path)

    for pref in &profile.papers {
        if let Some(lib_paper) = snapshot
            .items
            .iter()
            .find(|p| p.library_id == pref.library_id)
        {
            let input_text = match (&lib_paper.title, &lib_paper.abstract_text) {
                (t, Some(a)) if !a.is_empty() => EmbeddingClient::make_input_text(t, a),
                _ => continue,
            };
            let input_hash = EmbeddingClient::compute_input_hash(
                provider_id,
                &config.embedding.model,
                &config_hash,
                &input_text,
            );
            let vec_path = StatePath::new(format!("cache/embeddings/{}.vec", input_hash))?;

            if let Some(bytes) = store.read_bytes(&vec_path)? {
                library_embs.push((pref.library_id.clone(), bytes_to_vec(&bytes), pref.weight));
            } else {
                uncached_library.push((pref.library_id.clone(), pref.weight, input_text, vec_path));
            }
        }
    }

    // Batch embed uncached library papers
    if !uncached_library.is_empty() {
        info!(
            count = uncached_library.len(),
            "embedding uncached library papers"
        );
        let texts: Vec<String> = uncached_library
            .iter()
            .map(|(_, _, t, _)| t.clone())
            .collect();

        let embeddings = if is_local {
            let model_name = config.embedding.model.clone();
            let batch_size = config.embedding.batch_size;
            let cache_dir = store.root().join("cache").to_path_buf();
            tokio::task::spawn_blocking(move || {
                let client = daily_paper_core::embedding::fastembed::LocalEmbeddingClient::new(
                    &model_name,
                    batch_size,
                    Some(&cache_dir),
                )?;
                client.embed_batch(&texts)
            })
            .await??
        } else {
            let client = EmbeddingClient::new(
                config.embedding.base_url.clone().unwrap_or_default(),
                config.embedding.api_key.clone().unwrap_or_default(),
                config.embedding.model.clone(),
                config.embedding.batch_size,
                config.embedding.timeout_secs,
                config.embedding.max_retries,
                config.embedding.max_concurrency,
            );
            client.embed_batch(&texts).await?
        };

        for (i, emb) in embeddings.iter().enumerate() {
            let (ref lib_id, weight, _, ref vec_path) = uncached_library[i];
            store.write_bytes(vec_path, &vec_to_bytes(emb))?;
            library_embs.push((lib_id.clone(), emb.clone(), weight));
        }
    }

    info!(
        candidates = candidate_embs.len(),
        library = library_embs.len(),
        "embedding complete"
    );

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok((candidate_embs, library_embs))
}

async fn stage_rerank(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    _candidates: &[CandidatePaper],
    candidate_embs: &[(String, Vec<f32>)],
    library_embs: &[(String, Vec<f32>, f32)],
    snapshot: &ZoteroSnapshot,
) -> anyhow::Result<(
    daily_paper_core::rerank::selection::ReadSelection,
    Vec<(String, f32)>,
)> {
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::Rerank,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let ranked = rerank(
        candidate_embs,
        library_embs,
        config.reranker.top_k_library_matches,
    );

    let ranked_ids: Vec<String> = ranked.iter().map(|r| r.paper_id.clone()).collect();
    let candidate_set_hash = compute_candidate_set_hash(&ranked_ids);

    let profile = build_interest_profile(snapshot, &config.zotero.filters);

    let rerank_cache_key = compute_rerank_cache_key(
        &profile.profile_id,
        &candidate_set_hash,
        &config.embedding.model,
        &sha256_hex(format!("top_k={}", config.reranker.top_k_library_matches).as_bytes()),
    );

    let selection = select_top_n(&rerank_cache_key, &ranked_ids, config.reader.top_n);

    // Collect scores for selected papers
    let scores: Vec<(String, f32)> = selection
        .selected_paper_ids
        .iter()
        .filter_map(|pid| {
            let r = ranked.iter().find(|r| &r.paper_id == pid)?;
            Some((pid.clone(), r.score))
        })
        .collect();

    info!(
        ranked = ranked.len(),
        selected = selection.selected_paper_ids.len(),
        "rerank complete"
    );

    for (i, paper_id) in selection.selected_paper_ids.iter().enumerate() {
        if let Some(r) = ranked.iter().find(|r| &r.paper_id == paper_id) {
            debug!(rank = i + 1, paper_id = %paper_id, score = r.score, "selected paper");
        }
    }

    // Cache the selection
    let sel_path = StatePath::new(format!("cache/rerank/{}.json", selection.selection_id))?;
    store.write_json(&sel_path, &selection)?;

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record.output_ref = Some(selection.selection_id.clone());
    manifest.stages.push(record);

    Ok((selection, scores))
}

async fn stage_deep_read(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    selected_ids: &[String],
    candidates: &[CandidatePaper],
    force_read: bool,
) -> anyhow::Result<Vec<ReadResult>> {
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::DeepRead,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let reader = ReaderClient::new(
        config.reader.base_url.clone(),
        config.reader.api_key.clone(),
        config.reader.model.clone(),
        config.reader.timeout_secs,
        config.reader.max_retries,
        config.reader.max_concurrency,
        config.reader.max_input_tokens,
    );

    let template_hash = compute_template_hash(&build_system_prompt(None));
    let mut read_results = Vec::new();
    let mut any_failure = false;

    let paper_dir = store.root().join("cache/papers");
    std::fs::create_dir_all(&paper_dir)?;

    for paper_id in selected_ids {
        let candidate = match candidates.iter().find(|c| &c.paper_id == paper_id) {
            Some(c) => c,
            None => {
                warn!(paper_id = %paper_id, "selected paper not in candidates");
                any_failure = true;
                continue;
            }
        };

        info!(paper_id = %paper_id, title = %candidate.title, "deep reading");

        // Check read result cache
        let pdf_dir = paper_dir.join(paper_id);
        let read_cache_path = find_cached_read_result(
            store,
            paper_id,
            &template_hash,
            &config.reader.model,
            &config.reader.language,
        )?;

        if !force_read {
            if let Some(ref path) = read_cache_path {
                if let Some(result) = store.read_json::<ReadResult>(&StatePath::new(path)?)? {
                    info!(paper_id = %paper_id, "using cached read result");
                    read_results.push(result);
                    continue;
                }
            }
        }

        // Download PDF
        let pdf_url = match candidate.pdf_url.as_deref() {
            Some(url) => url,
            None => {
                warn!(paper_id = %paper_id, "no PDF URL, skipping");
                any_failure = true;
                continue;
            }
        };

        let pdf_asset = match download_pdf(
            paper_id,
            pdf_url,
            &pdf_dir,
            config.pdf.timeout_secs,
            config.pdf.max_pdf_mb,
        )
        .await
        {
            Ok(a) => a,
            Err(e) => {
                warn!(paper_id = %paper_id, error = %e, "PDF download failed");
                mark_stage_blocked(
                    manifest,
                    &format!("PDF download failed for {}: {}", paper_id, e),
                );
                any_failure = true;
                continue;
            }
        };

        // Extract text
        let extracted = match extract_text(
            paper_id,
            Path::new(&pdf_asset.file_path),
            &pdf_asset.sha256,
            &pdf_dir,
            config.pdf.timeout_secs,
            config.pdf.max_text_chars,
        )
        .await
        {
            Ok(e) => e,
            Err(e) => {
                warn!(paper_id = %paper_id, error = %e, "PDF extract failed");
                mark_stage_blocked(
                    manifest,
                    &format!("PDF extract failed for {}: {}", paper_id, e),
                );
                any_failure = true;
                continue;
            }
        };

        // Fetch metadata
        let metadata = extract_from_source(paper_id, &candidate.source_metadata);

        // Build prompt
        let full_text =
            std::fs::read_to_string(Path::new(&extracted.text_path)).unwrap_or_default();
        let sections = parse_sections(&full_text);
        let selected_text = select_sections_for_reading(
            &full_text,
            &sections,
            config.reader.max_input_tokens * 3, // rough char estimate
        );

        let authors_str = candidate
            .authors
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let user_prompt = build_user_prompt(
            &candidate.title,
            &candidate.abstract_text,
            &authors_str,
            &selected_text,
            &config.reader.language,
        );
        let system_prompt = build_system_prompt(None);

        let trimmed_prompt = trim_to_token_budget(&user_prompt, config.reader.max_input_tokens * 4);

        // Call LLM
        let (raw_response, token_usage) =
            match reader.complete(&system_prompt, &trimmed_prompt).await {
                Ok(r) => r,
                Err(e) => {
                    warn!(paper_id = %paper_id, error = %e, "LLM read failed");
                    mark_stage_blocked(
                        manifest,
                        &format!("LLM read failed for {}: {}", paper_id, e),
                    );
                    any_failure = true;
                    continue;
                }
            };

        // Parse LLM output (JSON with structured summary and affiliations)
        let parsed = daily_paper_core::reader::template::parse_llm_output(&raw_response);
        let summary = parsed
            .as_ref()
            .map(|p| p.summary.clone())
            .unwrap_or_else(|| raw_response.clone());
        let author_affiliations = parsed
            .as_ref()
            .map(|p| {
                p.author_affiliations
                    .iter()
                    .map(|aa| daily_paper_core::models::read::AuthorAffiliation {
                        name: aa.name.clone(),
                        affiliation: aa.affiliation.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Use LLM-parsed URLs when available (more accurate than source metadata)
        let llm_project_url = parsed.as_ref().and_then(|p| p.project_url.clone());
        let llm_code_url = parsed.as_ref().and_then(|p| p.code_url.clone());

        let read_cache_key = compute_read_cache_key(
            paper_id,
            &pdf_asset.sha256,
            &extracted.text_extract_key,
            &metadata.metadata_key,
            &template_hash,
            &config.reader.model,
            &config.reader.language,
        );

        let read_result = ReadResult {
            paper_id: paper_id.clone(),
            cache_key: read_cache_key.clone(),
            generated_at: Utc::now(),
            model_id: config.reader.model.clone(),
            reader_template_hash: template_hash.clone(),
            language: config.reader.language.clone(),
            summary,
            metadata: PaperMetadataSummary {
                institutions: metadata.institutions.clone(),
                notable_authors: metadata.notable_authors.clone(),
                project_url: llm_project_url.or_else(|| metadata.project_url.clone()),
                code_url: llm_code_url.or_else(|| metadata.code_url.clone()),
            },
            author_affiliations,
            token_usage,
            warnings: Vec::new(),
        };

        // Cache read result
        let read_result_path = StatePath::new(format!(
            "cache/papers/{}/read/{}.json",
            paper_id, read_cache_key
        ))?;
        store.write_json(&read_result_path, &read_result)?;

        read_results.push(read_result);
    }

    if any_failure && config.reader.on_read_failure == "block" {
        // Don't return error here — the caller checks read_results
        warn!("some papers failed deep read; pipeline will be blocked");
    }

    info!(
        total = selected_ids.len(),
        succeeded = read_results.len(),
        "deep read complete"
    );

    record.status = if read_results.len() == selected_ids.len() {
        StageStatus::Succeeded
    } else {
        StageStatus::Failed
    };
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok(read_results)
}

async fn stage_render(
    store: &FileStateStore,
    manifest: &mut RunManifest,
    run_id: &str,
    selected_ids: &[String],
    candidates: &[CandidatePaper],
    read_results: &[ReadResult],
    scores: &[(String, f32)],
) -> anyhow::Result<(String, Option<String>)> {
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::Render,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let report_papers: Vec<ReportPaper> = selected_ids
        .iter()
        .enumerate()
        .filter_map(|(i, paper_id)| {
            let candidate = candidates.iter().find(|c| &c.paper_id == paper_id)?;
            let read_result = read_results
                .iter()
                .find(|r| &r.paper_id == paper_id)
                .cloned();

            Some(ReportPaper {
                paper_id: paper_id.clone(),
                rank: i + 1,
                title: candidate.title.clone(),
                authors: candidate.authors.clone(),
                abstract_text: candidate.abstract_text.clone(),
                landing_url: candidate.landing_url.clone(),
                pdf_url: candidate.pdf_url.clone(),
                read_result,
                score: scores
                    .iter()
                    .find(|(id, _)| id == paper_id)
                    .map(|(_, s)| *s)
                    .unwrap_or(0.0),
            })
        })
        .collect();

    let title = format!("Daily Paper - {}", manifest.date_window.label);
    let html_body = render_html(&title, &report_papers, run_id);
    let text_body = render_text(&title, &report_papers, run_id);

    // Write report files
    let html_path = format!("reports/{}/report.html", run_id);
    let text_path = format!("reports/{}/report.txt", run_id);

    store.write_string(&StatePath::new(&html_path)?, &html_body)?;
    store.write_string(&StatePath::new(&text_path)?, &text_body)?;

    let report_hash = sha256_hex(html_body.as_bytes());
    let report_instance_id = format!("{}-{}", run_id, &report_hash[..8]);

    let rendered = RenderedReport {
        report_hash: report_hash.clone(),
        report_instance_id: report_instance_id.clone(),
        run_id: run_id.to_string(),
        generated_at: Utc::now(),
        title,
        html_path: store.root().join(&html_path).to_string_lossy().to_string(),
        text_path: Some(store.root().join(&text_path).to_string_lossy().to_string()),
        ranked_paper_ids: selected_ids.to_vec(),
        read_paper_ids: read_results.iter().map(|r| r.paper_id.clone()).collect(),
    };

    let report_meta_path = StatePath::new(format!("reports/{}/report.json", run_id))?;
    store.write_json(&report_meta_path, &rendered)?;

    info!(
        papers = report_papers.len(),
        html = %html_path,
        "render complete"
    );

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok((
        store.root().join(&html_path).to_string_lossy().to_string(),
        Some(store.root().join(&text_path).to_string_lossy().to_string()),
    ))
}

async fn stage_send(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    run_id: &str,
    html_path: &str,
    text_path: Option<&str>,
    force: bool,
) -> anyhow::Result<()> {
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::Send,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    // Read the report metadata to get report_hash
    let report_meta_path = StatePath::new(format!("reports/{}/report.json", run_id))?;
    let rendered: RenderedReport = store
        .read_json(&report_meta_path)?
        .context("report metadata not found")?;

    let sink_config_hash = {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(config.email.smtp_server.as_bytes());
        hasher.update(config.email.smtp_port.to_le_bytes());
        hasher.update(config.email.sender.as_bytes());
        hasher.update(config.email.receiver.as_bytes());
        hex::encode(hasher.finalize())
    };

    let delivery_key = compute_delivery_key(
        &rendered.report_hash,
        "smtp",
        &sink_config_hash,
        &config.email.receiver,
    );

    // Check if already sent
    if !force {
        let receipt_path = StatePath::new(format!("deliveries/{}.json", delivery_key))?;
        if store.exists(&receipt_path)? {
            info!("report already sent, skipping (use --force-send to override)");
            record.status = StageStatus::Succeeded;
            record.cache_hit = true;
            record.finished_at = Some(Utc::now());
            manifest.stages.push(record);
            return Ok(());
        }
    }

    let subject = format!("Daily Paper - {}", rendered.title);
    let now = Utc::now();

    let receipt = daily_paper_core::deliver::receipt::deliver_email(
        &rendered.report_hash,
        &rendered.report_instance_id,
        run_id,
        Path::new(html_path),
        text_path.map(Path::new),
        &config.email.smtp_server,
        config.email.smtp_port,
        &config.email.sender,
        &config.email.receiver,
        &config.email.password,
        &subject,
        now,
    )
    .context("failed to send email")?;

    // Save delivery receipt
    let receipt_path = StatePath::new(format!("deliveries/{}.json", delivery_key))?;
    store.write_json(&receipt_path, &receipt)?;

    info!(delivery_key = %delivery_key, "email sent successfully");

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────

fn generate_run_id() -> String {
    let now = Local::now();
    let ts = now.format("%Y%m%d-%H%M%S").to_string();
    let suffix: String = (0..6)
        .map(|_| format!("{:x}", fastrand::u8(0..16)))
        .collect();
    format!("{}-{}", ts, suffix)
}

fn compute_date_window(date_arg: Option<&str>) -> anyhow::Result<DateWindow> {
    let local_today = match date_arg {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .context(format!("invalid date '{}', expected YYYY-MM-DD", s))?,
        None => {
            let now = Local::now();
            (now - Duration::days(1)).date_naive()
        }
    };

    let start = Local
        .from_local_datetime(&local_today.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .context("ambiguous local time")?
        .with_timezone(&Utc);

    let end = Local
        .from_local_datetime(
            &(local_today + Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
        .single()
        .context("ambiguous local time")?
        .with_timezone(&Utc);

    Ok(DateWindow {
        start,
        end,
        label: local_today.format("%Y-%m-%d").to_string(),
    })
}

fn build_cli_overrides(args: &RunArgs) -> Vec<CliOverride> {
    let mut overrides = Vec::new();
    if let Some(ref date) = args.date {
        overrides.push(CliOverride {
            key: "date".into(),
            value: date.clone(),
        });
    }
    if args.dry_run {
        overrides.push(CliOverride {
            key: "dry_run".into(),
            value: "true".into(),
        });
    }
    if args.force_zotero_sync {
        overrides.push(CliOverride {
            key: "force_zotero_sync".into(),
            value: "true".into(),
        });
    }
    if args.force_rerank {
        overrides.push(CliOverride {
            key: "force_rerank".into(),
            value: "true".into(),
        });
    }
    if args.force_read {
        overrides.push(CliOverride {
            key: "force_read".into(),
            value: "true".into(),
        });
    }
    if args.force_send {
        overrides.push(CliOverride {
            key: "force_send".into(),
            value: "true".into(),
        });
    }
    if let Some(max) = args.max_candidates {
        overrides.push(CliOverride {
            key: "max_candidates".into(),
            value: max.to_string(),
        });
    }
    overrides
}

fn build_interest_profile(
    snapshot: &ZoteroSnapshot,
    filters: &[daily_paper_core::config::ResolvedZoteroFilter],
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

fn path_matches_filter(collection_path: &str, filter_pattern: &str) -> bool {
    let prefix = filter_pattern
        .trim_end_matches("/**")
        .trim_end_matches("/*");
    if prefix == filter_pattern {
        // Exact match
        collection_path == filter_pattern
    } else {
        collection_path.starts_with(prefix)
    }
}

fn find_cached_read_result(
    store: &FileStateStore,
    paper_id: &str,
    template_hash: &str,
    model_id: &str,
    language: &str,
) -> anyhow::Result<Option<String>> {
    let paper_dir = format!("cache/papers/{}/read", paper_id);
    let full_dir = store.root().join(&paper_dir);
    if !full_dir.exists() {
        return Ok(None);
    }

    for entry in std::fs::read_dir(&full_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let state_path = StatePath::new(format!(
                "cache/papers/{}/read/{}",
                paper_id,
                path.file_name().unwrap().to_string_lossy()
            ))?;
            if let Some(result) = store.read_json::<ReadResult>(&state_path)? {
                if result.reader_template_hash == template_hash
                    && result.model_id == model_id
                    && result.language == language
                {
                    return Ok(Some(state_path.as_path().to_string_lossy().to_string()));
                }
            }
        }
    }
    Ok(None)
}

fn vec_to_bytes(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn bytes_to_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn skip_stage(manifest: &mut RunManifest, stage: StageName) {
    manifest.stages.push(StageRecord {
        stage,
        status: StageStatus::Skipped,
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    });
}

fn mark_stage_blocked(manifest: &mut RunManifest, message: &str) {
    warn!("{}", message);
    add_warning(manifest, "pipeline", message);
}

fn add_warning(manifest: &mut RunManifest, kind: &str, message: &str) {
    manifest.warnings.push(WarningRecord {
        kind: kind.to_string(),
        message: message.to_string(),
        context: serde_json::Value::Null,
    });
}

fn classify_error(e: &anyhow::Error) -> ErrorKind {
    let msg = e.to_string().to_lowercase();
    if msg.contains("pdf") && msg.contains("download") {
        ErrorKind::PdfDownload
    } else if msg.contains("pdf") && msg.contains("extract") {
        ErrorKind::PdfExtract
    } else if msg.contains("embedding") {
        ErrorKind::Embedding
    } else if msg.contains("llm") || msg.contains("openai") {
        ErrorKind::Llm
    } else if msg.contains("smtp") || msg.contains("delivery") || msg.contains("email") {
        ErrorKind::Delivery
    } else if msg.contains("render") {
        ErrorKind::Render
    } else if msg.contains("rate") && msg.contains("limit") {
        ErrorKind::RateLimited
    } else if msg.contains("auth") {
        ErrorKind::Auth
    } else if msg.contains("config") {
        ErrorKind::Config
    } else if msg.contains("network") || msg.contains("timeout") || msg.contains("connection") {
        ErrorKind::RetryableNetwork
    } else if msg.contains("source") {
        ErrorKind::SourceUnavailable
    } else {
        ErrorKind::Storage
    }
}

// ── Pipeline event helpers ───────────────────────────────────────────

fn emit_event(
    event_tx: Option<&tokio::sync::broadcast::Sender<PipelineEvent>>,
    run_id: &str,
    stage: StageName,
    _is_start: bool,
) {
    if let Some(tx) = event_tx {
        let _ = tx.send(PipelineEvent::StageStart {
            run_id: run_id.to_string(),
            stage,
        });
    }
}

fn emit_stage_end<T>(
    event_tx: Option<&tokio::sync::broadcast::Sender<PipelineEvent>>,
    manifest: &RunManifest,
    run_id: &str,
    stage: StageName,
    result: &anyhow::Result<T>,
) {
    if let Some(tx) = event_tx {
        let record = manifest.stages.iter().find(|s| s.stage == stage);
        let (status, cache_hit, duration_ms) = if let Some(r) = record {
            let ms = r
                .finished_at
                .map(|f| (f - r.started_at).num_milliseconds().max(0) as u64)
                .unwrap_or(0);
            (r.status.clone(), r.cache_hit, ms)
        } else {
            let status = if result.is_ok() {
                StageStatus::Succeeded
            } else {
                StageStatus::Failed
            };
            (status, false, 0u64)
        };
        let _ = tx.send(PipelineEvent::StageEnd {
            run_id: run_id.to_string(),
            stage,
            status,
            cache_hit,
            duration_ms,
        });
    }
}

// ── Cached data loaders for --stage ─────────────────────────────────

fn find_source_run(store: &FileStateStore, from_run: Option<&str>) -> anyhow::Result<String> {
    if let Some(id) = from_run {
        return Ok(id.to_string());
    }
    // Find latest run with a manifest
    let runs_dir = store.root().join("runs");
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
    entries
        .first()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .ok_or_else(|| anyhow::anyhow!("no runs found"))
}

fn load_cached_snapshot(
    store: &FileStateStore,
    source: Option<&RunManifest>,
) -> anyhow::Result<ZoteroSnapshot> {
    let source = source.ok_or_else(|| anyhow::anyhow!("no source run for cached snapshot"))?;
    let snap_ref = source
        .stages
        .iter()
        .find(|s| s.stage == StageName::ZoteroSync)
        .and_then(|s| s.output_ref.as_deref())
        .ok_or_else(|| anyhow::anyhow!("source run has no ZoteroSync output"))?;
    let path = StatePath::new(format!("cache/zotero/snapshots/{}.json", snap_ref))?;
    store
        .read_json(&path)?
        .ok_or_else(|| anyhow::anyhow!("cached snapshot not found: {}", snap_ref))
}

fn load_cached_candidates(
    store: &FileStateStore,
    source: Option<&RunManifest>,
) -> anyhow::Result<Vec<CandidatePaper>> {
    let source = source.ok_or_else(|| anyhow::anyhow!("no source run for cached candidates"))?;
    let hash = match source
        .stages
        .iter()
        .find(|s| s.stage == StageName::SourceFetch)
        .and_then(|s| s.output_ref.as_deref())
    {
        Some(h) => h,
        None => return Ok(Vec::new()), // source run had no candidates
    };
    let path = StatePath::new(format!("cache/arxiv/{}.json", hash))?;
    Ok(store.read_json(&path)?.unwrap_or_default())
}

fn load_cached_dedup(
    store: &FileStateStore,
    source: Option<&RunManifest>,
    candidates: &[CandidatePaper],
    snapshot: &ZoteroSnapshot,
) -> anyhow::Result<DedupResult> {
    let _ = (store, snapshot);
    let source = source.ok_or_else(|| anyhow::anyhow!("no source run for cached dedup"))?;
    warn!("loading cached dedup: re-running deduplicate with current data");
    let kept = candidates.to_vec();
    Ok(DedupResult {
        run_id: source.run_id.clone(),
        candidates: kept,
        duplicates: Vec::new(),
        skipped_existing: Vec::new(),
    })
}

#[allow(clippy::type_complexity)]
fn load_cached_embeddings(
    store: &FileStateStore,
    source: Option<&RunManifest>,
    candidates: &[CandidatePaper],
    snapshot: &ZoteroSnapshot,
) -> anyhow::Result<(Vec<(String, Vec<f32>)>, Vec<(String, Vec<f32>, f32)>)> {
    let _ = (store, source, candidates, snapshot);
    anyhow::bail!("loading cached embeddings not yet supported; re-run with --stage embedding")
}

fn load_cached_rerank(
    store: &FileStateStore,
    source: Option<&RunManifest>,
) -> anyhow::Result<(
    daily_paper_core::rerank::selection::ReadSelection,
    Vec<(String, f32)>,
)> {
    let source = source.ok_or_else(|| anyhow::anyhow!("no source run for cached rerank"))?;
    let sel_ref = source
        .stages
        .iter()
        .find(|s| s.stage == StageName::Rerank)
        .and_then(|s| s.output_ref.as_deref())
        .ok_or_else(|| anyhow::anyhow!("source run has no Rerank output"))?;
    let path = StatePath::new(format!("cache/rerank/{}.json", sel_ref))?;
    let sel: Option<daily_paper_core::rerank::selection::ReadSelection> = store.read_json(&path)?;
    let selection = sel.ok_or_else(|| anyhow::anyhow!("cached rerank not found: {}", sel_ref))?;
    Ok((selection, Vec::new()))
}

fn load_cached_read_results(
    store: &FileStateStore,
    source: Option<&RunManifest>,
) -> anyhow::Result<Vec<ReadResult>> {
    let source = source.ok_or_else(|| anyhow::anyhow!("no source run for cached read results"))?;
    let read_ref = source
        .stages
        .iter()
        .find(|s| s.stage == StageName::DeepRead)
        .and_then(|s| s.output_ref.as_deref())
        .ok_or_else(|| anyhow::anyhow!("source run has no DeepRead output"))?;
    // output_ref for DeepRead is a comma-separated list of paper IDs
    let paper_ids: Vec<&str> = read_ref.split(',').collect();
    let mut results = Vec::new();
    for pid in paper_ids {
        let path = StatePath::new(format!("cache/papers/{}/read/{}.json", pid, pid))?;
        if let Some(r) = store.read_json::<ReadResult>(&path)? {
            results.push(r);
        }
    }
    Ok(results)
}

fn load_cached_render(
    store: &FileStateStore,
    source: Option<&RunManifest>,
    run_id: &str,
) -> anyhow::Result<(String, Option<String>)> {
    let _ = source;
    // Check if this run already has a report
    let html_path = store
        .root()
        .join("reports")
        .join(run_id)
        .join("report.html");
    if html_path.exists() {
        let text_path = store.root().join("reports").join(run_id).join("report.txt");
        Ok((
            html_path.to_string_lossy().to_string(),
            if text_path.exists() {
                Some(text_path.to_string_lossy().to_string())
            } else {
                None
            },
        ))
    } else {
        anyhow::bail!("no cached report found for run {}", run_id)
    }
}
