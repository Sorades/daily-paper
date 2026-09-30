use std::sync::atomic::Ordering;
use axum::http::StatusCode;

use crate::models::run::*;
use crate::pipeline::PipelineOptions;
use crate::state::store::FileStateStore;

use crate::commands::run::PreparedPipeline;

use super::state::{AppState, RunningGuard};
use super::types::{api_error, ApiError, RunRequest};

pub(crate) fn parse_stage_filter(stages: &[String]) -> Result<Option<Vec<StageName>>, ApiError> {
    if stages.is_empty() {
        return Ok(None);
    }
    let mut parsed = Vec::new();
    for s in stages {
        let stage = StageName::from_kebab(s).ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                format!("unknown stage '{}'", s),
            )
        })?;
        parsed.push(stage);
    }
    Ok(Some(parsed))
}

pub(crate) fn build_pipeline_options(req: &RunRequest) -> Result<PipelineOptions, ApiError> {
    let stages = req.stages.clone().unwrap_or_default();
    let stage_filter = parse_stage_filter(&stages)?.unwrap_or_default();

    Ok(PipelineOptions {
        date: req.date.clone(),
        stages: stage_filter,
        from_run: req.from_run.clone(),
        dry_run: req.dry_run.unwrap_or(true),
        send_email: req.send_email.unwrap_or(false),
        no_email: req.no_email.unwrap_or(true),
        force_zotero_sync: req.force_zotero_sync.unwrap_or(false),
        force_embedding: req.force_embedding.unwrap_or(false),
        force_rerank: req.force_rerank.unwrap_or(false),
        force_read: req.force_read.unwrap_or(false),
        force_send: req.force_send.unwrap_or(false),
        max_candidates: req.max_candidates,
    })
}

pub(crate) async fn spawn_pipeline(state: &AppState, req: &RunRequest) -> Result<String, ApiError> {
    let options = build_pipeline_options(req)?;

    let running = state.pipeline_running.clone();
    if running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(api_error(StatusCode::CONFLICT, "pipeline already running"));
    }

    let resolved = std::sync::Arc::new(state.config.read().await.clone());
    let store = state.store.clone();
    let config_path = state.config_path.clone();
    let options_clone = options.clone();
    let event_tx = state.pipeline_tx.clone();

    let prepared = match tokio::task::spawn_blocking(move || {
        PreparedPipeline::prepare(
            store,
            resolved,
            &config_path,
            &options_clone,
            Some(event_tx),
        )
    })
    .await
    {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => {
            running.store(false, Ordering::SeqCst);
            return Err(api_error(StatusCode::BAD_REQUEST, e.to_string()));
        }
        Err(e) => {
            running.store(false, Ordering::SeqCst);
            return Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
    };

    let run_id = prepared.run_id.clone();
    let _ = state.pipeline_tx.send(PipelineEvent::Started {
        run_id: run_id.clone(),
    });

    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    {
        let mut cancel_lock = state.cancel_tx.lock().await;
        *cancel_lock = Some(cancel_tx);
    }

    let running_bg = running.clone();
    let cancel_state = state.cancel_tx.clone();
    tokio::spawn(async move {
        let _running_guard = RunningGuard(running_bg);
        let mut prepared = prepared;
        tokio::select! {
            _ = cancel_rx.wait_for(|&cancelled| cancelled) => {
                tracing::warn!("pipeline execution cancelled by user");
                let _ = prepared.cancel();
            }
            res = prepared.execute(&options) => {
                let _ = res;
            }
        }
        let mut lock = cancel_state.lock().await;
        *lock = None;
    });

    Ok(run_id)
}

/// Execute pipeline synchronously and wait for completion (used for immediate feedback in api_run_send)
pub(crate) async fn run_pipeline_sync_wait(
    state: &AppState,
    req: &RunRequest,
) -> Result<String, ApiError> {
    let options = build_pipeline_options(req)?;

    let running = state.pipeline_running.clone();
    if running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(api_error(StatusCode::CONFLICT, "pipeline already running"));
    }
    let _running_guard = RunningGuard(running);

    let resolved = std::sync::Arc::new(state.config.read().await.clone());
    let mut prepared = PreparedPipeline::prepare(
        state.store.clone(),
        resolved,
        &state.config_path,
        &options,
        Some(state.pipeline_tx.clone()),
    )
    .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;

    let run_id = prepared.run_id.clone();
    let _ = state.pipeline_tx.send(PipelineEvent::Started {
        run_id: run_id.clone(),
    });

    prepared
        .execute(&options)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("pipeline failed: {}", e)))?;

    Ok(run_id)
}

pub(crate) fn read_run_manifest(store: &FileStateStore, run_id: &str) -> Result<RunManifest, ApiError> {
    let key = crate::state::StorageKey::RunManifest { run_id };
    store
        .get_json::<RunManifest>(&key)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("run '{}' not found", run_id),
            )
        })
}

pub(crate) fn remove_run_artifacts(store: &FileStateStore, run_id: &str) -> Result<(), ApiError> {
    let run_dir = crate::state::StorageKey::RunDir { run_id }
        .to_state_path()
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid run id"))?
        .resolve(store.root());
    let report_dir = crate::state::StorageKey::RunReportDir { run_id }
        .to_state_path()
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid run id"))?
        .resolve(store.root());

    if run_dir.exists() {
        std::fs::remove_dir_all(&run_dir)
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    if report_dir.exists() {
        std::fs::remove_dir_all(&report_dir)
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    Ok(())
}
