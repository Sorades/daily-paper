pub mod cache_loader;
pub mod context;
pub mod stage_deep_read;
pub mod stage_embedding;
pub mod stage_render;
pub mod stage_rerank;
pub mod stage_send;
pub mod stage_source;
pub mod stage_zotero;

use std::path::Path;
use anyhow::Context;
use chrono::Utc;
use tracing::{info, warn};

use crate::cli::RunArgs;
use crate::config::{load_config, ResolvedConfig};
use crate::models::common::sha256_hex;
use crate::models::run::*;
use crate::pipeline::PipelineOptions;
use crate::state::keys::StorageKey;
use crate::state::path::StatePath;
use crate::state::store::FileStateStore;

pub(crate) use cache_loader::find_source_run;
pub(crate) use context::{
    build_cli_overrides, compute_date_window_with_tz, error_to_kind, generate_run_id,
};

/// Parameters for preparing and launching a pipeline run session.
pub struct PreparedPipeline {
    pub run_id: String,
    pub manifest: RunManifest,
    pub manifest_path: StatePath,
    pub date_window: crate::models::common::DateWindow,
    pub stage_filter: Option<Vec<StageName>>,
    pub source_manifest: Option<RunManifest>,
    pub store: std::sync::Arc<FileStateStore>,
    pub config: std::sync::Arc<ResolvedConfig>,
    pub event_tx: Option<tokio::sync::broadcast::Sender<PipelineEvent>>,
}

impl PreparedPipeline {
    /// Prepare a pipeline run session by discovering source run, creating manifest and acquiring lock.
    pub fn prepare(
        store: std::sync::Arc<FileStateStore>,
        config: std::sync::Arc<ResolvedConfig>,
        config_path: &Path,
        options: &PipelineOptions,
        event_tx: Option<tokio::sync::broadcast::Sender<PipelineEvent>>,
    ) -> anyhow::Result<Self> {
        let stage_filter: Option<Vec<StageName>> = if options.stages.is_empty() {
            None
        } else {
            Some(options.stages.clone())
        };

        let source_manifest: Option<RunManifest> = if let Some(ref filter) = stage_filter {
            let source_run_id =
                find_source_run(&store, options.from_run.as_deref(), Some(filter))?;
            info!(source_run = %source_run_id, "loading source run for cached data");
            let manifest_key = StorageKey::RunManifest { run_id: &source_run_id };
            let m: Option<RunManifest> = store.get_json(&manifest_key)?;
            if m.is_none() {
                warn!("source run manifest not found, stages will need to re-fetch data");
            }
            m
        } else {
            None
        };

        let run_id = generate_run_id();
        let date_window = compute_date_window_with_tz(
            options.date.as_deref(),
            config.schedule.timezone.as_deref(),
        )?;

        let manifest = RunManifest {
            run_id: run_id.clone(),
            parent_run_id: source_manifest.as_ref().map(|m| m.run_id.clone()),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            config_hash: sha256_hex(config_path.to_string_lossy().as_bytes()),
            cli_overrides: build_cli_overrides(options),
            date_window: date_window.clone(),
            stages: Vec::new(),
            warnings: Vec::new(),
            error: None,
        };

        let manifest_key = StorageKey::RunManifest { run_id: &run_id };
        store.put_json(&manifest_key, &manifest)?;
        let manifest_path = manifest_key.to_state_path()?;

        Ok(Self {
            run_id,
            manifest,
            manifest_path,
            date_window,
            stage_filter,
            source_manifest,
            store,
            config,
            event_tx,
        })
    }

    /// Execute the prepared pipeline to completion and record outcomes.
    pub async fn execute(&mut self, options: &PipelineOptions) -> anyhow::Result<()> {
        let _lock = self
            .store
            .acquire_lock(&self.run_id, std::time::Duration::from_secs(3600))
            .context("failed to acquire run lock")?;

        let stage_filter_ref = self.stage_filter.as_deref();
        let mut cx = context::ExecutionContext {
            store: &self.store,
            config: &self.config,
            manifest: &mut self.manifest,
            run_id: &self.run_id,
            date_window: &self.date_window,
            stage_filter: stage_filter_ref,
            source_manifest: self.source_manifest.as_ref(),
            event_tx: self.event_tx.as_ref(),
            manifest_path: &self.manifest_path,
        };

        let result = run_pipeline(&mut cx, options).await;

        self.manifest.finished_at = Some(Utc::now());
        match &result {
            Ok(()) => {
                if self.manifest.status == RunStatus::Running {
                    self.manifest.status = RunStatus::Succeeded;
                    info!(run_id = %self.run_id, "pipeline completed successfully");
                } else {
                    info!(run_id = %self.run_id, status = ?self.manifest.status, "pipeline settled");
                }
                if let Some(ref tx) = self.event_tx {
                    let _ = tx.send(PipelineEvent::Ended {
                        run_id: self.run_id.clone(),
                        status: self.manifest.status.clone(),
                    });
                }
            }
            Err(e) => {
                self.manifest.status = RunStatus::Failed;
                self.manifest.error = Some(ErrorRecord {
                    kind: error_to_kind(e),
                    message: e.to_string(),
                    retryable: false,
                    context: serde_json::Value::Null,
                    occurred_at: Utc::now(),
                });
                warn!(run_id = %self.run_id, error = %e, "pipeline failed");
                if let Some(ref tx) = self.event_tx {
                    let _ = tx.send(PipelineEvent::Ended {
                        run_id: self.run_id.clone(),
                        status: RunStatus::Failed,
                    });
                }
            }
        }

        let _ = self.store.put_json(&StorageKey::RunManifest { run_id: &self.run_id }, &self.manifest);
        result
    }

    /// Mark this prepared pipeline as cancelled and safely flush manifest and events.
    pub fn cancel(&mut self) -> anyhow::Result<()> {
        self.manifest.status = RunStatus::Cancelled;
        self.manifest.finished_at = Some(Utc::now());
        self.manifest.warnings.push(WarningRecord {
            kind: "cancelled".to_string(),
            message: "pipeline execution was cancelled by user".to_string(),
            context: serde_json::Value::Null,
        });

        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(PipelineEvent::Ended {
                run_id: self.run_id.clone(),
                status: RunStatus::Cancelled,
            });
        }

        self.store.put_json(&StorageKey::RunManifest { run_id: &self.run_id }, &self.manifest)?;
        info!(run_id = %self.run_id, "pipeline marked as cancelled and saved");
        Ok(())
    }
}

pub async fn execute(data_dir: &Path, args: RunArgs) -> anyhow::Result<()> {
    let options = PipelineOptions {
        date: args.date,
        stages: args
            .stages
            .iter()
            .map(|s| {
                StageName::from_kebab(s).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown stage '{}'. Valid: zotero-sync, source-fetch, embedding, rerank, deep-read, render, send",
                        s
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        from_run: args.from_run,
        dry_run: args.dry_run,
        send_email: args.send_email,
        no_email: args.no_email,
        force_zotero_sync: args.force_zotero_sync,
        force_embedding: args.force_embedding,
        force_rerank: args.force_rerank,
        force_read: args.force_read,
        force_send: args.force_send,
        max_candidates: args.max_candidates,
    };

    let config_path = data_dir.join("config.toml");
    let (_raw, resolved) = load_config(&config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;

    info!(config = %config_path.display(), "config loaded");
    info!(data_dir = %data_dir.display(), "data directory");

    let store = std::sync::Arc::new(FileStateStore::new(data_dir.to_path_buf()));
    store.ensure_dirs()?;

    let mut prepared = PreparedPipeline::prepare(
        store,
        std::sync::Arc::new(resolved),
        &config_path,
        &options,
        None,
    )?;
    info!(run_id = %prepared.run_id, "starting pipeline run");
    info!(
        start = %prepared.date_window.start,
        end = %prepared.date_window.end,
        label = %prepared.date_window.label,
        "date window"
    );

    prepared.execute(&options).await
}

pub async fn run_pipeline(
    cx: &mut context::ExecutionContext<'_>,
    options: &PipelineOptions,
) -> anyhow::Result<()> {
    let date = cx.date().to_string();
    let cache_date = cx.cache_date().to_string();
    cx.store.ensure_date_dir(&date)?;

    let mut completed_stages: Vec<StageName> = Vec::new();

    let all_requested_done = |completed: &[StageName], filter: Option<&[StageName]>| -> bool {
        if let Some(filter) = filter {
            filter.iter().all(|s| completed.contains(s))
        } else {
            false
        }
    };

    // Stage 1: Zotero sync
    let snapshot = if cx.should_run(&StageName::ZoteroSync) {
        cx.emit_stage_start(StageName::ZoteroSync);
        let r = stage_zotero::stage_zotero_sync(
            cx,
            options.force_zotero_sync,
        )
        .await;
        cx.emit_stage_finish(StageName::ZoteroSync, &r);
        cx.save_manifest()?;
        completed_stages.push(StageName::ZoteroSync);
        let snapshot = r?;
        if all_requested_done(&completed_stages, cx.stage_filter) {
            info!("all requested stages completed");
            return Ok(());
        }
        snapshot
    } else {
        cache_loader::load_cached_snapshot(cx.store, &cache_date, cx.source_manifest)?
    };

    // Stage 2: Source fetch
    let dedup = if cx.should_run(&StageName::SourceFetch) {
        cx.emit_stage_start(StageName::SourceFetch);
        let r = stage_source::stage_source_fetch(
            cx,
            options.date.is_some(),
            &snapshot,
            options.max_candidates,
        )
        .await;
        cx.emit_stage_finish(StageName::SourceFetch, &r);
        cx.save_manifest()?;
        completed_stages.push(StageName::SourceFetch);
        let dedup = r?;
        if all_requested_done(&completed_stages, cx.stage_filter) {
            info!("all requested stages completed");
            return Ok(());
        }
        dedup
    } else {
        cache_loader::load_cached_candidates(cx.store, &cache_date)?
    };

    if dedup.candidates.is_empty() {
        warn!("no candidate papers found after dedup");
        context::add_warning(cx.manifest, "source", "no candidate papers found");
        cx.manifest.status = RunStatus::Blocked;
        return Ok(());
    }

    // Stage 4: Embedding
    let (candidate_embs, library_embs) = if cx.should_run(&StageName::Embedding) {
        cx.emit_stage_start(StageName::Embedding);
        let r = stage_embedding::stage_embedding(
            cx,
            &dedup.candidates,
            &snapshot,
            options.force_embedding,
        )
        .await;
        cx.emit_stage_finish(StageName::Embedding, &r);
        cx.save_manifest()?;
        completed_stages.push(StageName::Embedding);
        r?
    } else {
        cache_loader::load_cached_embeddings(cx.store, &cache_date, &dedup.candidates, &snapshot)?
    };

    if all_requested_done(&completed_stages, cx.stage_filter) {
        info!("all requested stages completed");
        return Ok(());
    }

    // Stage 5: Rerank + selection
    let (selection, rerank_scores) = if cx.should_run(&StageName::Rerank) {
        cx.emit_stage_start(StageName::Rerank);
        let r = stage_rerank::stage_rerank(
            cx,
            &candidate_embs,
            &library_embs,
            &snapshot,
        )
        .await;
        cx.emit_stage_finish(StageName::Rerank, &r);
        cx.save_manifest()?;
        completed_stages.push(StageName::Rerank);
        r?
    } else {
        cache_loader::load_cached_rerank(cx.store, &cache_date, cx.source_manifest)?
    };

    if all_requested_done(&completed_stages, cx.stage_filter) {
        info!("all requested stages completed");
        return Ok(());
    }

    if selection.selected_paper_ids.is_empty() {
        warn!("no papers selected after rerank");
        context::add_warning(cx.manifest, "rerank", "no papers selected");
        cx.manifest.status = RunStatus::Blocked;
        return Ok(());
    }

    // Stage 6-9: Deep read
    let read_results = if cx.should_run(&StageName::DeepRead) {
        cx.emit_stage_start(StageName::DeepRead);
        let r = stage_deep_read::stage_deep_read(
            cx,
            &selection.selected_paper_ids,
            &dedup.candidates,
            options.force_read,
        )
        .await;
        cx.emit_stage_finish(StageName::DeepRead, &r);
        cx.save_manifest()?;
        completed_stages.push(StageName::DeepRead);
        r?
    } else {
        cache_loader::load_cached_read_results(cx.store, &cache_date, cx.source_manifest)?
    };

    if all_requested_done(&completed_stages, cx.stage_filter) {
        info!("all requested stages completed");
        return Ok(());
    }

    // Stage 10: Render
    let (html_path, text_path) = if cx.should_run(&StageName::Render) {
        cx.emit_stage_start(StageName::Render);
        let r = stage_render::stage_render(
            cx,
            stage_render::RenderInput {
                selected_ids: &selection.selected_paper_ids,
                candidates: &dedup.candidates,
                read_results: &read_results,
                scores: &rerank_scores,
            },
        )
        .await;
        cx.emit_stage_finish(StageName::Render, &r);
        cx.save_manifest()?;
        completed_stages.push(StageName::Render);
        r?
    } else {
        cache_loader::load_cached_render(cx.store, &cache_date, cx.source_manifest)?
    };

    // Stage 11: Send
    if cx.should_run(&StageName::Send) {
        let should_send = if options.dry_run || options.no_email {
            false
        } else if options.send_email {
            true
        } else if cfg!(debug_assertions) {
            false
        } else {
            true
        };

        if !should_send {
            if options.dry_run {
                info!("dry-run: skipping email send");
            } else {
                info!("email skipped (use --send-email to deliver in dev)");
            }
            context::record_stage(
                cx.manifest,
                StageRecord {
                    stage: StageName::Send,
                    status: StageStatus::Skipped,
                    started_at: Utc::now(),
                    finished_at: Some(Utc::now()),
                    cache_hit: false,
                    input_hash: None,
                    output_ref: None,
                    error: None,
                },
            );
        } else {
            let report_run_id = if cx.should_run(&StageName::Render) {
                cx.run_id
            } else {
                cx.source_manifest.map(|m| m.run_id.as_str()).unwrap_or(cx.run_id)
            };
            cx.emit_stage_start(StageName::Send);
            let r = stage_send::stage_send(
                cx,
                report_run_id,
                &html_path,
                text_path.as_deref(),
                options.force_send,
            )
            .await;
            cx.emit_stage_finish(StageName::Send, &r);
            cx.save_manifest()?;
            completed_stages.push(StageName::Send);
            r?
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::context::*;
    use chrono::TimeZone;

    #[test]
    fn default_date_window_uses_local_or_tz_date() {
        let window = compute_date_window(None).unwrap();
        let expected_date = chrono::Local::now().date_naive().to_string();
        assert_eq!(window.label, expected_date);
    }

    #[test]
    fn explicit_date_window_uses_requested_date() {
        let window = compute_date_window(Some("2026-06-06")).unwrap();
        assert_eq!(window.label, "2026-06-06");
        assert_eq!(window.end - window.start, chrono::Duration::days(1));
    }

    #[test]
    fn arxiv_announcement_date_uses_utc_minus_four_boundary() {
        let before_boundary = chrono::Utc.with_ymd_and_hms(2026, 6, 9, 3, 59, 59).unwrap();
        let after_boundary = chrono::Utc.with_ymd_and_hms(2026, 6, 9, 4, 0, 0).unwrap();

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
