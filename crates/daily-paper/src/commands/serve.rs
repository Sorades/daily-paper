use std::path::Path;

use axum::Router;
use axum::extract::State;
use axum::response::Html;
use tracing::info;

use crate::cli::ServeArgs;

pub async fn execute(state_dir: &Path, args: ServeArgs) -> anyhow::Result<()> {
    let reports_dir = state_dir.join("reports");

    if !reports_dir.exists() {
        anyhow::bail!("reports directory not found: {}", reports_dir.display());
    }

    let addr = format!("0.0.0.0:{}", args.port);
    info!(addr = %addr, reports = %reports_dir.display(), "starting web server");

    let app = Router::new()
        .route("/", axum::routing::get(index))
        .route("/report/{run_id}", axum::routing::get(report))
        .with_state(reports_dir);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn index(State(reports_dir): State<std::path::PathBuf>) -> Html<String> {
    let mut runs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&reports_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let run_id = entry.file_name().to_string_lossy().to_string();
                let report_path = path.join("report.html");
                if report_path.exists() {
                    let modified = report_path
                        .metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| {
                            let datetime: chrono::DateTime<chrono::Local> = t.into();
                            Some(datetime.format("%Y-%m-%d %H:%M").to_string())
                        })
                        .unwrap_or_default();
                    runs.push((run_id, modified));
                }
            }
        }
    }

    runs.sort_by(|a, b| b.1.cmp(&a.1));

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Daily Paper Reports</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; }
h1 { color: #1a1a1a; }
.report-list { list-style: none; padding: 0; }
.report-list li { margin: 10px 0; padding: 12px; background: #f8f9fa; border-radius: 6px; }
.report-list a { color: #3498db; text-decoration: none; font-weight: 500; }
.report-list a:hover { text-decoration: underline; }
.date { color: #666; font-size: 0.9em; margin-left: 10px; }
</style>
</head>
<body>
<h1>Daily Paper Reports</h1>
"#,
    );

    if runs.is_empty() {
        html.push_str("<p>No reports found.</p>");
    } else {
        html.push_str("<ul class=\"report-list\">");
        for (run_id, date) in &runs {
            html.push_str(&format!(
                "<li><a href=\"/report/{run_id}\">{run_id}</a><span class=\"date\">{date}</span></li>"
            ));
        }
        html.push_str("</ul>");
    }

    html.push_str("</body></html>");
    Html(html)
}

async fn report(
    State(reports_dir): State<std::path::PathBuf>,
    axum::extract::Path(run_id): axum::extract::Path<String>,
) -> Result<Html<String>, (axum::http::StatusCode, String)> {
    let report_path = reports_dir.join(&run_id).join("report.html");

    if !report_path.exists() {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            format!("report not found: {}", run_id),
        ));
    }

    let content = std::fs::read_to_string(&report_path)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Html(content))
}
