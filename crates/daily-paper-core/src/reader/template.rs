use sha2::Digest;
use std::path::Path;

/// Default system prompt for deep reading (fallback if template file not found).
const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a research paper analyst. Given a paper's title, abstract, and selected text sections, produce:

1. A concise summary in the specified language
2. List of institutions/affiliations mentioned in the paper

Output valid JSON in this exact format:
{
  "summary": "Your concise summary paragraph here...",
  "author_affiliations": [
    {"name": "Author Name", "affiliation": "University or Company"}
  ],
  "notable_authors": ["Famous Author 1", "Famous Author 2"],
  "project_url": "https://project-page.example.com or null",
  "code_url": "https://github.com/example/repo or null"
}

Guidelines for summary:
- Explain what the paper does
- Describe the method or technical approach
- State the key results or contributions
- Explain relevance to reader's interests

Guidelines for affiliations:
- Extract ALL institutions mentioned in the paper
- Look for affiliations in footnotes, author blocks, or first page
- Use the most specific institution name (e.g., "MIT CSAIL" not just "MIT")
- Include each unique institution only once
- For each author, try to find their affiliation

Output ONLY the JSON, no other text."#;

/// Build the user prompt from paper info and selected text.
pub fn build_user_prompt(
    title: &str,
    abstract_text: &str,
    authors: &str,
    selected_text: &str,
    language: &str,
) -> String {
    format!(
        r#"Title: {title}

Authors: {authors}

Abstract: {abstract_text}

Selected text from the paper:
---
{selected_text}
---

Summarize this paper in {language}. Extract author affiliations from the text."#
    )
}

/// Build the system prompt (can be overridden by template file).
///
/// Looks for template in this order:
/// 1. `.daily-paper/config/templates/system_prompt.txt` (project mode)
/// 2. `templates/system_prompt.txt` relative to current directory
/// 3. Built-in default
pub fn build_system_prompt(template_dir: Option<&Path>) -> String {
    // Try to load from template directory
    if let Some(dir) = template_dir {
        let path = dir.join("system_prompt.txt");
        if let Ok(content) = std::fs::read_to_string(&path) {
            return content;
        }
    }

    // Try default locations
    let default_paths = [
        Path::new(".daily-paper/config/templates/system_prompt.txt"),
        Path::new("templates/system_prompt.txt"),
    ];

    for path in &default_paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return content;
        }
    }

    // Fall back to built-in
    DEFAULT_SYSTEM_PROMPT.to_string()
}

/// Parsed LLM output containing summary and metadata.
#[derive(Debug, Clone)]
pub struct ParsedLlmOutput {
    pub summary: String,
    pub author_affiliations: Vec<AuthorAffiliation>,
    pub project_url: Option<String>,
    pub code_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuthorAffiliation {
    pub name: String,
    pub affiliation: Option<String>,
}

/// Parse LLM JSON output.
pub fn parse_llm_output(json_str: &str) -> Option<ParsedLlmOutput> {
    // Try to extract JSON from the response (handle markdown code blocks)
    let json_str = json_str.trim();
    let json_str = if json_str.starts_with("```") {
        // Remove markdown code block markers
        let lines: Vec<&str> = json_str.lines().collect();
        let start = lines.iter().position(|l| l.contains('{'))?;
        let end = lines.iter().rposition(|l| l.contains('}'))?;
        lines[start..=end].join("\n")
    } else {
        json_str.to_string()
    };

    let v: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    // Parse summary — can be a string (old format) or object (new structured format)
    let summary = match v.get("summary")? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(obj) => {
            // Structured summary: { problem, insight, method, results, limitation }
            let parts: Vec<String> = [
                ("Problem", obj.get("problem")),
                ("Insight", obj.get("insight")),
                ("Method", obj.get("method")),
                ("Results", obj.get("results")),
                ("Limitation", obj.get("limitation")),
            ]
            .iter()
            .filter_map(|(label, val)| {
                let s = val.and_then(|v| v.as_str())?.trim();
                if s.is_empty() || s == "null" { None }
                else { Some(format!("**{}**: {}", label, s)) }
            })
            .collect();
            parts.join("\n\n")
        }
        _ => return None,
    };

    let author_affiliations = v
        .get("author_affiliations")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    let affiliation = item
                        .get("affiliation")
                        .and_then(|a| a.as_str())
                        .filter(|s| !s.is_empty() && *s != "null")
                        .map(|s| s.to_string());
                    Some(AuthorAffiliation { name, affiliation })
                })
                .collect()
        })
        .unwrap_or_default();

    let project_url = v
        .get("project_url")
        .and_then(|u| u.as_str())
        .filter(|s| !s.is_empty() && *s != "null")
        .map(|s| s.to_string());

    let code_url = v
        .get("code_url")
        .and_then(|u| u.as_str())
        .filter(|s| !s.is_empty() && *s != "null")
        .map(|s| s.to_string());

    Some(ParsedLlmOutput {
        summary,
        author_affiliations,
        project_url,
        code_url,
    })
}

/// Compute the template hash for cache invalidation.
pub fn compute_template_hash(template: &str) -> String {
    hex::encode(sha2::Sha256::digest(template.as_bytes()))
}

/// Estimate token count from text (rough: ~4 chars per token for English).
pub fn estimate_tokens(text: usize) -> usize {
    text / 4
}

/// Trim text to fit within token budget, preserving structure.
///
/// Prioritizes keeping the beginning and end of the text.
pub fn trim_to_token_budget(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }

    // Keep first 60% and last 30%, cut middle
    let keep_start = (max_chars * 6) / 10;
    let keep_end = (max_chars * 3) / 10;
    let cut_marker = "\n\n[... text trimmed for length ...]\n\n";

    let start = &text[..keep_start.min(text.len())];
    let end_start = text.len().saturating_sub(keep_end);
    let end = &text[end_start..];

    format!("{}{}{}", start, cut_marker, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_contains_fields() {
        let prompt = build_user_prompt(
            "Test Paper",
            "An abstract.",
            "Alice Smith",
            "Selected text here.",
            "Chinese",
        );
        assert!(prompt.contains("Test Paper"));
        assert!(prompt.contains("Alice Smith"));
        assert!(prompt.contains("Chinese"));
    }

    #[test]
    fn template_hash_deterministic() {
        let h1 = compute_template_hash("test template");
        let h2 = compute_template_hash("test template");
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn trim_short_text_unchanged() {
        let text = "short text";
        assert_eq!(trim_to_token_budget(text, 1000), text);
    }

    #[test]
    fn trim_long_text_reduced() {
        let text = "a".repeat(10000);
        let trimmed = trim_to_token_budget(&text, 1000);
        assert!(trimmed.len() <= 1000 + 100); // marker adds some chars
        assert!(trimmed.contains("trimmed"));
    }

    #[test]
    fn default_system_prompt_non_empty() {
        let prompt = build_system_prompt(None);
        assert!(prompt.len() > 50);
    }

    #[test]
    fn load_template_from_file() {
        let dir = Path::new("tests/fixtures");
        let prompt = build_system_prompt(Some(dir));
        // Should load from file if exists, otherwise fall back to default
        assert!(prompt.len() > 50);
    }

    #[test]
    fn parse_valid_llm_output() {
        // Old format (string summary) — backward compatible
        let json = r#"{
            "summary": "This paper proposes a new method.",
            "author_affiliations": [
                {"name": "Alice Smith", "affiliation": "MIT"},
                {"name": "Bob Jones", "affiliation": "Stanford"}
            ],
            "project_url": "https://example.com",
            "code_url": null
        }"#;

        let result = parse_llm_output(json).unwrap();
        assert_eq!(result.summary, "This paper proposes a new method.");
        assert_eq!(result.author_affiliations.len(), 2);
        assert_eq!(result.author_affiliations[0].name, "Alice Smith");
        assert_eq!(result.author_affiliations[0].affiliation, Some("MIT".into()));
        assert_eq!(result.project_url, Some("https://example.com".into()));
        assert_eq!(result.code_url, None);
    }

    #[test]
    fn parse_structured_summary() {
        let json = r#"{
            "summary": {
                "problem": "Agents fail in noisy environments.",
                "insight": "Train with noise injection.",
                "method": "NoiseAgent adds perturbations to training rollouts.",
                "results": "12% improvement on MMLU.",
                "limitation": "Only tested on text-based tasks."
            },
            "author_affiliations": [],
            "project_url": null,
            "code_url": null
        }"#;

        let result = parse_llm_output(json).unwrap();
        assert!(result.summary.contains("Problem"));
        assert!(result.summary.contains("Agents fail"));
        assert!(result.summary.contains("Results"));
        assert!(result.summary.contains("12%"));
    }

    #[test]
    fn parse_llm_output_with_markdown() {
        let json = r#"```json
{
    "summary": "Test summary",
    "author_affiliations": [],
    "project_url": null,
    "code_url": null
}
```"#;

        let result = parse_llm_output(json).unwrap();
        assert_eq!(result.summary, "Test summary");
    }
}
