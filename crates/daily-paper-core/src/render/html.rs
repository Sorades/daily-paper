use chrono::Utc;

use crate::models::common::Author;
use crate::models::read::ReadResult;

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
        _ => "tag-method",
    }
}

/// Format structured summary text into HTML.
///
/// Converts "**Label**: text" blocks into flex rows with fixed-width tags.
fn format_summary_html(summary: &str) -> String {
    let mut html = String::new();
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
            html.push_str(&format!(
                r#"<div class="summary-block"><span class="summary-tag {tag_class}">{label}</span><span class="summary-text">{text}</span></div>"#,
                tag_class = tag_class,
                label = escape_html(label),
                text = escape_html(text),
            ));
        } else {
            html.push_str(&format!(
                r#"<div class="summary-block"><span class="summary-text">{}</span></div>"#,
                escape_html(block)
            ));
        }
    }
    html
}

/// Render HTML report.
pub fn render_html(title: &str, papers: &[ReportPaper], run_id: &str) -> String {
    let mut html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
:root {{
  --bg: #ffffff;
  --card-bg: #ffffff;
  --card-border: #e5e7eb;
  --text: #111827;
  --text-dim: #6b7280;
  --text-faint: #9ca3af;
  --accent: #4f46e5;
  --accent-soft: #eef2ff;
  --score-high: #059669;
  --score-mid: #d97706;
  --score-low: #dc2626;
}}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  background: var(--bg);
  color: var(--text);
  line-height: 1.6;
  -webkit-font-smoothing: antialiased;
}}
header {{
  padding: 2.5rem 2rem 1.5rem;
  border-bottom: 1px solid var(--card-border);
}}
header h1 {{
  font-size: 1.3rem;
  font-weight: 600;
  color: var(--text);
  letter-spacing: -0.01em;
}}
.container {{
  max-width: 820px;
  margin: 0 auto;
  padding: 1.2rem 1.2rem 3rem;
}}
.paper {{
  background: var(--card-bg);
  border: 1px solid var(--card-border);
  border-radius: 8px;
  padding: 1.3rem 1.5rem;
  margin-bottom: 0.8rem;
  transition: border-color 0.15s;
}}
.paper:hover {{
  border-color: #d1d5db;
}}
.paper-head {{
  display: flex;
  align-items: flex-start;
  gap: 0.8rem;
}}
.rank {{
  flex-shrink: 0;
  width: 1.6rem;
  height: 1.6rem;
  background: var(--accent-soft);
  color: var(--accent);
  border-radius: 5px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 700;
  font-size: 0.75rem;
}}
.title {{
  font-size: 1rem;
  font-weight: 600;
  color: var(--text);
  line-height: 1.45;
}}
.title a {{
  color: inherit;
  text-decoration: none;
}}
.title a:hover {{
  color: var(--accent);
}}
.authors {{
  margin-top: 0.4rem;
  font-size: 0.8rem;
  color: var(--text-dim);
  line-height: 1.5;
}}
.affiliations {{
  margin-top: 0.1rem;
  font-size: 0.72rem;
  color: var(--text-faint);
}}
.meta-bar {{
  margin-top: 0.5rem;
  padding: 0.4rem 0;
}}
.meta-bar .authors {{
  margin-top: 0;
}}
.meta-bar .affiliations {{
  margin-top: 0.1rem;
}}
.meta-row {{
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 0.4rem;
  flex-wrap: wrap;
}}
.chip {{
  display: inline-flex;
  align-items: center;
  font-size: 0.72rem;
  padding: 0.15rem 0.5rem;
  border-radius: 4px;
  gap: 0.25rem;
  text-decoration: none;
  transition: all 0.15s;
}}
.chip-arxiv {{
  background: var(--accent-soft);
  color: var(--accent);
}}
.chip-arxiv:hover {{
  background: #e0e7ff;
  color: var(--accent);
}}
.chip-pdf {{
  background: #fef2f2;
  color: #b91c1c;
}}
.chip-pdf:hover {{
  background: #fee2e2;
}}
.chip-score {{
  font-weight: 600;
}}
.chip-score-high {{ background: #ecfdf5; color: var(--score-high); }}
.chip-score-mid  {{ background: #fffbeb; color: var(--score-mid); }}
.chip-score-low  {{ background: #fef2f2; color: var(--score-low); }}
.chip-proj {{
  background: var(--accent-soft);
  color: var(--accent);
}}
.chip-proj:hover {{ background: #e0e7ff; }}
.summary {{
  margin-top: 0.6rem;
  border-left: 2px solid #d0d5dd;
  padding-left: 0.6rem;
  font-size: 0.82rem;
  line-height: 1.55;
}}
.summary-block {{
  padding: 0.25rem 0;
}}
.summary-tag {{
  display: inline-block;
  min-width: 5rem;
  white-space: nowrap;
  font-size: 0.68rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0.1rem 0.4rem;
  border-radius: 3px;
  text-align: center;
  vertical-align: middle;
  margin-right: 0.4rem;
}}
.tag-problem  {{ background: #fef3c7; color: #92400e; }}
.tag-insight  {{ background: #dbeafe; color: #1e40af; }}
.tag-method   {{ background: #e0e7ff; color: #3730a3; }}
.tag-results  {{ background: #d1fae5; color: #065f46; }}
.tag-limitation {{ background: #fce7d5; color: #9a3412; }}
.summary-text {{
  color: #374151;
}}
.notable {{
  margin-top: 0.4rem;
  font-size: 0.75rem;
  color: var(--text-faint);
}}
.notable span {{ color: var(--text-dim); }}
footer {{
  text-align: center;
  padding: 2rem 1rem;
  color: var(--text-faint);
  font-size: 0.7rem;
}}
</style>
</head>
<body>
<header>
  <h1>{title}</h1>
</header>
<div class="container">
"#,
        title = escape_html(title),
    );

    for paper in papers {
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
                r#"<div class="affiliations">{}</div>"#,
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

        // Score chip
        let score_cls = if paper.score >= 0.7 {
            "chip-score-high"
        } else if paper.score >= 0.4 {
            "chip-score-mid"
        } else {
            "chip-score-low"
        };
        let score_chip = format!(
            r#"<span class="chip chip-score {score_cls}">{}</span>"#,
            format_score(paper.score)
        );

        // Meta chips: score + arXiv ID + PDF
        let mut chips = vec![score_chip];
        if let Some(arxiv_id) = extract_arxiv_id(&paper.paper_id) {
            chips.push(format!(
                r#"<a class="chip chip-arxiv" href="https://arxiv.org/abs/{}">{}</a>"#,
                arxiv_id, arxiv_id
            ));
            chips.push(format!(
                r#"<a class="chip chip-pdf" href="https://arxiv.org/pdf/{}">PDF</a>"#,
                arxiv_id
            ));
        } else {
            if let Some(url) = &paper.landing_url {
                chips.push(format!(
                    r#"<a class="chip chip-arxiv" href="{url}">Paper</a>"#
                ));
            }
            if let Some(url) = &paper.pdf_url {
                chips.push(format!(r#"<a class="chip chip-pdf" href="{url}">PDF</a>"#));
            }
        }

        // Project / code links as chips
        if let Some(ref result) = paper.read_result {
            if let Some(ref url) = result.metadata.project_url {
                chips.push(format!(
                    r#"<a class="chip chip-proj" href="{url}">Project</a>"#
                ));
            }
            if let Some(ref url) = result.metadata.code_url {
                chips.push(format!(
                    r#"<a class="chip chip-proj" href="{url}">Code</a>"#
                ));
            }
        }

        html.push_str(&format!(
            r#"<div class="paper">
  <div class="paper-head">
    <div class="rank">{rank}</div>
    <div class="title">{title}</div>
  </div>
  <div class="meta-bar">
    <div class="authors">{authors}</div>
    {aff}
    <div class="meta-row">{chips}</div>
  </div>
"#,
            rank = paper.rank,
            title = title_html,
            authors = authors_line,
            aff = aff_html,
            chips = chips.join("\n    "),
        ));

        // Summary
        if let Some(result) = &paper.read_result {
            html.push_str(&format!(
                r#"  <div class="summary">{}</div>"#,
                format_summary_html(&result.summary)
            ));
            if !result.metadata.notable_authors.is_empty() {
                html.push_str(&format!(
                    r#"  <div class="notable"><span>Notable:</span> {}</div>"#,
                    escape_html(&result.metadata.notable_authors.join(", "))
                ));
            }
        } else {
            html.push_str(r#"  <div class="summary"><em>Summary not available.</em></div>"#);
        }

        html.push_str("</div>\n");
    }

    html.push_str("</div>\n");

    let now = Utc::now().format("%Y-%m-%d %H:%M UTC");
    html.push_str(&format!(
        r#"<footer>daily-paper · {now} · {run_id}</footer>
</body>
</html>"#
    ));

    html
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

        let html = render_html("Daily Papers", &papers, "test-run-1");
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
}
