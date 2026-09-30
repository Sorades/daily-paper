use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::Json;
use chrono::Datelike;
use futures_util::stream::Stream;
use serde::Deserialize;
use tracing::info;

use crate::models::run::*;
use crate::state::store::CACHE_KINDS;

use super::runner::{read_run_manifest, remove_run_artifacts, run_pipeline_sync_wait, spawn_pipeline};
use super::state::AppState;
use super::storage_utils::{collect_runs, days_in_month, dir_size_and_count, format_bytes};
use super::types::*;

pub(crate) async fn api_run_trigger(
    State(state): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunStartResponse>, ApiError> {
    info!(req = ?req, "received api_run_trigger request");
    let run_id = spawn_pipeline(&state, &req).await?;
    info!(run_id = %run_id, "api_run_trigger spawned successfully");
    Ok(Json(RunStartResponse {
        run_id,
        message: "pipeline started".to_string(),
    }))
}

pub(crate) async fn api_run_cancel(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut cancel_lock = state.cancel_tx.lock().await;
    if let Some(tx) = cancel_lock.take() {
        let _ = tx.send(true);
        info!("pipeline execution cancellation requested");
        Ok(Json(serde_json::json!({
            "status": "cancelled",
            "message": "cancellation requested"
        })))
    } else {
        Ok(Json(serde_json::json!({
            "status": "not_running",
            "message": "no pipeline currently running"
        })))
    }
}

#[derive(Deserialize)]
pub(crate) struct RunStreamParams {
    run_id: Option<String>,
}

pub(crate) async fn api_run_stream(
    State(state): State<AppState>,
    Query(params): Query<RunStreamParams>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.pipeline_tx.subscribe();
    let filter_run_id = params.run_id;

    let stream = async_stream::stream! {
        while let Ok(event) = rx.recv().await {
            if let Some(ref target_id) = filter_run_id {
                let matches = match &event {
                    PipelineEvent::Started { run_id } => run_id == target_id,
                    PipelineEvent::StageStart { run_id, .. } => run_id == target_id,
                    PipelineEvent::StageEnd { run_id, .. } => run_id == target_id,
                    PipelineEvent::Progress { run_id, .. } => run_id == target_id,
                    PipelineEvent::Ended { run_id, .. } => run_id == target_id,
                };
                if !matches {
                    continue;
                }
            }
            if let Ok(data) = serde_json::to_string(&event) {
                yield Ok(Event::default().event("pipeline").data(data));
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

pub(crate) async fn api_stats(
    State(state): State<AppState>,
    AxumPath((year, month)): AxumPath<(i32, u32)>,
) -> Result<Json<StatsResponse>, ApiError> {
    if !(1..=12).contains(&month) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            format!("invalid month: {}", month),
        ));
    }

    let runs = collect_runs(&state.store);
    let num_days = days_in_month(year, month).unwrap_or(31);

    let mut days: Vec<StatsDay> = (1..=num_days)
        .map(|d| StatsDay {
            day: d,
            total: 0,
            success: 0,
            failed: 0,
        })
        .collect();

    for run in &runs {
        if run.started_at.year() == year && run.started_at.month() == month {
            let day_idx = (run.started_at.day() - 1) as usize;
            if day_idx < days.len() {
                days[day_idx].total += 1;
                match run.status {
                    RunStatus::Succeeded => days[day_idx].success += 1,
                    RunStatus::Failed => days[day_idx].failed += 1,
                    _ => {}
                }
            }
        }
    }

    Ok(Json(StatsResponse { year, month, days }))
}

pub(crate) async fn api_date_detail(
    State(state): State<AppState>,
    AxumPath(date): AxumPath<String>,
) -> Result<Json<DateResponse>, ApiError> {
    let runs = collect_runs(&state.store);
    let matching: Vec<RunSummary> = runs
        .into_iter()
        .filter(|r| r.date_window.label == date)
        .map(|r| {
            let report_key = crate::state::StorageKey::RunReportHtml { run_id: &r.run_id };
            let report_exists = state.store.key_exists(&report_key).unwrap_or(false);
            RunSummary {
                run_id: r.run_id,
                status: match r.status {
                    RunStatus::Running => "running".to_string(),
                    RunStatus::Succeeded => "succeeded".to_string(),
                    RunStatus::Failed => "failed".to_string(),
                    RunStatus::Blocked => "blocked".to_string(),
                    RunStatus::Cancelled => "cancelled".to_string(),
                },
                started_at: r.started_at.to_rfc3339(),
                finished_at: r.finished_at.map(|t| t.to_rfc3339()),
                report_exists,
                error: r.error.map(|e| ErrorSummary {
                    kind: format!("{:?}", e.kind),
                    message: e.message,
                }),
            }
        })
        .collect();

    Ok(Json(DateResponse {
        date,
        runs: matching,
    }))
}

pub(crate) async fn api_run_detail(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<RunManifest>, ApiError> {
    let manifest = read_run_manifest(&state.store, &run_id)?;
    Ok(Json(manifest))
}

pub(crate) async fn api_run_delete(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<ApiMessage>, ApiError> {
    remove_run_artifacts(&state.store, &run_id)?;
    Ok(Json(ApiMessage {
        message: "deleted".to_string(),
    }))
}

pub(crate) async fn api_date_delete(
    State(state): State<AppState>,
    AxumPath(date): AxumPath<String>,
) -> Result<Json<BulkDeleteResponse>, ApiError> {
    let runs = collect_runs(&state.store);
    let to_delete: Vec<String> = runs
        .iter()
        .filter(|r| r.date_window.label == date)
        .map(|r| r.run_id.clone())
        .collect();

    let count = to_delete.len();
    for run_id in &to_delete {
        remove_run_artifacts(&state.store, run_id)?;
    }

    Ok(Json(BulkDeleteResponse {
        message: "deleted".to_string(),
        count,
    }))
}

pub(crate) async fn api_runs_bulk_delete(
    State(state): State<AppState>,
    Json(req): Json<BulkDeleteRequest>,
) -> Result<Json<BulkDeleteResponse>, ApiError> {
    let count = req.run_ids.len();
    for run_id in &req.run_ids {
        remove_run_artifacts(&state.store, run_id)?;
    }
    Ok(Json(BulkDeleteResponse {
        message: "deleted".to_string(),
        count,
    }))
}

pub(crate) async fn api_run_send(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<ApiMessage>, ApiError> {
    let source_manifest = read_run_manifest(&state.store, &run_id)?;
    let report_key = crate::state::StorageKey::RunReportHtml { run_id: &run_id };
    let report_exists = state
        .store
        .key_exists(&report_key)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !report_exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "no report found for this run",
        ));
    }

    let req = RunRequest {
        date: Some(source_manifest.date_window.label),
        stages: Some(vec![StageName::Send.to_kebab().to_string()]),
        from_run: Some(source_manifest.run_id),
        dry_run: Some(false),
        send_email: Some(true),
        no_email: Some(false),
        force_zotero_sync: Some(false),
        force_embedding: Some(false),
        force_rerank: Some(false),
        force_read: Some(false),
        force_send: Some(false),
        max_candidates: None,
    };

    let send_run_id = run_pipeline_sync_wait(&state, &req).await?;

    Ok(Json(ApiMessage {
        message: format!("send completed in run {}", send_run_id),
    }))
}

pub(crate) async fn api_config_get(State(state): State<AppState>) -> Json<ConfigResponse> {
    let content = std::fs::read_to_string(&state.config_path).unwrap_or_default();
    Json(ConfigResponse {
        path: state.config_path.to_string_lossy().to_string(),
        content,
    })
}

pub(crate) async fn api_config_put(
    State(state): State<AppState>,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<Json<ApiMessage>, ApiError> {
    crate::config::validate_config_str(&req.content)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;

    let (_raw, resolved) = crate::config::parse_config_str(&req.content)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;

    std::fs::write(&state.config_path, &req.content).map_err(|e| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to write config: {}", e),
        )
    })?;

    *state.config.write().await = resolved;
    info!("config updated via API");

    Ok(Json(ApiMessage {
        message: "config updated and reloaded".to_string(),
    }))
}

pub(crate) async fn api_config_reload(
    State(state): State<AppState>,
) -> Result<Json<ApiMessage>, ApiError> {
    let (_raw, resolved) = crate::config::load_config(&state.config_path)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    *state.config.write().await = resolved;
    info!("config reloaded from disk via API");

    Ok(Json(ApiMessage {
        message: "config reloaded".to_string(),
    }))
}

pub(crate) async fn api_cache_info(State(state): State<AppState>) -> Json<CacheResponse> {
    let mut caches = Vec::new();
    for &(name, subdir, _) in CACHE_KINDS {
        let dir = state.store.root().join(subdir);
        let (size_bytes, file_count) = dir_size_and_count(&dir);
        caches.push(CacheInfo {
            kind: name.to_string(),
            size_bytes,
            size_display: format_bytes(size_bytes),
            file_count,
        });
    }
    Json(CacheResponse { caches })
}

pub(crate) async fn api_cache_clean(
    State(state): State<AppState>,
    AxumPath(kind): AxumPath<String>,
) -> Result<Json<CacheCleanResponse>, ApiError> {
    if state.pipeline_running.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "cannot clean cache while pipeline is running",
        ));
    }

    if kind == "all" {
        let mut freed_bytes = 0;
        for (_, subdir, _) in CACHE_KINDS {
            let dir = state.store.root().join(subdir);
            let (size, _) = dir_size_and_count(&dir);
            freed_bytes += size;
            if dir.exists() {
                std::fs::remove_dir_all(&dir)
                    .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
        }
        state
            .store
            .ensure_dirs()
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        return Ok(Json(CacheCleanResponse {
            message: "cleaned".to_string(),
            kind,
            freed_bytes,
        }));
    }

    let Some((_, subdir, _)) = CACHE_KINDS.iter().find(|(name, _, _)| *name == kind) else {
        let valid = CACHE_KINDS
            .iter()
            .map(|(name, _, _)| *name)
            .chain(std::iter::once("all"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            format!("invalid cache kind '{}'; valid kinds: {}", kind, valid),
        ));
    };

    let dir = state.store.root().join(subdir);
    let (size_before, _) = dir_size_and_count(&dir);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CacheCleanResponse {
        message: "cleaned".to_string(),
        kind,
        freed_bytes: size_before,
    }))
}

pub(crate) async fn api_logs_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.log_tx.subscribe();
    let initial_lines: Vec<String> = {
        let buf = state.log_buffer.lock().await;
        buf.iter().cloned().collect()
    };

    let stream = async_stream::stream! {
        for line in initial_lines {
            yield Ok(Event::default().event("log").data(line));
        }
        while let Ok(line) = rx.recv().await {
            yield Ok(Event::default().event("log").data(line));
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

pub(crate) async fn api_status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok".to_string(),
        pipeline_running: state.pipeline_running.load(std::sync::atomic::Ordering::SeqCst),
    })
}
