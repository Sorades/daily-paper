use crate::error::{Error, Result};
use super::raw::RawConfig;
use super::resolved::ResolvedConfig;
use std::path::{Path, PathBuf};

/// Load and resolve configuration from a TOML file.
pub fn load_config(path: &Path) -> Result<(RawConfig, ResolvedConfig)> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!("failed to read config file '{}': {}", path.display(), e))
    })?;

    let raw: RawConfig = toml::from_str(&content)?;
    let resolved = ResolvedConfig::from_raw(&raw)?;
    Ok((raw, resolved))
}

/// Find the default config file path.
///
/// Priority:
/// 1. `.daily-paper/config/config.toml` in current directory (project mode)
/// 2. Platform config directory (system mode)
pub fn default_config_path() -> Option<PathBuf> {
    // Check for project-local config first
    let project_config = Path::new(".daily-paper/config/config.toml");
    if project_config.exists() {
        return Some(project_config.to_path_buf());
    }

    // Fall back to platform config directory
    directories::ProjectDirs::from("", "", "daily-paper")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}

/// Find the default state directory.
///
/// Priority:
/// 1. `.daily-paper/state` in current directory (project mode)
/// 2. Platform data directory (system mode)
pub fn default_state_dir() -> PathBuf {
    // Check for project-local directory
    let project_dir = Path::new(".daily-paper");
    if project_dir.exists() {
        return project_dir.join("state");
    }

    // Fall back to platform data directory
    directories::ProjectDirs::from("", "", "daily-paper")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".daily-paper"))
}

/// Check if we're in project mode (`.daily-paper/` exists in current directory).
pub fn is_project_mode() -> bool {
    Path::new(".daily-paper").exists()
}

/// Initialize project directory structure with default files.
///
/// Creates `.daily-paper/config/` with config and templates.
/// Does not touch state/ or cache/ directories.
pub fn init_project_dir() -> Result<PathBuf> {
    let project_dir = Path::new(".daily-paper");
    let config_dir = project_dir.join("config");

    // Create project root if it doesn't exist
    if !project_dir.exists() {
        std::fs::create_dir_all(project_dir)?;
        std::fs::create_dir_all(project_dir.join("state"))?;
    }

    // Create config directory
    std::fs::create_dir_all(config_dir.join("templates"))?;

    // Write default config
    std::fs::write(
        config_dir.join("config.toml"),
        DEFAULT_CONFIG,
    )?;

    // Write default system prompt
    std::fs::write(
        config_dir.join("templates/system_prompt.txt"),
        DEFAULT_SYSTEM_PROMPT,
    )?;

    // Write default HTML template
    std::fs::write(
        config_dir.join("templates/report.html"),
        DEFAULT_REPORT_TEMPLATE,
    )?;

    tracing::info!("initialized config at {}", config_dir.display());

    Ok(project_dir.to_path_buf())
}

const DEFAULT_CONFIG: &str = r#"# Daily Paper 配置文件
# 文档: https://github.com/your-repo/daily-paper

[state]
# 状态目录（默认使用 .daily-paper/state，无需配置）
# dir = "/custom/path"

[zotero]
user_id = ""
api_key_env = "ZOTERO_API_KEY"
max_snapshot_age_hours = 168

[[zotero.filters]]
path = "Papers/**"
weight = 2.0

[[sources]]
kind = "arxiv"
categories = ["cs.AI", "cs.CL", "cs.LG"]
include_cross_list = false

[embedding]
kind = "fastembed"
model = "BAAI/bge-small-en-v1.5"
batch_size = 64

[reranker]
kind = "embedding_similarity"
top_k_library_matches = 20

[reader]
kind = "openai-compatible"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
model = "gpt-4o-mini"
top_n = 10
language = "zh-CN"
timeout_secs = 120
max_retries = 3

[pdf]
extractor = "pdftotext"
timeout_secs = 60
max_pdf_mb = 50
max_text_chars = 300000

[email]
smtp_server = "smtp.example.com"
smtp_port = 465
sender = "you@example.com"
receiver = "you@example.com"
password_env = "SMTP_PASSWORD"
"#;

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a research paper analyst. Given a paper's title, abstract, and selected text sections, produce a structured analysis.

Output valid JSON in this exact format:
{
  "summary": {
    "problem": "What problem does this paper solve? Why does it matter? (1-2 sentences)",
    "insight": "What is the key idea or novelty that distinguishes this work? (1-2 sentences)",
    "method": "High-level technical approach — no formulas, just the pipeline or core mechanism (2-3 sentences)",
    "results": "Key quantitative results: benchmarks, numbers, comparisons (1-2 sentences)",
    "limitation": "Main limitations or boundary conditions (1 sentence, or null if not discussed)"
  },
  "author_affiliations": [
    {"name": "Author Name", "affiliation": "University or Company"}
  ],
  "project_url": "https://project-page.example.com or null",
  "code_url": "https://github.com/example/repo or null"
}

Guidelines:
- Write in the specified language
- Be precise and concise — every word should carry information
- For results: always include specific numbers if available (e.g., "outperforms baseline by 12% on MMLU")
- For insight: focus on what's NEW, not what's standard
- For limitation: be honest, not generic (avoid "needs more data" type statements)
- Extract ALL unique institutions from the paper (affiliations section, footnotes, first page)
- Use specific institution names (e.g., "MIT CSAIL" not just "MIT")

Output ONLY the JSON, no other text."#;

const DEFAULT_REPORT_TEMPLATE: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{{title}}</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; line-height: 1.6; color: #333; }
h1 { color: #1a1a1a; border-bottom: 2px solid #e0e0e0; padding-bottom: 10px; }
.paper { margin: 20px 0; padding: 15px; border: 1px solid #e0e0e0; border-radius: 8px; }
.paper h2 { margin-top: 0; color: #2c3e50; }
.authors { color: #666; font-style: italic; }
.authors small { display: block; margin-top: 4px; font-style: normal; color: #888; }
.summary { background: #f8f9fa; padding: 12px; border-radius: 4px; margin-top: 10px; }
.links a { margin-right: 15px; color: #3498db; text-decoration: none; }
.links a:hover { text-decoration: underline; }
.footer { margin-top: 30px; padding-top: 10px; border-top: 1px solid #e0e0e0; color: #999; font-size: 0.85em; }
</style>
</head>
<body>
<h1>{{title}}</h1>
{{papers}}
<div class="footer">Generated by daily-paper on {{generated_at}} (run: {{run_id}})</div>
</body>
</html>
"#;
