use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use chrono::{Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::cli::RunArgs;
use crate::progress::StageProgress;
use daily_paper_core::config::load_config;
use daily_paper_core::config::ResolvedConfig;
use daily_paper_core::embedding::openai::EmbeddingClient;
use daily_paper_core::embedding::{compute_input_hash, make_input_text};
use daily_paper_core::metadata::fetcher::extract_from_source;
use daily_paper_core::models::candidate::CandidatePaper;
use daily_paper_core::models::common::{sha256_hex, DateWindow};
use daily_paper_core::models::dedup::{DedupResult, DuplicateReason, ExistingLibraryMatch};
use daily_paper_core::models::interest::{InterestPaperRef, InterestProfile};
use daily_paper_core::models::read::{
    compute_read_cache_key, PaperMetadataSummary, ReadResult, TokenUsage,
};
use daily_paper_core::models::report::{compute_delivery_key, RenderedReport};
use daily_paper_core::models::run::*;
use daily_paper_core::models::zotero::ZoteroSnapshot;
use daily_paper_core::pdf::download::download_pdf;
use daily_paper_core::pdf::extract::extract_text;
use daily_paper_core::pdf::section::{parse_sections, select_sections_for_reading};
use daily_paper_core::reader::openai::ReaderClient;
use daily_paper_core::reader::template::{
    build_system_prompt, build_tldr_system_prompt, build_tldr_user_prompt, build_user_prompt,
    compute_template_hash, is_valid_structured_summary, parse_llm_output, parse_tldr_output,
    trim_to_token_budget,
};
use daily_paper_core::render::html::{render_html, ReportPaper};
use daily_paper_core::render::text::render_text;
use daily_paper_core::rerank::cosine::rerank;
use daily_paper_core::rerank::selection::{
    compute_candidate_set_hash, compute_rerank_cache_key, select_top_n,
};
use daily_paper_core::source::arxiv::client::{ArxivBackendKind, ArxivClient};
use daily_paper_core::state::path::StatePath;
use daily_paper_core::state::store::FileStateStore;
use daily_paper_core::zotero::client::ZoteroClient;
use daily_paper_core::zotero::convert::zotero_item_to_library_paper;
use daily_paper_core::zotero::profile::{
    build_collection_paths, compute_rules_hash, compute_snapshot_id,
};

/// Cached embeddings for a pipeline run.
/// Mapping of paper IDs to their embedding hashes (for date-based cache).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EmbeddingIndex {
    /// (paper_id, input_hash) for candidate papers
    candidate_hashes: Vec<(String, String)>,
    /// (library_id, weight, input_hash) for library papers
    library_hashes: Vec<(String, f32, String)>,
}

#[derive(Clone)]
struct PendingEmbedding {
    text: String,
    vec_path: StatePath,
}

pub async fn execute(data_dir: &Path, args: RunArgs) -> anyhow::Result<()> {
    let config_path = data_dir.join("config.toml");
    let (_raw, resolved) = load_config(&config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;

    info!(config = %config_path.display(), "config loaded");
    info!(data_dir = %data_dir.display(), "data directory");

    let store = FileStateStore::new(data_dir.to_path_buf());
    store.ensure_dirs()?;

    // Parse --stage filter
    let stage_filter: Option<Vec<StageName>> = if args.stages.is_empty() {
        None
    } else {
        let mut stages = Vec::new();
        for s in &args.stages {
            let stage = StageName::from_kebab(s).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown stage '{}'. Valid: zotero-sync, source-fetch, embedding, rerank, deep-read, render, send",
                    s
                )
            })?;
            stages.push(stage);
        }
        Some(stages)
    };

    // If --stage is specified, load source run manifest for cached data
    let source_manifest: Option<RunManifest> = if stage_filter.is_some() {
        let source_run_id =
            find_source_run(&store, args.from_run.as_deref(), stage_filter.as_deref())?;
        info!(source_run = %source_run_id, "loading source run for cached data");
        let manifest_path = StatePath::new(format!("cache/runs/{}/manifest.json", source_run_id))?;
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

    let manifest_path = StatePath::new(format!("cache/runs/{}/manifest.json", run_id))?;
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
    let date = &date_window.label;
    let cache_date = source_manifest
        .map(|m| m.date_window.label.as_str())
        .unwrap_or(date);
    store.ensure_date_dir(date)?;

    let should_run =
        |stage: &StageName| -> bool { stage_filter.map(|f| f.contains(stage)).unwrap_or(true) };

    // Track which stages have been completed in this run
    let mut completed_stages: Vec<StageName> = Vec::new();

    // Helper to check if all requested stages are done
    let all_requested_done = |completed: &[StageName], filter: Option<&[StageName]>| -> bool {
        if let Some(filter) = filter {
            filter.iter().all(|s| completed.contains(s))
        } else {
            false // No filter means run all stages
        }
    };

    // Stage 1: Zotero sync
    let snapshot = if should_run(&StageName::ZoteroSync) {
        emit_event(event_tx, run_id, StageName::ZoteroSync, true);
        let r = stage_zotero_sync(store, config, manifest, args.force_zotero_sync, date).await;
        emit_stage_end(event_tx, manifest, run_id, StageName::ZoteroSync, &r);
        let _ = store.write_json(manifest_path, manifest);
        completed_stages.push(StageName::ZoteroSync);
        let snapshot = r?;
        if all_requested_done(&completed_stages, stage_filter) {
            info!("all requested stages completed");
            return Ok(());
        }
        snapshot
    } else {
        load_cached_snapshot(store, cache_date, source_manifest)?
    };

    // Stage 2: Source fetch (includes dedup)
    let mut dedup = if should_run(&StageName::SourceFetch) {
        emit_event(event_tx, run_id, StageName::SourceFetch, true);
        let r = stage_source_fetch(
            store,
            config,
            manifest,
            date_window,
            args.date.is_some(),
            &snapshot,
            run_id,
        )
        .await;
        emit_stage_end(event_tx, manifest, run_id, StageName::SourceFetch, &r);
        let _ = store.write_json(manifest_path, manifest);
        completed_stages.push(StageName::SourceFetch);
        let dedup = r?;
        if all_requested_done(&completed_stages, stage_filter) {
            info!("all requested stages completed");
            return Ok(());
        }
        dedup
    } else {
        load_cached_candidates(store, cache_date)?
    };

    if dedup.candidates.is_empty() {
        warn!("no candidate papers found after dedup");
        add_warning(manifest, "source", "no candidate papers found");
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
        let r = stage_embedding(
            store,
            config,
            manifest,
            &dedup.candidates,
            &snapshot,
            date,
            event_tx,
            run_id,
        )
        .await;
        emit_stage_end(event_tx, manifest, run_id, StageName::Embedding, &r);
        let _ = store.write_json(manifest_path, manifest);
        completed_stages.push(StageName::Embedding);
        r?
    } else {
        load_cached_embeddings(store, cache_date, &dedup.candidates, &snapshot)?
    };

    // Check if all requested stages are done
    if all_requested_done(&completed_stages, stage_filter) {
        info!("all requested stages completed");
        return Ok(());
    }

    // Stage 5: Rerank + selection
    let (selection, rerank_scores) = if should_run(&StageName::Rerank) {
        emit_event(event_tx, run_id, StageName::Rerank, true);
        let r = stage_rerank(
            store,
            config,
            manifest,
            &candidate_embs,
            &library_embs,
            &snapshot,
            date,
        )
        .await;
        emit_stage_end(event_tx, manifest, run_id, StageName::Rerank, &r);
        let _ = store.write_json(manifest_path, manifest);
        completed_stages.push(StageName::Rerank);
        r?
    } else {
        load_cached_rerank(store, cache_date, source_manifest)?
    };

    // Check if all requested stages are done
    if all_requested_done(&completed_stages, stage_filter) {
        info!("all requested stages completed");
        return Ok(());
    }

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
            date,
            event_tx,
            run_id,
        )
        .await;
        emit_stage_end(event_tx, manifest, run_id, StageName::DeepRead, &r);
        let _ = store.write_json(manifest_path, manifest);
        completed_stages.push(StageName::DeepRead);
        r?
    } else {
        load_cached_read_results(store, cache_date, source_manifest)?
    };

    // Check if all requested stages are done
    if all_requested_done(&completed_stages, stage_filter) {
        info!("all requested stages completed");
        return Ok(());
    }

    // Stage 10: Render
    let (html_path, text_path) = if should_run(&StageName::Render) {
        emit_event(event_tx, run_id, StageName::Render, true);
        let r = stage_render(
            store,
            manifest,
            run_id,
            RenderInput {
                selected_ids: &selection.selected_paper_ids,
                candidates: &dedup.candidates,
                read_results: &read_results,
                scores: &rerank_scores,
                date,
            },
        )
        .await;
        emit_stage_end(event_tx, manifest, run_id, StageName::Render, &r);
        let _ = store.write_json(manifest_path, manifest);
        completed_stages.push(StageName::Render);
        r?
    } else {
        load_cached_render(store, cache_date, source_manifest)?
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
            let report_run_id = if should_run(&StageName::Render) {
                run_id
            } else {
                source_manifest.map(|m| m.run_id.as_str()).unwrap_or(run_id)
            };
            emit_event(event_tx, run_id, StageName::Send, true);
            let r = stage_send(
                store,
                config,
                manifest,
                report_run_id,
                &html_path,
                text_path.as_deref(),
                args.force_send,
            )
            .await;
            emit_stage_end(event_tx, manifest, run_id, StageName::Send, &r);
            let _ = store.write_json(manifest_path, manifest);
            completed_stages.push(StageName::Send);
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
    date: &str,
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

    // When incremental sync returns no new items and the library version
    // hasn't changed, reuse the cached snapshot instead of creating an
    // empty one that would overwrite the existing data.
    if since_version.is_some() && items.is_empty() {
        let versions_match = sync_state.as_ref().and_then(|s| s.library_version) == library_version;

        if versions_match {
            if let Some(ref state) = sync_state {
                if let Some(ref snapshot_id) = state.last_snapshot_id {
                    let snap_path =
                        StatePath::new(format!("cache/zotero/snapshots/{}.json", snapshot_id))?;
                    if let Some(snapshot) = store.read_json::<ZoteroSnapshot>(&snap_path)? {
                        info!(
                            snapshot_id = %snapshot_id,
                            paper_count = snapshot.item_count,
                            library_version = ?library_version,
                            "Zotero library unchanged, reusing cached snapshot"
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
            // Cached snapshot missing — fall through to a full sync
            info!("cached snapshot missing, falling back to full sync");
            let items = client
                .fetch_items(None)
                .await
                .context("failed to fetch Zotero items (full sync)")?;
            // Continue with the full items list below
            // (items variable is shadowed, rest of the pipeline proceeds)
            return stage_zotero_sync_inner(
                store,
                config,
                manifest,
                record,
                sync_state_path,
                client,
                library_version,
                items,
                date,
            )
            .await;
        }
        // Library version changed but no items returned — do a full sync
        info!(
            old_version = ?sync_state.as_ref().and_then(|s| s.library_version),
            new_version = ?library_version,
            "library version changed but no items returned, falling back to full sync"
        );
        let items = client
            .fetch_items(None)
            .await
            .context("failed to fetch Zotero items (full sync)")?;
        return stage_zotero_sync_inner(
            store,
            config,
            manifest,
            record,
            sync_state_path,
            client,
            library_version,
            items,
            date,
        )
        .await;
    }

    stage_zotero_sync_inner(
        store,
        config,
        manifest,
        record,
        sync_state_path,
        client,
        library_version,
        items,
        date,
    )
    .await
}

/// Core snapshot creation logic extracted so both normal and fallback paths
/// can share it without duplicating 60 lines.
async fn stage_zotero_sync_inner(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    mut record: StageRecord,
    sync_state_path: StatePath,
    client: ZoteroClient,
    library_version: Option<u64>,
    items: Vec<serde_json::Value>,
    date: &str,
) -> anyhow::Result<ZoteroSnapshot> {
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

    // Write snapshot ID to date directory
    let snapshot_id_path = StatePath::new(format!("archive/{}/snapshot_id.txt", date))?;
    store.write_string(&snapshot_id_path, &snapshot_id)?;

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
    date_explicit: bool,
    snapshot: &ZoteroSnapshot,
    run_id: &str,
) -> anyhow::Result<DedupResult> {
    let date = &date_window.label;
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

    let mut all_candidates = Vec::new();

    // RSS for today's papers (pubDate = announcement date),
    // Export for historical queries (submittedDate query).
    let today = default_arxiv_announcement_date().to_string();
    let use_rss = !date_explicit || date_window.label == today;

    for source in &config.sources {
        if source.kind == "arxiv" {
            let backend = if use_rss {
                ArxivBackendKind::Rss
            } else {
                ArxivBackendKind::Export
            };
            info!(
                categories = ?source.categories,
                include_cross_list = source.include_cross_list,
                backend = ?backend,
                "fetching from arXiv"
            );
            let client = ArxivClient::with_backend(
                backend,
                source.categories.clone(),
                source.include_cross_list,
                source.max_results_per_page,
                source.max_pages,
            );
            let candidates = client
                .fetch(date_window.start, date_window.end)
                .await
                .context("failed to fetch from arXiv")?;
            all_candidates.extend(candidates);
        } else {
            warn!(kind = %source.kind, "unsupported source kind, skipping");
        }
    }

    // Filter by published_at to keep only recent papers.
    // For Export backend, `published_at` is submission date (not announcement date),
    // so we use a wider window to catch papers submitted a few days before announcement.
    // For RSS backend, this is already filtered by pubDate in the parser.
    let before_filter = all_candidates.len();
    let cutoff = date_window.start - chrono::Duration::days(2);
    all_candidates.retain(|c| c.published_at.map(|p| p >= cutoff).unwrap_or(false));
    if all_candidates.len() < before_filter {
        info!(
            before = before_filter,
            after = all_candidates.len(),
            "filtered candidates by published_at (last 2 days)"
        );
    }

    // Deduplicate by paper_id across sources
    all_candidates.sort_by(|a, b| a.paper_id.cmp(&b.paper_id));
    all_candidates.dedup_by(|a, b| a.paper_id == b.paper_id);

    info!(count = all_candidates.len(), "fetched candidate papers");

    // Write raw candidates to date directory (before library dedup)
    let candidates_date_path = StatePath::new(format!("archive/{}/candidates.json", date))?;
    store.write_json(&candidates_date_path, &all_candidates)?;

    // Deduplicate against library and within-candidate
    let dedup = deduplicate_candidates(&all_candidates, snapshot, run_id);

    info!(
        input = all_candidates.len(),
        kept = dedup.candidates.len(),
        library_matches = dedup.skipped_existing.len(),
        within_dupes = dedup.duplicates.len(),
        "deduplication complete"
    );

    // Write dedup result to date directory
    let dedup_path = StatePath::new(format!("archive/{}/dedup.json", date))?;
    store.write_json(&dedup_path, &dedup)?;

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok(dedup)
}

/// Deduplicate candidates against the Zotero library and within the candidate set.
fn deduplicate_candidates(
    candidates: &[CandidatePaper],
    snapshot: &ZoteroSnapshot,
    run_id: &str,
) -> DedupResult {
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

    DedupResult {
        run_id: run_id.to_string(),
        candidates: kept,
        duplicates,
        skipped_existing,
    }
}

#[allow(clippy::too_many_arguments)]
async fn stage_embedding(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    candidates: &[CandidatePaper],
    snapshot: &ZoteroSnapshot,
    date: &str,
    event_tx: Option<&tokio::sync::broadcast::Sender<PipelineEvent>>,
    run_id: &str,
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

    let mut candidate_embs = Vec::new();
    let mut candidate_hashes: Vec<(String, String)> = Vec::new();
    let mut pending_candidates: Vec<(usize, PendingEmbedding)> = Vec::new();

    // Check candidate embedding cache first so interrupted runs can resume from saved .vec files.
    for (i, candidate) in candidates.iter().enumerate() {
        let input_text = make_input_text(&candidate.title, &candidate.abstract_text);
        let input_hash = compute_input_hash(
            provider_id,
            &config.embedding.model,
            &config_hash,
            &input_text,
        );
        let vec_path = StatePath::new(format!("cache/embeddings/{}.vec", input_hash))?;

        if let Some(bytes) = store.read_bytes(&vec_path)? {
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

    // Build interest profile and embed library
    let profile = build_interest_profile(snapshot, &config.zotero.filters);
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
                &config.embedding.model,
                &config_hash,
                &input_text,
            );
            let vec_path = StatePath::new(format!("cache/embeddings/{}.vec", input_hash))?;

            if let Some(bytes) = store.read_bytes(&vec_path)? {
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
    let progress = StageProgress::new(StageName::Embedding, pending_total, event_tx, run_id);

    if !pending_candidates.is_empty() {
        info!(
            count = pending_candidates.len(),
            local = is_local,
            "embedding uncached candidate papers"
        );
        let pending: Vec<PendingEmbedding> = pending_candidates
            .iter()
            .map(|(_, pending)| pending.clone())
            .collect();
        let embeddings =
            embed_and_cache_pending(store, config, is_local, &progress, &pending, "candidates")
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
            local = is_local,
            "embedding uncached library papers"
        );
        let pending: Vec<PendingEmbedding> = pending_library
            .iter()
            .map(|(_, _, pending)| pending.clone())
            .collect();
        let embeddings =
            embed_and_cache_pending(store, config, is_local, &progress, &pending, "library")
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

    // Sort by original order using HashMap for O(n log n) instead of O(n^2 log n)
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

    // Write embedding index to date directory (only hashes, not full vectors)
    let index = EmbeddingIndex {
        candidate_hashes,
        library_hashes,
    };
    let index_date_path = StatePath::new(format!("archive/{}/embeddings.json", date))?;
    store.write_json(&index_date_path, &index)?;

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok((candidate_embs, library_embs))
}

async fn embed_and_cache_pending(
    store: &FileStateStore,
    config: &ResolvedConfig,
    is_local: bool,
    progress: &StageProgress,
    pending: &[PendingEmbedding],
    label: &str,
) -> anyhow::Result<Vec<Vec<f32>>> {
    if pending.is_empty() {
        return Ok(Vec::new());
    }

    let batch_size = config.embedding.batch_size.max(1);

    if is_local {
        let model_name = config.embedding.model.clone();
        let cache_dir = store.root().join("cache").to_path_buf();
        let store_root = store.root().to_path_buf();
        let pending = pending.to_vec();
        let progress = progress.clone();
        let label = label.to_string();

        return tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Vec<f32>>> {
            let client = daily_paper_core::embedding::fastembed::LocalEmbeddingClient::new(
                &model_name,
                batch_size,
                Some(&cache_dir),
            )?;
            let store = FileStateStore::new(store_root);
            let mut all_embeddings = Vec::with_capacity(pending.len());
            let mut saved = 0usize;

            for chunk in pending.chunks(batch_size) {
                let texts: Vec<String> = chunk.iter().map(|item| item.text.clone()).collect();
                let embeddings = client.embed_batch(&texts)?;
                validate_embedding_count(chunk.len(), embeddings.len())?;

                for (item, embedding) in chunk.iter().zip(embeddings.iter()) {
                    store.write_bytes(&item.vec_path, &vec_to_bytes(embedding))?;
                    saved += 1;
                    progress.inc(&format!("{} {}/{}", label, saved, pending.len()));
                }

                all_embeddings.extend(embeddings);
            }

            Ok(all_embeddings)
        })
        .await?;
    }

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

async fn stage_rerank(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    candidate_embs: &[(String, Vec<f32>)],
    library_embs: &[(String, Vec<f32>, f32)],
    snapshot: &ZoteroSnapshot,
    date: &str,
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

    // Build scores vector for all ranked papers
    let paper_scores: Vec<(String, f32)> = ranked
        .iter()
        .map(|r| (r.paper_id.clone(), r.score))
        .collect();

    let selection = select_top_n(
        &rerank_cache_key,
        &ranked_ids,
        config.reader.top_n,
        paper_scores,
    );

    // Use scores from selection (already stored)
    let scores = selection.paper_scores.clone();

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

    // Write rerank result to date directory
    let rerank_date_path = StatePath::new(format!("archive/{}/rerank.json", date))?;
    store.write_json(&rerank_date_path, &selection)?;

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record.output_ref = Some(selection.selection_id.clone());
    manifest.stages.push(record);

    Ok((selection, scores))
}

#[allow(clippy::too_many_arguments)]
async fn generate_tldr_fallback(
    reader: &ReaderClient,
    candidate: &CandidatePaper,
    authors: &str,
    selected_text: &str,
    language: &str,
    max_input_tokens: usize,
    paper_id: &str,
) -> anyhow::Result<(String, Option<TokenUsage>)> {
    let system_prompt = build_tldr_system_prompt();
    let user_prompt = build_tldr_user_prompt(
        &candidate.title,
        &candidate.abstract_text,
        authors,
        selected_text,
        language,
    );
    let trimmed_prompt = trim_to_token_budget(&user_prompt, max_input_tokens * 4);
    let (raw_response, token_usage) = reader
        .complete(&system_prompt, &trimmed_prompt)
        .await
        .with_context(|| format!("TLDR fallback LLM call failed for {}", paper_id))?;
    let summary = parse_tldr_output(&raw_response)
        .map_err(|e| anyhow::anyhow!("TLDR fallback returned invalid format: {}", e))?;
    Ok((summary, token_usage))
}

#[allow(clippy::too_many_arguments)]
async fn stage_deep_read(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    selected_ids: &[String],
    candidates: &[CandidatePaper],
    force_read: bool,
    date: &str,
    event_tx: Option<&tokio::sync::broadcast::Sender<PipelineEvent>>,
    run_id: &str,
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

    let progress = StageProgress::new(StageName::DeepRead, selected_ids.len(), event_tx, run_id);

    let max_attempts = if config.reader.on_read_failure == "retry" {
        3
    } else {
        1
    };

    for paper_id in selected_ids {
        let candidate = match candidates.iter().find(|c| &c.paper_id == paper_id) {
            Some(c) => c,
            None => {
                warn!(paper_id = %paper_id, "selected paper not in candidates");
                any_failure = true;
                continue;
            }
        };

        // Check read result cache (not retried)
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
                    progress.inc(&format!("{} (cached)", paper_id));
                    read_results.push(result);
                    continue;
                }
            }
        }

        // Check for PDF URL (permanent failure, not retried)
        let pdf_url = match candidate.pdf_url.as_deref() {
            Some(url) => url,
            None => {
                warn!(paper_id = %paper_id, "no PDF URL, skipping");
                any_failure = true;
                continue;
            }
        };

        // Retry loop for transient operations
        let mut result: Option<ReadResult> = None;
        for attempt in 1..=max_attempts {
            if attempt > 1 {
                warn!(
                    paper_id = %paper_id,
                    attempt,
                    max = max_attempts,
                    "retrying paper"
                );
                progress.inc(&format!(
                    "{} (retry {}/{})",
                    paper_id, attempt, max_attempts
                ));
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            } else {
                info!(paper_id = %paper_id, title = %candidate.title, "deep reading");
                progress.inc(&format!("{} - {}", paper_id, &candidate.title));
            }

            // Download PDF
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
                    warn!(paper_id = %paper_id, attempt, error = %e, "PDF download failed");
                    if attempt == max_attempts {
                        mark_stage_blocked(
                            manifest,
                            &format!("PDF download failed for {}: {}", paper_id, e),
                        );
                    }
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
                    warn!(paper_id = %paper_id, attempt, error = %e, "PDF extract failed");
                    if attempt == max_attempts {
                        mark_stage_blocked(
                            manifest,
                            &format!("PDF extract failed for {}: {}", paper_id, e),
                        );
                    }
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

            let trimmed_prompt =
                trim_to_token_budget(&user_prompt, config.reader.max_input_tokens * 4);

            let structured_result = match reader.complete(&system_prompt, &trimmed_prompt).await {
                Ok((raw_response, token_usage)) => {
                    // Parse LLM output strictly; malformed content must not enter the report cache.
                    match parse_llm_output(&raw_response) {
                        Ok(parsed) => Some((parsed, token_usage)),
                        Err(e) => {
                            warn!(
                                paper_id = %paper_id,
                                attempt,
                                error = %e,
                                "LLM read returned invalid report format"
                            );
                            if attempt < max_attempts {
                                continue;
                            }
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!(paper_id = %paper_id, attempt, error = %e, "LLM read failed");
                    if attempt < max_attempts {
                        continue;
                    }
                    None
                }
            };

            let (
                summary,
                author_affiliations,
                llm_project_url,
                llm_code_url,
                token_usage,
                warnings,
            ) = if let Some((parsed, token_usage)) = structured_result {
                let author_affiliations = parsed
                    .author_affiliations
                    .iter()
                    .map(|aa| daily_paper_core::models::read::AuthorAffiliation {
                        name: aa.name.clone(),
                        affiliation: aa.affiliation.clone(),
                    })
                    .collect();
                (
                    parsed.summary.clone(),
                    author_affiliations,
                    parsed.project_url.clone(),
                    parsed.code_url.clone(),
                    token_usage,
                    Vec::new(),
                )
            } else {
                match generate_tldr_fallback(
                    &reader,
                    candidate,
                    &authors_str,
                    &selected_text,
                    &config.reader.language,
                    config.reader.max_input_tokens,
                    paper_id,
                )
                .await
                {
                    Ok((summary, token_usage)) => {
                        warn!(paper_id = %paper_id, "using TLDR fallback for deep read report");
                        (
                            summary,
                            Vec::new(),
                            None,
                            None,
                            token_usage,
                            vec!["structured_read_failed; used_tldr_fallback".to_string()],
                        )
                    }
                    Err(e) => {
                        mark_stage_blocked(
                            manifest,
                            &format!(
                                "LLM read failed for {}; TLDR fallback also failed: {}",
                                paper_id, e
                            ),
                        );
                        continue;
                    }
                }
            };

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
                warnings,
            };

            // Cache read result
            let read_result_path = StatePath::new(format!(
                "cache/papers/{}/read/{}.json",
                paper_id, read_cache_key
            ))?;
            store.write_json(&read_result_path, &read_result)?;

            // Write read result to date directory
            let read_date_path =
                StatePath::new(format!("archive/{}/read/{}.json", date, paper_id))?;
            store.write_json(&read_date_path, &read_result)?;

            result = Some(read_result);
            break;
        }

        if let Some(r) = result {
            read_results.push(r);
        } else {
            any_failure = true;
        }
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
    progress.finish(&format!(
        "{}/{} succeeded",
        read_results.len(),
        selected_ids.len()
    ));

    // Store paper IDs as output_ref for cache loading
    let paper_ids_str = read_results
        .iter()
        .map(|r| r.paper_id.as_str())
        .collect::<Vec<_>>()
        .join(",");

    record.status = if read_results.len() == selected_ids.len() {
        StageStatus::Succeeded
    } else {
        StageStatus::Failed
    };
    record.finished_at = Some(Utc::now());
    record.output_ref = Some(paper_ids_str);
    manifest.stages.push(record);

    Ok(read_results)
}

struct RenderInput<'a> {
    selected_ids: &'a [String],
    candidates: &'a [CandidatePaper],
    read_results: &'a [ReadResult],
    scores: &'a [(String, f32)],
    date: &'a str,
}

async fn stage_render(
    store: &FileStateStore,
    manifest: &mut RunManifest,
    run_id: &str,
    input: RenderInput<'_>,
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

    let report_papers: Vec<ReportPaper> = input
        .selected_ids
        .iter()
        .enumerate()
        .filter_map(|(i, paper_id)| {
            let candidate = input.candidates.iter().find(|c| &c.paper_id == paper_id)?;
            let read_result = input
                .read_results
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
                score: input
                    .scores
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

    // Write report files to cache (versioned)
    let cache_html_path = format!("cache/reports/{}/report.html", run_id);
    let cache_text_path = format!("cache/reports/{}/report.txt", run_id);

    store.write_string(&StatePath::new(&cache_html_path)?, &html_body)?;
    store.write_string(&StatePath::new(&cache_text_path)?, &text_body)?;

    // Write report files to archive
    let archive_html_path = format!("archive/{}/report/report.html", input.date);
    let archive_text_path = format!("archive/{}/report/report.txt", input.date);

    store.write_string(&StatePath::new(&archive_html_path)?, &html_body)?;
    store.write_string(&StatePath::new(&archive_text_path)?, &text_body)?;

    let report_hash = sha256_hex(html_body.as_bytes());
    let report_instance_id = format!("{}-{}", run_id, &report_hash[..8]);
    let generated_at = Utc::now();

    let rendered = RenderedReport {
        report_hash: report_hash.clone(),
        report_instance_id: report_instance_id.clone(),
        run_id: run_id.to_string(),
        generated_at,
        title,
        html_path: store
            .root()
            .join(&cache_html_path)
            .to_string_lossy()
            .to_string(),
        text_path: Some(
            store
                .root()
                .join(&cache_text_path)
                .to_string_lossy()
                .to_string(),
        ),
        ranked_paper_ids: input.selected_ids.to_vec(),
        read_paper_ids: input
            .read_results
            .iter()
            .map(|r| r.paper_id.clone())
            .collect(),
    };

    // Write report metadata to cache for send stage
    let report_meta_path = StatePath::new(format!("cache/reports/{}/report.json", run_id))?;
    store.write_json(&report_meta_path, &rendered)?;

    info!(
        papers = report_papers.len(),
        html = %archive_html_path,
        "render complete"
    );

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok((
        store
            .root()
            .join(&archive_html_path)
            .to_string_lossy()
            .to_string(),
        Some(
            store
                .root()
                .join(&archive_text_path)
                .to_string_lossy()
                .to_string(),
        ),
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
    let report_meta_path = StatePath::new(format!("cache/reports/{}/report.json", run_id))?;
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
        let receipt_path =
            StatePath::new(format!("cache/deliveries/history/{}.json", delivery_key))?;
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
        &daily_paper_core::deliver::receipt::EmailDelivery {
            report_hash: &rendered.report_hash,
            report_instance_id: &rendered.report_instance_id,
            run_id,
            html_path: Path::new(html_path),
            text_path: text_path.map(Path::new),
            smtp_server: &config.email.smtp_server,
            smtp_port: config.email.smtp_port,
            sender: &config.email.sender,
            receiver: &config.email.receiver,
            password: &config.email.password,
            subject: &subject,
            now,
        },
    )
    .context("failed to send email")?;

    // Save delivery receipt
    let receipt_path = StatePath::new(format!("cache/deliveries/history/{}.json", delivery_key))?;
    store.write_json(&receipt_path, &receipt)?;

    info!(delivery_key = %delivery_key, "email sent successfully");

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    manifest.stages.push(record);

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────

pub(crate) fn generate_run_id() -> String {
    let now = Local::now();
    let ts = now.format("%Y%m%d-%H%M%S").to_string();
    let suffix: String = (0..6)
        .map(|_| format!("{:x}", fastrand::u8(0..16)))
        .collect();
    format!("{}-{}", ts, suffix)
}

/// arXiv RSS announcement pubDate is midnight US Eastern (UTC-4 during DST).
const ARXIV_ANNOUNCEMENT_UTC_OFFSET_HOURS: i64 = 4;

fn default_arxiv_announcement_date() -> NaiveDate {
    arxiv_announcement_date_for(Utc::now())
}

fn arxiv_announcement_date_for(now: chrono::DateTime<Utc>) -> NaiveDate {
    (now - Duration::hours(ARXIV_ANNOUNCEMENT_UTC_OFFSET_HOURS)).date_naive()
}

pub(crate) fn compute_date_window(date_arg: Option<&str>) -> anyhow::Result<DateWindow> {
    let target_date = match date_arg {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .context(format!("invalid date '{}', expected YYYY-MM-DD", s))?,
        None => default_arxiv_announcement_date(),
    };

    let start = target_date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end = (target_date + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    Ok(DateWindow {
        start,
        end,
        label: target_date.format("%Y-%m-%d").to_string(),
    })
}

pub(crate) fn build_cli_overrides(args: &RunArgs) -> Vec<CliOverride> {
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
                    if is_valid_structured_summary(&result.summary) {
                        return Ok(Some(state_path.as_path().to_string_lossy().to_string()));
                    }
                    warn!(
                        paper_id = %paper_id,
                        path = %state_path.as_path().display(),
                        "ignoring cached read result with invalid summary format"
                    );
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

pub(crate) fn classify_error(e: &anyhow::Error) -> ErrorKind {
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

pub(crate) fn find_source_run(
    store: &FileStateStore,
    from_run: Option<&str>,
    stage_filter: Option<&[StageName]>,
) -> anyhow::Result<String> {
    if let Some(id) = from_run {
        return Ok(id.to_string());
    }
    // Find latest run with a manifest that has stages
    let runs_dir = store.root().join("cache/runs");
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

    // Find the first entry with stages that has the required stages
    for entry in &entries {
        let manifest_path = StatePath::new(format!(
            "cache/runs/{}/manifest.json",
            entry.file_name().to_string_lossy()
        ))?;
        if let Ok(Some(manifest)) = store.read_json::<RunManifest>(&manifest_path) {
            if manifest.stages.is_empty() {
                continue;
            }
            // If we have a stage filter, check that the run has all required stages
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
            return Ok(entry.file_name().to_string_lossy().to_string());
        }
    }

    anyhow::bail!("no runs found with completed stages")
}

fn stage_output_ref(manifest: Option<&RunManifest>, stage: StageName) -> Option<&str> {
    manifest?
        .stages
        .iter()
        .find(|s| s.stage == stage && s.status == StageStatus::Succeeded)
        .and_then(|s| s.output_ref.as_deref())
}

fn load_cached_snapshot(
    store: &FileStateStore,
    date: &str,
    source: Option<&RunManifest>,
) -> anyhow::Result<ZoteroSnapshot> {
    if let Some(snapshot_id) = stage_output_ref(source, StageName::ZoteroSync) {
        let snap_path = StatePath::new(format!("cache/zotero/snapshots/{}.json", snapshot_id))?;
        if let Some(snapshot) = store.read_json::<ZoteroSnapshot>(&snap_path)? {
            return Ok(snapshot);
        }
        anyhow::bail!("cached snapshot not found: {}", snapshot_id);
    }

    // Try to read snapshot ID from date directory
    let snapshot_id_path = StatePath::new(format!("archive/{}/snapshot_id.txt", date))?;
    if let Some(snapshot_id) = store.read_string(&snapshot_id_path)? {
        let snap_path = StatePath::new(format!("cache/zotero/snapshots/{}.json", snapshot_id))?;
        if let Some(snapshot) = store.read_json::<ZoteroSnapshot>(&snap_path)? {
            return Ok(snapshot);
        }
    }

    // Fallback: find the most recent snapshot
    let snapshots_dir = store.root().join("cache/zotero/snapshots");
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
            // Make path relative to store root
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

fn load_cached_candidates(store: &FileStateStore, date: &str) -> anyhow::Result<DedupResult> {
    // Try to read dedup result from date directory
    let dedup_path = StatePath::new(format!("archive/{}/dedup.json", date))?;
    if let Some(dedup) = store.read_json::<DedupResult>(&dedup_path)? {
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
fn load_cached_embeddings(
    store: &FileStateStore,
    date: &str,
    candidates: &[CandidatePaper],
    _snapshot: &ZoteroSnapshot,
) -> anyhow::Result<(Vec<(String, Vec<f32>)>, Vec<(String, Vec<f32>, f32)>)> {
    // Try to read embedding index from date directory
    let index_path = StatePath::new(format!("archive/{}/embeddings.json", date))?;
    if let Some(index) = store.read_json::<EmbeddingIndex>(&index_path)? {
        // Validate that cached embeddings match current candidates
        let current_ids: std::collections::HashSet<&str> =
            candidates.iter().map(|c| c.paper_id.as_str()).collect();
        let cached_ids: std::collections::HashSet<&str> = index
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

        // Load candidate embeddings from .vec files
        let mut candidate_embs = Vec::new();
        for (paper_id, input_hash) in &index.candidate_hashes {
            let vec_path = StatePath::new(format!("cache/embeddings/{}.vec", input_hash))?;
            if let Some(bytes) = store.read_bytes(&vec_path)? {
                let vec = bytes_to_vec(&bytes);
                candidate_embs.push((paper_id.clone(), vec));
            } else {
                anyhow::bail!("embedding .vec file not found: {}", input_hash);
            }
        }

        // Load library embeddings from .vec files
        let mut library_embs = Vec::new();
        for (lib_id, weight, input_hash) in &index.library_hashes {
            let vec_path = StatePath::new(format!("cache/embeddings/{}.vec", input_hash))?;
            if let Some(bytes) = store.read_bytes(&vec_path)? {
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

fn load_cached_rerank(
    store: &FileStateStore,
    date: &str,
    source: Option<&RunManifest>,
) -> anyhow::Result<(
    daily_paper_core::rerank::selection::ReadSelection,
    Vec<(String, f32)>,
)> {
    if let Some(selection_id) = stage_output_ref(source, StageName::Rerank) {
        let path = StatePath::new(format!("cache/rerank/{}.json", selection_id))?;
        if let Some(selection) =
            store.read_json::<daily_paper_core::rerank::selection::ReadSelection>(&path)?
        {
            let scores = selection.paper_scores.clone();
            return Ok((selection, scores));
        }
        anyhow::bail!("cached rerank not found: {}", selection_id);
    }

    // Try to read from date directory
    let rerank_path = StatePath::new(format!("archive/{}/rerank.json", date))?;
    if let Some(rerank) =
        store.read_json::<daily_paper_core::rerank::selection::ReadSelection>(&rerank_path)?
    {
        let scores = rerank.paper_scores.clone();
        return Ok((rerank, scores));
    }

    anyhow::bail!("no cached rerank found for date {}", date)
}

fn load_cached_read_results(
    store: &FileStateStore,
    date: &str,
    source: Option<&RunManifest>,
) -> anyhow::Result<Vec<ReadResult>> {
    if let Some(read_ref) = stage_output_ref(source, StageName::DeepRead) {
        let mut results = Vec::new();
        for paper_id in read_ref.split(',').filter(|id| !id.is_empty()) {
            let path = StatePath::new(format!("archive/{}/read/{}.json", date, paper_id))?;
            if let Some(result) = store.read_json::<ReadResult>(&path)? {
                results.push(result);
            }
        }
        if !results.is_empty() {
            return Ok(results);
        }
    }

    // Try to read from date directory
    let read_dir = store.root().join(format!("archive/{}/read", date));
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

fn load_cached_render(
    store: &FileStateStore,
    date: &str,
    source: Option<&RunManifest>,
) -> anyhow::Result<(String, Option<String>)> {
    if let Some(source) = source {
        let html_path = store
            .root()
            .join(format!("cache/reports/{}/report.html", source.run_id));
        if html_path.exists() {
            let text_path = store
                .root()
                .join(format!("cache/reports/{}/report.txt", source.run_id));
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

    // Read report directly from archive
    let archive_html_path = StatePath::new(format!("archive/{}/report/report.html", date))?;
    if store.exists(&archive_html_path)? {
        let html = store
            .root()
            .join(archive_html_path.as_path())
            .to_string_lossy()
            .to_string();
        let text_path = StatePath::new(format!("archive/{}/report/report.txt", date))?;
        let text = if store.exists(&text_path)? {
            Some(
                store
                    .root()
                    .join(text_path.as_path())
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn default_date_window_uses_arxiv_announcement_date() {
        let window = compute_date_window(None).unwrap();
        assert_eq!(window.label, default_arxiv_announcement_date().to_string());
    }

    #[test]
    fn explicit_date_window_uses_requested_date() {
        let window = compute_date_window(Some("2026-06-06")).unwrap();
        assert_eq!(window.label, "2026-06-06");
        assert_eq!(window.end - window.start, chrono::Duration::days(1));
    }

    #[test]
    fn arxiv_announcement_date_uses_utc_minus_four_boundary() {
        let before_boundary = Utc.with_ymd_and_hms(2026, 6, 9, 3, 59, 59).unwrap();
        let after_boundary = Utc.with_ymd_and_hms(2026, 6, 9, 4, 0, 0).unwrap();

        assert_eq!(
            arxiv_announcement_date_for(before_boundary).to_string(),
            "2026-06-08"
        );
        assert_eq!(
            arxiv_announcement_date_for(after_boundary).to_string(),
            "2026-06-09"
        );
    }
}
