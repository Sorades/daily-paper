pub(crate) mod handlers;
pub(crate) mod runner;
pub(crate) mod scheduler;
pub(crate) mod state;
pub(crate) mod storage_utils;
pub(crate) mod types;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::response::Redirect;
use axum::Router;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;

use crate::config::load_config;
use crate::models::run::*;
use crate::state::store::FileStateStore;

use self::handlers::*;
use self::scheduler::scheduler_loop;
use self::state::{AppState, MAX_LOG_LINES};
use self::storage_utils::reconcile_orphan_runs;

pub(crate) use self::state::LogBuffer;

pub async fn execute(data_dir: &Path, port: Option<u16>, no_schedule: bool) -> anyhow::Result<()> {
    let config_path = data_dir.join("config.toml");
    let (_raw, resolved) = load_config(&config_path)?;
    let port = port.unwrap_or(resolved.web.port);

    let store = FileStateStore::new(data_dir.to_path_buf());
    store.ensure_dirs()?;

    let (pipeline_tx, _) = broadcast::channel::<PipelineEvent>(256);
    let (log_tx, _) = broadcast::channel::<String>(1024);
    let log_buffer = Arc::new(tokio::sync::Mutex::new(std::collections::VecDeque::new()));

    // Install WebLogLayer so tracing events feed into log_buffer + log_tx.
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

    let ui_path = {
        let p = Path::new(&resolved.web.ui_path);
        let candidate_in_data = data_dir.join(p);
        let candidate_in_workspace = std::env::current_dir().unwrap_or_default().join("ui/dist");
        if p.is_absolute() && p.exists() {
            p.to_path_buf()
        } else if candidate_in_data.exists() {
            candidate_in_data
        } else if candidate_in_workspace.exists() {
            candidate_in_workspace
        } else {
            candidate_in_data
        }
    };

    // Reconcile any orphan "Running" runs from previous server process crash / kill
    reconcile_orphan_runs(&store);

    let schedule_enabled = resolved.schedule.enabled && !no_schedule;
    let schedule_hour = resolved.schedule.hour();
    let schedule_minute = resolved.schedule.minute();
    let schedule_tz = resolved.schedule.timezone.clone();

    let state = AppState {
        store: Arc::new(store),
        config_path: config_path.clone(),
        config: Arc::new(RwLock::new(resolved)),
        pipeline_tx,
        log_buffer,
        log_tx,
        pipeline_running: Arc::new(AtomicBool::new(false)),
        cancel_tx: Arc::new(tokio::sync::Mutex::new(None)),
    };

    let app = Router::new()
        // Run
        .route("/api/run", axum::routing::post(api_run_trigger))
        .route("/api/run/stream", axum::routing::get(api_run_stream))
        .route("/api/run/cancel", axum::routing::post(api_run_cancel))
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
        .with_state(state.clone());

    let addr = format!("0.0.0.0:{}", port);
    info!(addr = %addr, "starting web server");

    if schedule_enabled {
        let scheduler_state = state;
        tokio::spawn(async move {
            scheduler_loop(scheduler_state, schedule_hour, schedule_minute, schedule_tz.as_deref()).await;
        });
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::runner::*;
    use super::storage_utils::*;
    use chrono::{DateTime, Utc};
    use crate::config::load_config;
    use crate::models::run::*;
    use crate::state::keys::StorageKey;
    use crate::state::store::FileStateStore;

    fn dt(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn manifest(run_id: &str, started_at: DateTime<Utc>, stages: Vec<StageName>) -> RunManifest {
        RunManifest {
            run_id: run_id.to_string(),
            parent_run_id: None,
            status: RunStatus::Succeeded,
            started_at,
            finished_at: Some(started_at + chrono::Duration::seconds(60)),
            config_hash: "test".to_string(),
            cli_overrides: vec![],
            date_window: crate::models::common::DateWindow {
                start: started_at,
                end: started_at + chrono::Duration::days(1),
                label: started_at.format("%Y-%m-%d").to_string(),
            },
            stages: stages
                .into_iter()
                .map(|stage| StageRecord {
                    stage,
                    status: StageStatus::Succeeded,
                    started_at,
                    finished_at: Some(started_at + chrono::Duration::seconds(10)),
                    cache_hit: false,
                    input_hash: None,
                    output_ref: None,
                    error: None,
                })
                .collect(),
            warnings: vec![],
            error: None,
        }
    }

    #[test]
    fn days_in_month_handles_boundaries() {
        assert_eq!(days_in_month(2026, 1), Some(31));
        assert_eq!(days_in_month(2026, 2), Some(28));
        assert_eq!(days_in_month(2024, 2), Some(29));
        assert_eq!(days_in_month(2026, 4), Some(30));
        assert_eq!(days_in_month(2026, 12), Some(31));
        assert_eq!(days_in_month(2026, 13), None);
    }

    #[test]
    fn parse_stage_filter_accepts_empty_and_known_stages() {
        assert_eq!(parse_stage_filter(&[]).unwrap(), None);
        assert_eq!(
            parse_stage_filter(&["rerank".to_string(), "render".to_string()]).unwrap(),
            Some(vec![StageName::Rerank, StageName::Render])
        );
    }

    #[test]
    fn parse_stage_filter_rejects_unknown_stage() {
        let err = parse_stage_filter(&["invalid-stage".to_string()]).unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn remove_run_artifacts_rejects_path_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let store = FileStateStore::new(temp.path().to_path_buf());
        let err = remove_run_artifacts(&store, "../escape").unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn collect_runs_sorts_by_started_at_descending() {
        let temp = tempfile::tempdir().unwrap();
        let store = FileStateStore::new(temp.path().to_path_buf());
        store.ensure_dirs().unwrap();

        let older = manifest("old", dt("2026-03-01T10:00:00Z"), vec![]);
        let newer = manifest("new", dt("2026-03-02T10:00:00Z"), vec![]);

        store
            .put_json(
                &StorageKey::RunManifest { run_id: "old" },
                &older,
            )
            .unwrap();
        store
            .put_json(
                &StorageKey::RunManifest { run_id: "new" },
                &newer,
            )
            .unwrap();

        let runs = collect_runs(&store);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].run_id, "new");
        assert_eq!(runs[1].run_id, "old");
    }

    #[test]
    fn find_source_run_selects_latest_run_with_required_stages() {
        let temp = tempfile::tempdir().unwrap();
        let store = FileStateStore::new(temp.path().to_path_buf());
        store.ensure_dirs().unwrap();

        let incomplete = manifest(
            "incomplete",
            dt("2026-03-02T10:00:00Z"),
            vec![StageName::ZoteroSync],
        );
        let complete = manifest(
            "complete",
            dt("2026-03-01T10:00:00Z"),
            vec![
                StageName::ZoteroSync,
                StageName::SourceFetch,
                StageName::Embedding,
            ],
        );

        store
            .put_json(
                &StorageKey::RunManifest { run_id: "incomplete" },
                &incomplete,
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store
            .put_json(
                &StorageKey::RunManifest { run_id: "complete" },
                &complete,
            )
            .unwrap();

        let found = crate::commands::run::cache_loader::find_source_run(
            &store,
            None,
            Some(&[StageName::Rerank]),
        )
        .unwrap();
        assert_eq!(found, "complete");
    }

    #[tokio::test]
    async fn prepared_pipeline_cancel_marks_manifest_cancelled() {
        let temp = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(FileStateStore::new(temp.path().to_path_buf()));
        store.ensure_dirs().unwrap();

        let config_str = r#"
[zotero]
user_id = "12345"
api_key = "secret"
max_snapshot_age_hours = 24

[[sources]]
kind = "arxiv"
categories = ["cs.AI"]

[embedding]
kind = "openai-compatible"
base_url = "https://api.example.com/v1"
api_key = "secret"
model = "text-embedding-3-small"

[reader]
kind = "openai"
base_url = "https://api.example.com/v1"
api_key = "secret"
model = "gpt-4o"

[email]
smtp_server = "smtp.example.com"
smtp_port = 587
sender = "bot@example.com"
receiver = "user@example.com"
password = "pwd"
"#;
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, config_str).unwrap();

        let (_raw, resolved) = load_config(&config_path).unwrap();
        let config = std::sync::Arc::new(resolved);
        let options = crate::pipeline::PipelineOptions::default();

        let mut prepared = crate::commands::run::PreparedPipeline::prepare(
            store.clone(),
            config,
            &config_path,
            &options,
            None,
        )
        .unwrap();

        assert_eq!(prepared.manifest.status, RunStatus::Running);
        prepared.cancel().unwrap();

        assert_eq!(prepared.manifest.status, RunStatus::Cancelled);
        let key = StorageKey::RunManifest {
            run_id: &prepared.run_id,
        };
        let reloaded: RunManifest = store.get_json(&key).unwrap().unwrap();
        assert_eq!(reloaded.status, RunStatus::Cancelled);
    }
}
