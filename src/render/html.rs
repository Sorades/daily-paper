use chrono::Utc;

use crate::error::Result;
use crate::models::common::Author;
use crate::models::read::ReadResult;
use crate::reader::template::load_template;

/// Default HTML report template (used when no custom template is provided).
const DEFAULT_REPORT_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>{{title}}</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Newsreader:ital,opsz,wght@0,6..72,400;0,6..72,500;1,6..72,400&family=Inter:wght@400;450;500;600;700&family=JetBrains+Mono:wght@400;500;600;700&display=swap" rel="stylesheet">
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@fontsource/maple-mono@5.2.6/index.css">
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@fontsource/maple-mono@5.2.6/700.css">
<style>
:root {
  --bg: #faf9f5;
  --text: #191816;
  --text-muted: #5e5b54;
  --text-faint: #8a867e;
  --line-strong: #191816;
  --line-light: #e6e4dc;

  --coral: #cc785c;
  --coral-soft: #f4ebe6;

  --score-high: #246a48;
  --score-mid: #b8621b;
  --score-low: #a83836;

  --link-code-text: #24292f;
  --link-code-border: #8c959f;
  --link-arxiv-text: #8c2d36;
  --link-arxiv-border: #d8969c;
  --link-pdf-text: #2a5a8c;
  --link-pdf-border: #9ab8d8;

  --insight-wash: #f3efe6;

  --font-serif: "Newsreader", "Charter", "Georgia", "Songti SC", serif;
  --font-sans: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", sans-serif;
  --font-mono: "JetBrains Mono", ui-monospace, monospace;
  --font-link-maple: "Maple Mono", "Maple Mono SC NF", "JetBrains Mono", monospace;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg: #141413;
    --text: #ece9e2;
    --text-muted: #a6a299;
    --text-faint: #737067;
    --line-strong: #ece9e2;
    --line-light: #2c2a26;

    --coral: #d97f62;
    --coral-soft: #281d19;

    --score-high: #5ec992;
    --score-mid: #e89b58;
    --score-low: #e87673;

    --link-code-text: #c9d1d9;
    --link-code-border: #484f58;
    --link-arxiv-text: #d66570;
    --link-arxiv-border: #612228;
    --link-pdf-text: #6cb0eb;
    --link-pdf-border: #2b4c6e;

    --insight-wash: #1b1916;
  }
}

* { margin: 0; padding: 0; box-sizing: border-box; }
body {
  font-family: var(--font-sans);
  background: var(--bg);
  color: var(--text);
  line-height: 1.6;
  -webkit-font-smoothing: antialiased;
  padding-bottom: env(safe-area-inset-bottom, 2.5rem);
}

.layout {
  max-width: 780px;
  margin: 0 auto;
  padding: 0 1.25rem;
}

header {
  padding: 1.25rem 0 0.65rem;
  border-bottom: 1.5px solid var(--line-strong);
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 0.35rem;
}
header h1 {
  font-family: var(--font-serif);
  font-size: 1.6rem;
  font-weight: 400;
  letter-spacing: -0.02em;
  color: var(--text);
}
.header-date {
  font-family: var(--font-mono);
  font-size: 0.78rem;
  color: var(--text-faint);
}

.paper-item {
  padding: 1.25rem 0 1.15rem;
  border-bottom: 1px solid var(--line-light);
  overflow-wrap: anywhere;
  word-break: break-word;
}
.paper-item:first-of-type {
  padding-top: 0.85rem;
}
.paper-item:last-of-type {
  border-bottom: 1.5px solid var(--line-strong);
}

.paper-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 0.35rem 0.8rem;
  margin-bottom: 0.35rem;
  font-size: 0.76rem;
}
.head-left {
  display: flex;
  align-items: baseline;
  gap: 0.55rem;
}
.rank-num {
  font-family: var(--font-mono);
  font-weight: 700;
  color: var(--text-muted);
}
.score-num {
  font-family: var(--font-mono);
  font-weight: 700;
}
.score-high { color: var(--score-high); }
.score-mid  { color: var(--score-mid); }
.score-low  { color: var(--score-low); }

.head-links {
  display: inline-flex;
  align-items: center;
  gap: 0.75rem;
  font-family: var(--font-link-maple);
}
.link-btn {
  text-decoration: none;
  display: inline-flex;
  align-items: center;
  gap: 0.32rem;
  font-weight: 500;
  border-bottom: 1px solid transparent;
  transition: all 0.15s;
  line-height: 1.2;
}
.link-icon {
  width: 13px;
  height: 13px;
  flex-shrink: 0;
  vertical-align: -1px;
}
.link-code {
  color: var(--link-code-text);
  font-weight: 500;
  border-bottom-color: var(--link-code-border);
}
.link-arxiv {
  color: var(--link-arxiv-text);
  border-bottom-color: var(--link-arxiv-border);
}
.link-pdf {
  color: var(--link-pdf-text);
  font-weight: 500;
  border-bottom-color: var(--link-pdf-border);
}
.link-btn:hover {
  filter: brightness(0.85);
}

.paper-title {
  font-family: var(--font-serif);
  font-size: 1.28rem;
  font-weight: 500;
  line-height: 1.34;
  letter-spacing: -0.01em;
  color: var(--text);
  margin-bottom: 0.3rem;
}
.paper-title a {
  color: inherit;
  text-decoration: none;
}
.paper-title a:hover {
  color: var(--coral);
}

.paper-meta {
  margin-bottom: 0.85rem;
}
.paper-authors {
  font-size: 0.82rem;
  color: var(--text-muted);
}
.paper-affil {
  font-size: 0.75rem;
  color: var(--text-faint);
  font-style: italic;
  margin-top: 0.1rem;
}

.featured-insight {
  background: var(--insight-wash);
  border-left: 2.5px solid var(--coral);
  padding: 0.65rem 0.85rem;
  border-radius: 0 4px 4px 0;
  margin-bottom: 0.85rem;
}
.insight-label {
  font-family: var(--font-mono);
  font-size: 0.66rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: var(--coral);
  margin-bottom: 0.25rem;
  display: flex;
  align-items: center;
  gap: 0.35rem;
}
.insight-label::before {
  content: "—";
}
.insight-body {
  font-family: var(--font-serif);
  font-size: 0.98rem;
  line-height: 1.55;
  color: var(--text);
}

.paper-details {
  display: grid;
  gap: 0.55rem;
  padding-left: 0.75rem;
  border-left: 1.5px solid var(--line-light);
}

.detail-row {
  line-height: 1.58;
  font-size: 0.84rem;
}

.detail-label {
  display: inline-block;
  width: max-content;
  min-width: 5.4rem;
  white-space: nowrap;
  word-break: keep-all;
  overflow-wrap: normal;
  text-align: right;
  box-sizing: border-box;
  font-family: var(--font-mono);
  font-size: 0.70rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.02em;
  color: var(--text);
  background: linear-gradient(180deg, transparent 55%, rgba(204, 120, 92, 0.22) 55%);
  padding-right: 0.25rem;
  padding-left: 0.2rem;
  margin-right: 0.5rem;
  vertical-align: baseline;
}
@media (prefers-color-scheme: dark) {
  .detail-label {
    background: linear-gradient(180deg, transparent 55%, rgba(217, 127, 98, 0.35) 55%);
  }
}
.detail-text {
  color: var(--text);
  display: inline;
}

.notable-authors {
  margin-top: 0.55rem;
  font-size: 0.75rem;
  color: var(--text-faint);
}
.notable-authors span {
  color: var(--text-muted);
  font-weight: 600;
}

footer {
  text-align: center;
  padding: 2rem 0 1.2rem;
  color: var(--text-faint);
  font-family: var(--font-mono);
  font-size: 0.72rem;
}

@media (max-width: 640px) {
  .layout {
    padding: 0 0.75rem;
  }
  header {
    padding: 0.9rem 0 0.5rem;
  }
  header h1 {
    font-size: 1.35rem;
  }
  .paper-item {
    padding: 0.95rem 0 0.85rem;
  }
  .paper-item:first-of-type {
    padding-top: 0.7rem;
  }
  .paper-title {
    font-size: 1.1rem;
    line-height: 1.32;
  }
  .featured-insight {
    padding: 0.55rem 0.75rem;
    margin-bottom: 0.75rem;
  }
  .insight-body {
    font-size: 0.92rem;
    line-height: 1.5;
  }
  .paper-details {
    padding-left: 0.6rem;
    gap: 0.45rem;
  }
  .detail-row {
    font-size: 0.82rem;
    line-height: 1.55;
  }
  .head-links {
    gap: 0.55rem;
  }
}
</style>
</head>
<body>
<div class="layout">
<header>
  <h1>{{title}}</h1>
  <span class="header-date">{{generated_at}}</span>
</header>
<main>
{{papers}}
</main>
<footer>daily-paper · {{generated_at}} · {{run_id}}</footer>
</div>
</body>
</html>"#;

/// Paper data for report rendering.
pub struct ReportPaper {
    pub paper_id: String,
    pub rank: usize,
    pub title: String,
    pub authors: Vec<Author>,
    pub abstract_text: String,
    pub landing_url: Option<String>,
    pub pdf_url: Option<String>,
    pub read_result: Option<ReadResult>,
    pub score: f32,
}

/// Extract arXiv ID from paper_id (e.g. "arxiv:2605.27209" → "2605.27209").
fn extract_arxiv_id(paper_id: &str) -> Option<&str> {
    paper_id.strip_prefix("arxiv:")
}

/// Format score as percentage string.
fn format_score(score: f32) -> String {
    format!("{:.0}%", (score * 100.0).round())
}

/// Map label name to CSS tag class.
fn label_to_tag_class(label: &str) -> &'static str {
    match label.to_lowercase().as_str() {
        "problem" => "tag-problem",
        "insight" => "tag-insight",
        "method" => "tag-method",
        "results" => "tag-results",
        "limitation" => "tag-limitation",
        "tldr" => "tag-tldr",
        _ => "tag-method",
    }
}

/// Format structured summary text into HTML.
///
/// Separates "Insight" into a featured pull-quote if present, followed by detail rows.
fn format_summary_html(summary: &str) -> String {
    let mut insight_html = String::new();
    let mut details_html = String::new();

    for block in summary.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        if let Some(colon_pos) = block.find(": ") {
            let (label_part, rest) = block.split_at(colon_pos);
            let label = label_part.trim_matches('*').trim();
            let text = &rest[2..]; // skip ": "
            let tag_class = label_to_tag_class(label);
            let escaped_label = escape_html(label);
            let escaped_text = escape_html(text);

            if label.eq_ignore_ascii_case("insight") {
                insight_html = format!(
                    r#"<div class="featured-insight"><div class="insight-label">{escaped_label}</div><div class="insight-body summary-text {tag_class}">{escaped_text}</div></div>"#
                );
            } else {
                details_html.push_str(&format!(
                    r#"<div class="detail-row"><span class="detail-label summary-tag {tag_class}">{escaped_label}</span><span class="detail-text summary-text">{escaped_text}</span></div>"#
                ));
            }
        } else {
            let escaped_block = escape_html(block);
            details_html.push_str(&format!(
                r#"<div class="detail-row"><span class="detail-text summary-text">{escaped_block}</span></div>"#
            ));
        }
    }

    let mut result = String::new();
    if !insight_html.is_empty() {
        result.push_str(&insight_html);
    }
    if !details_html.is_empty() {
        result.push_str(&format!(r#"<div class="paper-details">{details_html}</div>"#));
    }
    result
}

/// Render HTML for a single paper.
fn render_paper_html(paper: &ReportPaper) -> String {
    let mut html = String::new();

    let llm_affiliations = paper
        .read_result
        .as_ref()
        .map(|r| r.author_affiliations.as_slice());

    // Authors line
    let authors_str: Vec<String> = paper.authors.iter().map(|a| escape_html(&a.name)).collect();
    let authors_line = authors_str.join(", ");

    // Affiliations line (separate from authors)
    let mut affs: Vec<String> = Vec::new();
    if let Some(llm_affs) = llm_affiliations {
        for aff in llm_affs {
            if let Some(ref s) = aff.affiliation {
                if !s.is_empty() && !affs.contains(s) {
                    affs.push(s.clone());
                }
            }
        }
    }
    if affs.is_empty() {
        for a in &paper.authors {
            if let Some(ref s) = a.affiliation {
                if !s.is_empty() && !affs.contains(s) {
                    affs.push(s.clone());
                }
            }
        }
    }
    let aff_html = if affs.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="paper-affil">{}</div>"#,
            escape_html(&affs.join(" · "))
        )
    };

    // Title (linked to arXiv if available)
    let title_html = if let Some(arxiv_id) = extract_arxiv_id(&paper.paper_id) {
        format!(
            r#"<a href="https://arxiv.org/abs/{}">{}</a>"#,
            arxiv_id,
            escape_html(&paper.title)
        )
    } else if let Some(url) = &paper.landing_url {
        format!(r#"<a href="{url}">{}</a>"#, escape_html(&paper.title))
    } else {
        escape_html(&paper.title)
    };

    // Score class
    let score_cls = if paper.score >= 0.7 {
        "score-high"
    } else if paper.score >= 0.4 {
        "score-mid"
    } else {
        "score-low"
    };

    // Meta links: Order is Code ↗ -> arXiv ID -> PDF ↗
    let mut links = Vec::new();

    // 1. Project / code links
    let github_icon = r#"<svg class="link-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"><path d="M6 14.5c-3 .5-4.5-1.5-4.5-1.5M10 14.5v-2.2c0-.6-.2-1.1-.6-1.5 2-.2 4.1-1 4.1-4.5 0-1-.3-1.8-1-2.5.1-.2.4-1.2-.1-2.5 0 0-.8-.3-2.6.9-.8-.2-1.6-.3-2.4-.3s-1.6.1-2.4.3c-1.8-1.2-2.6-.9-2.6-.9-.5 1.3-.2 2.3-.1 2.5-.7.7-1 1.5-1 2.5 0 3.5 2.1 4.3 4.1 4.5-.3.3-.5.8-.5 1.5v2.2"></path></svg>"#;
    if let Some(ref result) = paper.read_result {
        if let Some(ref url) = result.metadata.code_url {
            links.push(format!(
                r#"<a class="link-btn link-code" href="{url}">{github_icon}Code ↗</a>"#
            ));
        } else if let Some(ref url) = result.metadata.project_url {
            links.push(format!(
                r#"<a class="link-btn link-code" href="{url}">{github_icon}Project ↗</a>"#
            ));
        }
    }

    // 2. arXiv link
    let arxiv_icon = r#"<svg class="link-icon" viewBox="0 0 24 24" fill="currentColor"><path d="M3.8423 0a1.0037 1.0037 0 0 0-.922.6078c-.1536.3687-.0438.6275.2938 1.1113l6.9185 8.3597-1.0223 1.1058a1.0393 1.0393 0 0 0 .003 1.4229l1.2292 1.3135-5.4391 6.4444c-.2803.299-.4538.823-.2971 1.1986a1.0253 1.0253 0 0 0 .9585.635.9133.9133 0 0 0 .6891-.3405l5.783-6.126 7.4902 8.0051a.8527.8527 0 0 0 .6835.2597.9575.9575 0 0 0 .8777-.6138c.1577-.377-.017-.7502-.306-1.1407l-7.0518-8.3418 1.0632-1.13a.9626.9626 0 0 0 .0089-1.3165L4.6336.4639s-.3733-.4535-.768-.463zm0 .272h.0166c.2179.0052.4874.2715.5644.3639l.005.006.0052.0055 10.169 10.9905a.6915.6915 0 0 1-.0072.945l-1.0666 1.133-1.4982-1.7724-8.5994-10.39c-.3286-.472-.352-.6183-.2592-.841a.7307.7307 0 0 1 .6704-.4401Zm14.341 1.5701a.877.877 0 0 0-.6554.2418l-5.6962 6.1584 1.6944 1.8319 5.3089-6.5138c.3251-.4335.479-.6603.3247-1.0292a1.1205 1.1205 0 0 0-.9763-.689zm-7.6557 12.2823 1.3186 1.4135-5.7864 6.1295a.6494.6494 0 0 1-.4959.26.7516.7516 0 0 1-.706-.4669c-.1119-.2682.0359-.6864.2442-.9083l.0051-.0055.0047-.0055z"/></svg>"#;
    if let Some(arxiv_id) = extract_arxiv_id(&paper.paper_id) {
        links.push(format!(
            r#"<a class="link-btn link-arxiv" href="https://arxiv.org/abs/{arxiv_id}">{arxiv_icon}{arxiv_id}</a>"#
        ));
    } else if let Some(url) = &paper.landing_url {
        links.push(format!(
            r#"<a class="link-btn link-arxiv" href="{url}">{arxiv_icon}Paper</a>"#
        ));
    }

    // 3. PDF link
    let pdf_icon = r#"<svg class="link-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" /><path d="M14 2v5a1 1 0 0 0 1 1h5" /><path d="M10 9H8" /><path d="M16 13H8" /><path d="M16 17H8" /></svg>"#;
    if let Some(arxiv_id) = extract_arxiv_id(&paper.paper_id) {
        links.push(format!(
            r#"<a class="link-btn link-pdf" href="https://arxiv.org/pdf/{arxiv_id}">{pdf_icon}PDF ↗</a>"#
        ));
    } else if let Some(url) = &paper.pdf_url {
        links.push(format!(
            r#"<a class="link-btn link-pdf" href="{url}">{pdf_icon}PDF ↗</a>"#
        ));
    }

    html.push_str(&format!(
        r#"<article class="paper-item">
  <div class="paper-head">
    <div class="head-left">
      <span class="rank-num">#{rank}</span>
      <span class="score-num {score_cls}">{score}</span>
    </div>
    <div class="head-links">
      {links}
    </div>
  </div>
  <h2 class="paper-title">{title}</h2>
  <div class="paper-meta">
    <div class="paper-authors">{authors}</div>
    {aff}
  </div>
"#,
        rank = paper.rank,
        score = format_score(paper.score),
        score_cls = score_cls,
        links = links.join("\n      "),
        title = title_html,
        authors = authors_line,
        aff = aff_html,
    ));

    // Summary
    if let Some(result) = &paper.read_result {
        html.push_str(&format_summary_html(&result.summary));
        if !result.metadata.notable_authors.is_empty() {
            html.push_str(&format!(
                r#"<div class="notable-authors"><span>Notable:</span> {}</div>"#,
                escape_html(&result.metadata.notable_authors.join(", "))
            ));
        }
    } else {
        html.push_str(r#"<div class="paper-details"><div class="detail-row"><span class="detail-text" style="color: var(--text-faint); font-style: italic;">Summary not available.</span></div></div>"#);
    }

    html.push_str("</article>\n");
    html
}

/// Render papers HTML (all papers concatenated).
fn render_papers_html(papers: &[ReportPaper]) -> String {
    papers.iter().map(render_paper_html).collect()
}

/// Render HTML report with optional custom template.
///
/// Template loading priority:
/// 1. Explicit path (must exist, error if not found)
/// 2. `.daily-paper/config/templates/report.html` (optional)
/// 3. Built-in DEFAULT_REPORT_TEMPLATE
pub fn render_html(
    title: &str,
    papers: &[ReportPaper],
    run_id: &str,
    template_path: Option<&str>,
) -> Result<String> {
    let template = load_template(template_path, "report.html", DEFAULT_REPORT_TEMPLATE)?;

    let now = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let papers_html = render_papers_html(papers);

    let html = template
        .replace("{{title}}", &escape_html(title))
        .replace("{{papers}}", &papers_html)
        .replace("{{generated_at}}", &now.to_string())
        .replace("{{run_id}}", &escape_html(run_id));

    Ok(html)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::common::Author;

    #[test]
    fn render_basic_report() {
        let papers = vec![ReportPaper {
            paper_id: "arxiv:2301.12345".into(),
            rank: 1,
            title: "Test Paper".into(),
            authors: vec![
                Author {
                    name: "Alice Smith".into(),
                    normalized_name: None,
                    affiliation: Some("MIT".into()),
                    url: None,
                },
                Author {
                    name: "Bob Jones".into(),
                    normalized_name: None,
                    affiliation: Some("Stanford".into()),
                    url: None,
                },
            ],
            abstract_text: "An abstract.".into(),
            landing_url: Some("https://arxiv.org/abs/2301.12345".into()),
            pdf_url: None,
            read_result: None,
            score: 0.85,
        }];

        let html = render_html("Daily Papers", &papers, "test-run-1", None).unwrap();
        assert!(html.contains("Test Paper"));
        assert!(html.contains("Alice Smith"));
        assert!(html.contains("Bob Jones"));
        assert!(html.contains("MIT"));
        assert!(html.contains("Stanford"));
        assert!(html.contains("#1"));
        assert!(html.contains("test-run-1"));
    }

    #[test]
    fn escape_html_special_chars() {
        assert_eq!(escape_html("<b>bold</b>"), "&lt;b&gt;bold&lt;/b&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("it's"), "it&#39;s");
    }

    #[test]
    fn format_five_part_summary_as_tags() {
        let summary = "**Problem**: Agents fail in noisy environments.\n\n**Insight**: Train with noise injection.\n\n**Method**: NoiseAgent adds perturbations.\n\n**Results**: 12% improvement on MMLU.\n\n**Limitation**: Only tested on text tasks.";

        let html = format_summary_html(summary);

        assert!(html.contains("tag-problem"));
        assert!(html.contains("tag-insight"));
        assert!(html.contains("tag-method"));
        assert!(html.contains("tag-results"));
        assert!(html.contains("tag-limitation"));
        assert!(html.contains("12% improvement"));
    }

    #[test]
    fn format_tldr_summary_as_tag() {
        let html = format_summary_html("**TLDR**: This is a fallback summary.");

        assert!(html.contains("tag-tldr"));
        assert!(html.contains("fallback summary"));
    }
}
