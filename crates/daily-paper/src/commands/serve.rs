use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{Json, Redirect};
use axum::Router;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;

use chrono::Datelike;

use crate::cli::ServeArgs;
use crate::commands::run::{
    build_cli_overrides, classify_error, compute_date_window, find_source_run, generate_run_id,
    run_pipeline,
};
use daily_paper_core::config::{load_config, ResolvedConfig};
use daily_paper_core::models::run::*;
use daily_paper_core::state::path::StatePath;
use daily_paper_core::state::store::{FileStateStore, CACHE_KINDS};

// ── Shared state ─────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    store: Arc<FileStateStore>,
    config_path: PathBuf,
    config: Arc<RwLock<ResolvedConfig>>,
    pipeline_tx: broadcast::Sender<PipelineEvent>,
    log_buffer: LogBuffer,
    log_tx: broadcast::Sender<String>,
    pipeline_running: Arc<AtomicBool>,
}

pub(crate) type LogBuffer = Arc<Mutex<VecDeque<String>>>;

const MAX_LOG_LINES: usize = 10_000;

// ── API request/response types ───────────────────────────────────────

#[derive(Deserialize)]
struct RunRequest {
    stages: Option<Vec<String>>,
    from_run: Option<String>,
    date: Option<String>,
    dry_run: Option<bool>,
    force_zotero_sync: Option<bool>,
    force_rerank: Option<bool>,
    force_read: Option<bool>,
    force_send: Option<bool>,
    max_candidates: Option<usize>,
    no_email: Option<bool>,
    send_email: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ApiMessage {
    message: String,
}

#[derive(Serialize)]
struct RunStartResponse {
    run_id: String,
    message: String,
}

#[derive(Serialize)]
struct StatsDay {
    day: u32,
    total: usize,
    success: usize,
    failed: usize,
}

#[derive(Serialize)]
struct StatsResponse {
    year: i32,
    month: u32,
    days: Vec<StatsDay>,
}

#[derive(Serialize)]
struct RunSummary {
    run_id: String,
    status: String,
    started_at: String,
    finished_at: Option<String>,
    report_exists: bool,
    error: Option<ErrorSummary>,
}

#[derive(Serialize)]
struct ErrorSummary {
    kind: String,
    message: String,
}

#[derive(Serialize)]
struct DateResponse {
    date: String,
    runs: Vec<RunSummary>,
}

#[derive(Serialize)]
struct ConfigResponse {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct ConfigUpdateRequest {
    content: String,
}

#[derive(Serialize)]
struct CacheInfo {
    kind: String,
    size_bytes: u64,
    size_display: String,
    file_count: usize,
}

#[derive(Serialize)]
struct CacheResponse {
    caches: Vec<CacheInfo>,
}

#[derive(Serialize)]
struct CacheCleanResponse {
    message: String,
    kind: String,
    freed_bytes: u64,
}

#[derive(Deserialize)]
struct BulkDeleteRequest {
    run_ids: Vec<String>,
}

#[derive(Serialize)]
struct BulkDeleteResponse {
    message: String,
    count: usize,
}

type ApiError = (StatusCode, Json<ApiMessage>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ApiMessage {
            message: message.into(),
        }),
    )
}

// ── Entry point ──────────────────────────────────────────────────────

pub async fn execute(data_dir: &Path, args: ServeArgs) -> anyhow::Result<()> {
    let config_path = data_dir.join("config.toml");
    let (_raw, resolved) =
        load_config(&config_path).map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;

    let store = FileStateStore::new(data_dir.to_path_buf());
    store.ensure_dirs()?;

    let port = args.port.unwrap_or(resolved.web.port);
    let (pipeline_tx, _) = broadcast::channel(256);
    let log_buffer: LogBuffer = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES)));
    let (log_tx, _) = broadcast::channel::<String>(1024);

    // Install WebLogLayer so tracing events feed into log_buffer + log_tx.
    // This MUST run before any tracing macros; dispatch() does not call
    // init_tracing() for the Serve command, so no global subscriber is set.
    {
        use crate::log_layer::WebLogLayer;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        use tracing_subscriber::Layer;

        let web_layer = WebLogLayer::new(log_buffer.clone(), log_tx.clone(), MAX_LOG_LINES);
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            );

        tracing_subscriber::registry()
            .with(fmt_layer)
            .with(web_layer)
            .init();
    }

    let ui_path = data_dir.join("ui");

    let state = AppState {
        store: Arc::new(store),
        config_path: config_path.clone(),
        config: Arc::new(RwLock::new(resolved)),
        pipeline_tx,
        log_buffer,
        log_tx,
        pipeline_running: Arc::new(AtomicBool::new(false)),
    };

    let app = Router::new()
        // Run
        .route("/api/run", axum::routing::post(api_run_trigger))
        .route("/api/run/stream", axum::routing::get(api_run_stream))
        // Stats & date
        .route("/api/stats/{year}/{month}", axum::routing::get(api_stats))
        .route("/api/date/{date}", axum::routing::get(api_date_detail))
        .route("/api/date/{date}", axum::routing::delete(api_date_delete))
        // Run details
        .route("/api/run/{run_id}", axum::routing::get(api_run_detail))
        .route("/api/run/{run_id}", axum::routing::delete(api_run_delete))
        .route("/api/run/{run_id}/send", axum::routing::post(api_run_send))
        .route(
            "/api/runs/bulk",
            axum::routing::delete(api_runs_bulk_delete),
        )
        // Config
        .route("/api/config", axum::routing::get(api_config_get))
        .route("/api/config", axum::routing::put(api_config_put))
        .route("/api/config/reload", axum::routing::post(api_config_reload))
        // Cache
        .route("/api/cache", axum::routing::get(api_cache_info))
        .route("/api/cache/{kind}", axum::routing::delete(api_cache_clean))
        // Logs
        .route("/api/logs/stream", axum::routing::get(api_logs_stream))
        // Status
        .route("/api/status", axum::routing::get(api_status))
        // Static files & redirect
        .route(
            "/",
            axum::routing::get(|| async { Redirect::permanent("/ui/") }),
        )
        .nest_service("/ui", ServeDir::new(&ui_path))
        .nest_service("/report", ServeDir::new(data_dir.join("cache/reports")))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    info!(addr = %addr, "starting web server");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

// ── Run handlers ────────────────────────────────────────────────────

async fn api_run_trigger(
    State(state): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunStartResponse>, ApiError> {
    if state.pipeline_running.load(Ordering::SeqCst) {
        return Err(api_error(StatusCode::CONFLICT, "pipeline already running"));
    }

    let stages = req.stages.clone().unwrap_or_default();
    let stage_filter = parse_stage_filter(&stages)?;

    let run_args = crate::cli::RunArgs {
        date: req.date,
        stages,
        from_run: req.from_run,
        dry_run: req.dry_run.unwrap_or(true),
        send_email: req.send_email.unwrap_or(false),
        no_email: req.no_email.unwrap_or(true),
        force_zotero_sync: req.force_zotero_sync.unwrap_or(false),
        force_rerank: req.force_rerank.unwrap_or(false),
        force_read: req.force_read.unwrap_or(false),
        force_send: req.force_send.unwrap_or(false),
        max_candidates: req.max_candidates,
    };

    let source_manifest = if let Some(filter) = stage_filter.as_deref() {
        let source_run_id =
            find_source_run(&state.store, run_args.from_run.as_deref(), Some(filter))
                .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;
        let path = StatePath::new(format!("cache/runs/{}/manifest.json", source_run_id))
            .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;
        Some(
            state
                .store
                .read_json::<RunManifest>(&path)
                .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
                .ok_or_else(|| {
                    api_error(
                        StatusCode::NOT_FOUND,
                        format!("source run '{}' not found", source_run_id),
                    )
                })?,
        )
    } else {
        None
    };

    let run_id = generate_run_id();
    let date_window = compute_date_window(run_args.date.as_deref())
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;

    let mut manifest = RunManifest {
        run_id: run_id.clone(),
        parent_run_id: source_manifest.as_ref().map(|m| m.run_id.clone()),
        status: RunStatus::Running,
        started_at: chrono::Utc::now(),
        finished_at: None,
        config_hash: daily_paper_core::models::common::sha256_hex(
            state.config_path.to_string_lossy().as_bytes(),
        ),
        cli_overrides: build_cli_overrides(&run_args),
        date_window: date_window.clone(),
        stages: Vec::new(),
        warnings: Vec::new(),
        error: None,
    };

    let manifest_path = StatePath::new(format!("cache/runs/{}/manifest.json", run_id))
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let running = state.pipeline_running.clone();
    if running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(api_error(StatusCode::CONFLICT, "pipeline already running"));
    }

    state
        .store
        .write_json(&manifest_path, &manifest)
        .map_err(|e| {
            running.store(false, Ordering::SeqCst);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    let store = state.store.clone();
    let config = state.config.clone();
    let event_tx = state.pipeline_tx.clone();
    let run_id_for_spawn = run_id.clone();

    let _ = event_tx.send(PipelineEvent::Started {
        run_id: run_id.clone(),
    });

    tokio::spawn(async move {
        let resolved = config.read().await.clone();
        let result =
            match store.acquire_lock(&run_id_for_spawn, std::time::Duration::from_secs(3600)) {
                Ok(_lock) => {
                    run_pipeline(
                        &store,
                        &resolved,
                        &mut manifest,
                        &run_args,
                        &run_id_for_spawn,
                        &date_window,
                        stage_filter.as_deref(),
                        source_manifest.as_ref(),
                        Some(&event_tx),
                        &manifest_path,
                    )
                    .await
                }
                Err(e) => Err(anyhow::anyhow!("failed to acquire run lock: {}", e)),
            };

        manifest.finished_at = Some(chrono::Utc::now());
        match &result {
            Ok(()) => {
                manifest.status = RunStatus::Succeeded;
                let _ = event_tx.send(PipelineEvent::Ended {
                    run_id: run_id_for_spawn.clone(),
                    status: RunStatus::Succeeded,
                });
            }
            Err(e) => {
                manifest.status = RunStatus::Failed;
                manifest.error = Some(ErrorRecord {
                    kind: classify_error(e),
                    message: e.to_string(),
                    retryable: false,
                    context: serde_json::Value::Null,
                    occurred_at: chrono::Utc::now(),
                });
                let _ = event_tx.send(PipelineEvent::Ended {
                    run_id: run_id_for_spawn.clone(),
                    status: RunStatus::Failed,
                });
            }
        }
        let _ = store.write_json(&manifest_path, &manifest);
        running.store(false, Ordering::SeqCst);
    });

    Ok(Json(RunStartResponse {
        run_id,
        message: "pipeline started".to_string(),
    }))
}

async fn api_run_stream(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.pipeline_tx.subscribe();
    let filter_run = params.get("run_id").cloned();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let should_send = match &event {
                        PipelineEvent::Started { run_id }
                        | PipelineEvent::StageStart { run_id, .. }
                        | PipelineEvent::StageEnd { run_id, .. }
                        | PipelineEvent::Progress { run_id, .. }
                        | PipelineEvent::Ended { run_id, .. } => {
                            filter_run.as_ref().is_none_or(|f| f == run_id)
                        }
                    };
                    if should_send {
                        if let Ok(data) = serde_json::to_string(&event) {
                            yield Ok(Event::default().event("pipeline").data(data));
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream)
}

// ── Stats & run handlers ─────────────────────────────────────────────

async fn api_stats(
    State(state): State<AppState>,
    AxumPath((year, month)): AxumPath<(i32, u32)>,
) -> Result<Json<StatsResponse>, ApiError> {
    let Some(days_in_month) = days_in_month(year, month) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            format!("invalid month '{}', expected 1-12", month),
        ));
    };

    let runs = collect_runs(&state.store);
    let mut day_counts: std::collections::HashMap<u32, (usize, usize)> =
        std::collections::HashMap::new();

    for run in &runs {
        let date = &run.date_window.label;
        if let Ok(nd) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            if nd.year() == year && nd.month() == month {
                let entry = day_counts.entry(nd.day()).or_insert((0, 0));
                entry.0 += 1;
                if run.status == RunStatus::Succeeded {
                    entry.1 += 1;
                }
            }
        }
    }

    let days = (1..=days_in_month)
        .map(|day| {
            let (total, success) = day_counts.get(&day).copied().unwrap_or((0, 0));
            StatsDay {
                day,
                total,
                success,
                failed: total - success,
            }
        })
        .collect();

    Ok(Json(StatsResponse { year, month, days }))
}

async fn api_date_detail(
    State(state): State<AppState>,
    AxumPath(date): AxumPath<String>,
) -> Json<DateResponse> {
    let runs = collect_runs(&state.store);
    let matching: Vec<RunSummary> = runs
        .iter()
        .filter(|r| r.date_window.label == date)
        .map(|r| {
            let report_path = state
                .store
                .root()
                .join("cache/reports")
                .join(&r.run_id)
                .join("report.html");
            RunSummary {
                run_id: r.run_id.clone(),
                status: format!("{:?}", r.status).to_lowercase(),
                started_at: r.started_at.to_rfc3339(),
                finished_at: r.finished_at.map(|f| f.to_rfc3339()),
                report_exists: report_path.exists(),
                error: r.error.as_ref().map(|e| ErrorSummary {
                    kind: format!("{:?}", e.kind),
                    message: e.message.clone(),
                }),
            }
        })
        .collect();

    Json(DateResponse {
        date,
        runs: matching,
    })
}

async fn api_run_detail(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<RunManifest>, ApiError> {
    let path = StatePath::new(format!("cache/runs/{}/manifest.json", run_id))
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid run id"))?;
    state
        .store
        .read_json(&path)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "run not found"))
        .map(Json)
}

async fn api_run_delete(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<ApiMessage>, ApiError> {
    remove_run_artifacts(&state.store, &run_id)?;
    Ok(Json(ApiMessage {
        message: "deleted".to_string(),
    }))
}

async fn api_date_delete(
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

async fn api_runs_bulk_delete(
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

async fn api_run_send(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<ApiMessage>, ApiError> {
    if state
        .pipeline_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(api_error(StatusCode::CONFLICT, "pipeline already running"));
    }
    let _running_guard = RunningGuard(state.pipeline_running.clone());

    let source_manifest = read_run_manifest(&state.store, &run_id)?;
    let report_path = StatePath::new(format!("cache/reports/{}/report.html", run_id))
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;
    let report_exists = state
        .store
        .exists(&report_path)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !report_exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "no report found for this run",
        ));
    }

    let send_run_id = generate_run_id();
    let run_args = crate::cli::RunArgs {
        date: Some(source_manifest.date_window.label.clone()),
        stages: vec![StageName::Send.to_kebab().to_string()],
        from_run: Some(source_manifest.run_id.clone()),
        dry_run: false,
        send_email: true,
        no_email: false,
        force_zotero_sync: false,
        force_rerank: false,
        force_read: false,
        force_send: false,
        max_candidates: None,
    };
    let stage_filter = vec![StageName::Send];
    let mut manifest = RunManifest {
        run_id: send_run_id.clone(),
        parent_run_id: Some(source_manifest.run_id.clone()),
        status: RunStatus::Running,
        started_at: chrono::Utc::now(),
        finished_at: None,
        config_hash: daily_paper_core::models::common::sha256_hex(
            state.config_path.to_string_lossy().as_bytes(),
        ),
        cli_overrides: build_cli_overrides(&run_args),
        date_window: source_manifest.date_window.clone(),
        stages: Vec::new(),
        warnings: Vec::new(),
        error: None,
    };
    let manifest_path = StatePath::new(format!("cache/runs/{}/manifest.json", send_run_id))
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    state
        .store
        .write_json(&manifest_path, &manifest)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let _lock = state
        .store
        .acquire_lock(&send_run_id, std::time::Duration::from_secs(3600))
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to acquire run lock: {}", e),
            )
        })?;
    let _ = state.pipeline_tx.send(PipelineEvent::Started {
        run_id: send_run_id.clone(),
    });

    let resolved = state.config.read().await.clone();
    let result = run_pipeline(
        &state.store,
        &resolved,
        &mut manifest,
        &run_args,
        &send_run_id,
        &source_manifest.date_window,
        Some(&stage_filter),
        Some(&source_manifest),
        Some(&state.pipeline_tx),
        &manifest_path,
    )
    .await;

    manifest.finished_at = Some(chrono::Utc::now());
    match &result {
        Ok(()) => {
            manifest.status = RunStatus::Succeeded;
            let _ = state.pipeline_tx.send(PipelineEvent::Ended {
                run_id: send_run_id.clone(),
                status: RunStatus::Succeeded,
            });
        }
        Err(e) => {
            manifest.status = RunStatus::Failed;
            manifest.error = Some(ErrorRecord {
                kind: classify_error(e),
                message: e.to_string(),
                retryable: false,
                context: serde_json::Value::Null,
                occurred_at: chrono::Utc::now(),
            });
            let _ = state.pipeline_tx.send(PipelineEvent::Ended {
                run_id: send_run_id.clone(),
                status: RunStatus::Failed,
            });
        }
    }
    let _ = state.store.write_json(&manifest_path, &manifest);

    result.map_err(|e| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to send report: {}", e),
        )
    })?;

    Ok(Json(ApiMessage {
        message: format!("send completed in run {}", send_run_id),
    }))
}

// ── Config handlers ──────────────────────────────────────────────────

async fn api_config_get(State(state): State<AppState>) -> Json<ConfigResponse> {
    let content = std::fs::read_to_string(&state.config_path).unwrap_or_default();
    Json(ConfigResponse {
        path: state.config_path.to_string_lossy().to_string(),
        content,
    })
}

async fn api_config_put(
    State(state): State<AppState>,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<Json<ApiMessage>, (StatusCode, Json<ApiMessage>)> {
    // Validate TOML
    toml::from_str::<daily_paper_core::config::RawConfig>(&req.content).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiMessage {
                message: format!("invalid toml: {}", e),
            }),
        )
    })?;

    std::fs::write(&state.config_path, &req.content).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiMessage {
                message: e.to_string(),
            }),
        )
    })?;

    Ok(Json(ApiMessage {
        message: "saved".to_string(),
    }))
}

async fn api_config_reload(
    State(state): State<AppState>,
) -> Result<Json<ApiMessage>, (StatusCode, Json<ApiMessage>)> {
    let (_raw, resolved) = load_config(&state.config_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiMessage {
                message: format!("failed to reload config: {}", e),
            }),
        )
    })?;

    *state.config.write().await = resolved;

    Ok(Json(ApiMessage {
        message: "reloaded".to_string(),
    }))
}

// ── Cache handlers ───────────────────────────────────────────────────

async fn api_cache_info(State(state): State<AppState>) -> Json<CacheResponse> {
    let caches: Vec<CacheInfo> = CACHE_KINDS
        .iter()
        .map(|(kind, subdir, _)| {
            let dir = state.store.root().join(subdir);
            let (size, count) = dir_size_and_count(&dir);
            CacheInfo {
                kind: kind.to_string(),
                size_bytes: size,
                size_display: format_bytes(size),
                file_count: count,
            }
        })
        .collect();

    Json(CacheResponse { caches })
}

async fn api_cache_clean(
    State(state): State<AppState>,
    AxumPath(kind): AxumPath<String>,
) -> Result<Json<CacheCleanResponse>, ApiError> {
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

// ── Log stream ───────────────────────────────────────────────────────

async fn api_logs_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    // Send buffered history first
    let history: Vec<String> = state.log_buffer.lock().await.iter().cloned().collect();
    let mut rx = state.log_tx.subscribe();

    let stream = async_stream::stream! {
        for line in &history {
            yield Ok(Event::default().event("log").data(line.as_str()));
        }
        // Real-time push from log broadcast channel
        loop {
            match rx.recv().await {
                Ok(line) => {
                    yield Ok(Event::default().event("log").data(line.as_str()));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    yield Ok(Event::default().event("log").data(format!("... {} messages dropped ...", n)));
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream)
}

// ── Status ──────────────────────────────────────────────────────────

#[derive(Serialize)]
struct StatusResponse {
    pipeline_running: bool,
}

async fn api_status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        pipeline_running: state.pipeline_running.load(Ordering::SeqCst),
    })
}

// ── Helpers ──────────────────────────────────────────────────────────

fn collect_runs(store: &FileStateStore) -> Vec<RunManifest> {
    let runs_dir = store.root().join("cache/runs");
    if !runs_dir.exists() {
        return Vec::new();
    }

    let mut manifests = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&runs_dir) {
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                if let Ok(path) = StatePath::new(format!(
                    "cache/runs/{}/manifest.json",
                    entry.file_name().to_string_lossy()
                )) {
                    if let Ok(Some(m)) = store.read_json::<RunManifest>(&path) {
                        manifests.push(m);
                    }
                }
            }
        }
    }
    manifests.sort_by_key(|m| std::cmp::Reverse(m.started_at));
    manifests
}

fn dir_size_and_count(path: &Path) -> (u64, usize) {
    if !path.exists() {
        return (0, 0);
    }
    let mut size = 0u64;
    let mut count = 0usize;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let (s, c) = dir_size_and_count(&p);
                size += s;
                count += c;
            } else if let Ok(meta) = p.metadata() {
                size += meta.len();
                count += 1;
            }
        }
    }
    (size, count)
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn parse_stage_filter(stages: &[String]) -> Result<Option<Vec<StageName>>, ApiError> {
    if stages.is_empty() {
        return Ok(None);
    }

    let mut parsed = Vec::with_capacity(stages.len());
    for stage in stages {
        let Some(name) = StageName::from_kebab(stage) else {
            let valid = StageName::all()
                .iter()
                .map(StageName::to_kebab)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("unknown stage '{}'. Valid: {}", stage, valid),
            ));
        };
        parsed.push(name);
    }
    Ok(Some(parsed))
}

fn read_run_manifest(store: &FileStateStore, run_id: &str) -> Result<RunManifest, ApiError> {
    let path = StatePath::new(format!("cache/runs/{}/manifest.json", run_id))
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;
    store
        .read_json(&path)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, format!("run '{}' not found", run_id)))
}

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    if !(1..=12).contains(&month) {
        return None;
    }
    let next_month = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }?;
    Some(next_month.pred_opt()?.day())
}

fn remove_run_artifacts(store: &FileStateStore, run_id: &str) -> Result<(), ApiError> {
    let run_dir = StatePath::new(format!("cache/runs/{}", run_id))
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid run id"))?
        .resolve(store.root());
    let report_dir = StatePath::new(format!("cache/reports/{}", run_id))
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

struct RunningGuard(Arc<AtomicBool>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn dt(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn manifest(run_id: &str, started_at: DateTime<Utc>, stages: Vec<StageName>) -> RunManifest {
        RunManifest {
            run_id: run_id.to_string(),
            parent_run_id: None,
            status: RunStatus::Succeeded,
            started_at,
            finished_at: Some(started_at),
            config_hash: "test".to_string(),
            cli_overrides: Vec::new(),
            date_window: daily_paper_core::models::common::DateWindow {
                start: dt("2026-06-06T00:00:00Z"),
                end: dt("2026-06-07T00:00:00Z"),
                label: "2026-06-06".to_string(),
            },
            stages: stages
                .into_iter()
                .map(|stage| StageRecord {
                    stage,
                    status: StageStatus::Succeeded,
                    started_at,
                    finished_at: Some(started_at),
                    cache_hit: false,
                    input_hash: None,
                    output_ref: None,
                    error: None,
                })
                .collect(),
            warnings: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn parse_stage_filter_accepts_empty_and_known_stages() {
        assert!(parse_stage_filter(&[]).unwrap().is_none());

        let stages = vec!["source-fetch".to_string(), "render".to_string()];
        let parsed = parse_stage_filter(&stages).unwrap().unwrap();
        assert_eq!(parsed, vec![StageName::SourceFetch, StageName::Render]);
    }

    #[test]
    fn parse_stage_filter_rejects_unknown_stage() {
        let stages = vec!["unknown".to_string()];
        let err = parse_stage_filter(&stages).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.message.contains("unknown stage"));
    }

    #[test]
    fn days_in_month_handles_boundaries() {
        assert_eq!(days_in_month(2026, 2), Some(28));
        assert_eq!(days_in_month(2024, 2), Some(29));
        assert_eq!(days_in_month(2026, 12), Some(31));
        assert_eq!(days_in_month(2026, 0), None);
        assert_eq!(days_in_month(2026, 13), None);
    }

    #[test]
    fn remove_run_artifacts_rejects_path_traversal() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = FileStateStore::new(tmp.path().to_path_buf());
        let (status, _) = remove_run_artifacts(&store, "../outside").unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn collect_runs_sorts_by_started_at_descending() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = FileStateStore::new(tmp.path().to_path_buf());
        store.ensure_dirs().unwrap();

        let old = manifest("old", dt("2026-06-06T01:00:00Z"), Vec::new());
        let new = manifest("new", dt("2026-06-06T02:00:00Z"), Vec::new());
        store
            .write_json(
                &StatePath::new("cache/runs/old/manifest.json").unwrap(),
                &old,
            )
            .unwrap();
        store
            .write_json(
                &StatePath::new("cache/runs/new/manifest.json").unwrap(),
                &new,
            )
            .unwrap();

        let runs = collect_runs(&store);
        assert_eq!(runs[0].run_id, "new");
        assert_eq!(runs[1].run_id, "old");
    }

    #[test]
    fn find_source_run_selects_latest_run_with_required_stages() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = FileStateStore::new(tmp.path().to_path_buf());
        store.ensure_dirs().unwrap();

        let incomplete = manifest(
            "incomplete",
            dt("2026-06-06T01:00:00Z"),
            vec![StageName::ZoteroSync],
        );
        store
            .write_json(
                &StatePath::new("cache/runs/incomplete/manifest.json").unwrap(),
                &incomplete,
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let complete = manifest(
            "complete",
            dt("2026-06-06T02:00:00Z"),
            StageName::Render.required_stages(),
        );
        store
            .write_json(
                &StatePath::new("cache/runs/complete/manifest.json").unwrap(),
                &complete,
            )
            .unwrap();

        let source = find_source_run(&store, None, Some(&[StageName::Render])).unwrap();
        assert_eq!(source, "complete");
    }
}
