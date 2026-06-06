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
use crate::commands::run::run_pipeline;
use daily_paper_core::config::{load_config, ResolvedConfig};
use daily_paper_core::models::common::DateWindow;
use daily_paper_core::models::run::*;
use daily_paper_core::state::path::StatePath;
use daily_paper_core::state::store::FileStateStore;

// ── Shared state ─────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    store: Arc<FileStateStore>,
    config_path: PathBuf,
    config: Arc<RwLock<ResolvedConfig>>,
    pipeline_tx: broadcast::Sender<PipelineEvent>,
    log_buffer: LogBuffer,
    pipeline_running: Arc<AtomicBool>,
}

type LogBuffer = Arc<Mutex<VecDeque<String>>>;

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

#[derive(Serialize)]
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

    let ui_path = data_dir.join("ui");

    let state = AppState {
        store: Arc::new(store),
        config_path: config_path.clone(),
        config: Arc::new(RwLock::new(resolved)),
        pipeline_tx,
        log_buffer,
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
) -> Result<Json<RunStartResponse>, (StatusCode, Json<ApiMessage>)> {
    if state.pipeline_running.load(Ordering::SeqCst) {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiMessage {
                message: "pipeline already running".to_string(),
            }),
        ));
    }

    // Parse stage filter
    let stage_filter: Option<Vec<StageName>> = req.stages.as_ref().map(|stages| {
        stages
            .iter()
            .filter_map(|s| StageName::from_kebab(s))
            .collect()
    });

    // Load source manifest if --from-run specified
    let source_manifest = if let Some(ref from_run) = req.from_run {
        let path =
            StatePath::new(format!("cache/runs/{}/manifest.json", from_run)).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiMessage {
                        message: e.to_string(),
                    }),
                )
            })?;
        state.store.read_json::<RunManifest>(&path).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiMessage {
                    message: e.to_string(),
                }),
            )
        })?
    } else {
        None
    };

    // Build RunArgs from request
    let run_args = crate::cli::RunArgs {
        date: req.date,
        stages: req.stages.unwrap_or_default(),
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

    let run_id = generate_run_id();
    let date_window = compute_date_window(run_args.date.as_deref()).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiMessage {
                message: e.to_string(),
            }),
        )
    })?;

    let mut manifest = RunManifest {
        run_id: run_id.clone(),
        parent_run_id: source_manifest.as_ref().map(|m| m.run_id.clone()),
        status: RunStatus::Running,
        started_at: chrono::Utc::now(),
        finished_at: None,
        config_hash: daily_paper_core::models::common::sha256_hex(
            state.config_path.to_string_lossy().as_bytes(),
        ),
        cli_overrides: Vec::new(),
        date_window: date_window.clone(),
        stages: Vec::new(),
        warnings: Vec::new(),
        error: None,
    };

    let manifest_path =
        StatePath::new(format!("cache/runs/{}/manifest.json", run_id)).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiMessage {
                    message: e.to_string(),
                }),
            )
        })?;

    state
        .store
        .write_json(&manifest_path, &manifest)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiMessage {
                    message: e.to_string(),
                }),
            )
        })?;

    let store = state.store.clone();
    let config = state.config.clone();
    let event_tx = state.pipeline_tx.clone();
    let running = state.pipeline_running.clone();
    let run_id_for_spawn = run_id.clone();

    running.store(true, Ordering::SeqCst);

    tokio::spawn(async move {
        let resolved = config.read().await.clone();
        let result = run_pipeline(
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
        .await;

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
) -> Json<StatsResponse> {
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

    let days_in_month = chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
        .unwrap_or(chrono::NaiveDate::from_ymd_opt(year, 12, 31).unwrap())
        .pred_opt()
        .unwrap()
        .day();

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

    Json(StatsResponse { year, month, days })
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
) -> Result<Json<RunManifest>, StatusCode> {
    let path = StatePath::new(format!("cache/runs/{}/manifest.json", run_id))
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    state
        .store
        .read_json(&path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}

async fn api_run_delete(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<ApiMessage>, StatusCode> {
    let run_dir = state.store.root().join("cache/runs").join(&run_id);
    let report_dir = state.store.root().join("cache/reports").join(&run_id);
    if run_dir.exists() {
        std::fs::remove_dir_all(&run_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if report_dir.exists() {
        std::fs::remove_dir_all(&report_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(Json(ApiMessage {
        message: "deleted".to_string(),
    }))
}

async fn api_date_delete(
    State(state): State<AppState>,
    AxumPath(date): AxumPath<String>,
) -> Result<Json<BulkDeleteResponse>, StatusCode> {
    let runs = collect_runs(&state.store);
    let to_delete: Vec<String> = runs
        .iter()
        .filter(|r| r.date_window.label == date)
        .map(|r| r.run_id.clone())
        .collect();

    let count = to_delete.len();
    for run_id in &to_delete {
        let run_dir = state.store.root().join("cache/runs").join(run_id);
        let report_dir = state.store.root().join("cache/reports").join(run_id);
        let _ = std::fs::remove_dir_all(run_dir);
        let _ = std::fs::remove_dir_all(report_dir);
    }

    Ok(Json(BulkDeleteResponse {
        message: "deleted".to_string(),
        count,
    }))
}

async fn api_runs_bulk_delete(
    State(state): State<AppState>,
    Json(req): Json<BulkDeleteRequest>,
) -> Result<Json<BulkDeleteResponse>, StatusCode> {
    let count = req.run_ids.len();
    for run_id in &req.run_ids {
        let run_dir = state.store.root().join("cache/runs").join(run_id);
        let report_dir = state.store.root().join("cache/reports").join(run_id);
        let _ = std::fs::remove_dir_all(run_dir);
        let _ = std::fs::remove_dir_all(report_dir);
    }
    Ok(Json(BulkDeleteResponse {
        message: "deleted".to_string(),
        count,
    }))
}

async fn api_run_send(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<ApiMessage>, (StatusCode, Json<ApiMessage>)> {
    let report_path = state
        .store
        .root()
        .join("cache/reports")
        .join(&run_id)
        .join("report.html");
    if !report_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiMessage {
                message: "no report found for this run".to_string(),
            }),
        ));
    }

    // TODO: implement actual email sending via stage_send
    Ok(Json(ApiMessage {
        message: "send not yet implemented".to_string(),
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
    let kinds = [
        "zotero",
        "arxiv",
        "embeddings",
        "rerank",
        "papers",
        "models",
    ];
    let caches: Vec<CacheInfo> = kinds
        .iter()
        .map(|kind| {
            let dir = state.store.root().join("cache").join(kind);
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
) -> Result<Json<CacheCleanResponse>, (StatusCode, Json<ApiMessage>)> {
    let dir = state.store.root().join("cache").join(&kind);
    if !dir.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiMessage {
                message: format!("cache directory not found: {}", kind),
            }),
        ));
    }

    let (size_before, _) = dir_size_and_count(&dir);
    std::fs::remove_dir_all(&dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiMessage {
                message: e.to_string(),
            }),
        )
    })?;
    std::fs::create_dir_all(&dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiMessage {
                message: e.to_string(),
            }),
        )
    })?;

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

    let stream = async_stream::stream! {
        for line in &history {
            yield Ok(Event::default().event("log").data(line.as_str()));
        }
        // Then keep connection open (no new events via broadcast for now)
        // The log buffer is populated by a tracing layer that can be added later
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            yield Ok(Event::default().event("ping").data(""));
        }
    };

    Sse::new(stream)
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
                if let Ok(Some(m)) = store.read_json::<RunManifest>(
                    &StatePath::new(format!(
                        "cache/runs/{}/manifest.json",
                        entry.file_name().to_string_lossy()
                    ))
                    .unwrap(),
                ) {
                    manifests.push(m);
                }
            }
        }
    }
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

// Re-exports from run module for API use
fn generate_run_id() -> String {
    let now = chrono::Local::now();
    let ts = now.format("%Y%m%d-%H%M%S").to_string();
    let suffix: String = (0..6)
        .map(|_| format!("{:x}", fastrand::u8(0..16)))
        .collect();
    format!("{}-{}", ts, suffix)
}

fn compute_date_window(date_arg: Option<&str>) -> anyhow::Result<DateWindow> {
    use chrono::{Duration, Local, NaiveDate, TimeZone, Utc};

    let today = Local::now().date_naive();

    let target_date = if let Some(d) = date_arg {
        NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("invalid date format '{}', expected YYYY-MM-DD", d))?
    } else {
        today
    };

    let start = Local
        .from_local_datetime(&target_date.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .ok_or_else(|| anyhow::anyhow!("ambiguous local time"))?
        .with_timezone(&Utc);

    let end = Local
        .from_local_datetime(
            &(target_date + Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
        .single()
        .ok_or_else(|| anyhow::anyhow!("ambiguous local time"))?
        .with_timezone(&Utc);

    Ok(DateWindow {
        start,
        end,
        label: target_date.to_string(),
    })
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
