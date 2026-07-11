use sha2::Digest;

use crate::error::Error;

/// Default system prompt for deep reading (fallback if template file not found).
const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a research paper analyst. Given a paper's title, abstract, and selected text sections, produce a structured analysis.

Output valid JSON in this exact format:
{
  "summary": {
    "problem": "What problem does this paper solve? Why does it matter? (1-2 sentences)",
    "insight": "What is the key idea or novelty that distinguishes this work? (1-2 sentences)",
    "method": "High-level technical approach, no formulas, just the pipeline or core mechanism (2-3 sentences)",
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
- Write summary field values in the specified language.
- Use exactly the five summary keys shown above: problem, insight, method, results, limitation.
- Do not include Markdown headings, bullet lists, numbered lists, author lists, or prose outside JSON.
- Do not put author or affiliation details inside summary.
- Be precise and concise; every word should carry information.
- For results, include specific numbers if available.
- For insight, focus on what is new, not what is standard.
- For limitation, be honest and specific; use null if the paper does not discuss one.
- Extract all unique institutions from affiliations, footnotes, or the first page.
- Use specific institution names, such as "MIT CSAIL" rather than just "MIT".

Output ONLY the JSON, no other text."#;

const TLDR_SYSTEM_PROMPT: &str = r#"You are a research paper analyst. The structured report format failed. Produce a short fallback TLDR.

Output valid JSON in this exact format:
{
  "tldr": "A concise 2-4 sentence summary in the specified language."
}

Guidelines:
- Write the TLDR in the specified language.
- Mention the core problem, the main idea, and the most important result if available.
- Do not include Markdown headings, bullet lists, numbered lists, author lists, or prose outside JSON.
- Output ONLY the JSON, no other text."#;

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

/// Build the fallback TLDR prompt from paper info and selected text.
pub fn build_tldr_user_prompt(
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

Write a TLDR for this paper in {language}."#
    )
}

/// Build the fallback TLDR system prompt.
pub fn build_tldr_system_prompt() -> String {
    TLDR_SYSTEM_PROMPT.to_string()
}

/// Load a template file with two-level priority:
///
/// 1. Explicit path (must exist, error if not found)
/// 2. Built-in default
///
/// When no explicit path is given, tries `.daily-paper/config/templates/{default_filename}` first.
pub fn load_template(
    explicit_path: Option<&str>,
    default_filename: &str,
    fallback: &str,
) -> crate::error::Result<String> {
    // 1. Explicit path -> must exist
    if let Some(p) = explicit_path {
        return std::fs::read_to_string(p)
            .map_err(|_| Error::Render(format!("template file not found: {}", p)));
    }

    // 2. Default config path -> optional
    let default_path = format!(".daily-paper/config/templates/{}", default_filename);
    if let Ok(content) = std::fs::read_to_string(&default_path) {
        return Ok(content);
    }

    // 3. Built-in fallback
    Ok(fallback.to_string())
}

/// Build the system prompt using unified template loading.
pub fn build_system_prompt(explicit_path: Option<&str>) -> crate::error::Result<String> {
    load_template(explicit_path, "system_prompt.txt", DEFAULT_SYSTEM_PROMPT)
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

const SUMMARY_LABELS: [&str; 5] = ["Problem", "Insight", "Method", "Results", "Limitation"];

/// Validate normalized report summary formats used by report rendering.
pub fn is_valid_structured_summary(summary: &str) -> bool {
    let blocks: Vec<&str> = summary
        .split("\n\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if blocks.len() == 1 {
        let prefix = "**TLDR**: ";
        return blocks[0].starts_with(prefix) && !blocks[0][prefix.len()..].trim().is_empty();
    }

    blocks.len() == SUMMARY_LABELS.len()
        && blocks.iter().zip(SUMMARY_LABELS).all(|(block, label)| {
            let prefix = format!("**{}**: ", label);
            block.starts_with(&prefix) && !block[prefix.len()..].trim().is_empty()
        })
}

fn extract_json_payload(json_str: &str) -> Result<String, String> {
    let json_str = json_str.trim();
    if json_str.starts_with("```") && json_str.ends_with("```") {
        let lines: Vec<&str> = json_str.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.trim_start().starts_with('{'))
            .ok_or_else(|| "markdown code block does not contain JSON object".to_string())?;
        let end = lines
            .iter()
            .rposition(|l| l.trim_end().ends_with('}'))
            .ok_or_else(|| "markdown code block does not contain JSON object".to_string())?;
        Ok(lines[start..=end].join("\n"))
    } else {
        Ok(json_str.to_string())
    }
}

/// Parse LLM JSON output.
pub fn parse_llm_output(json_str: &str) -> Result<ParsedLlmOutput, String> {
    let json_str = extract_json_payload(json_str)?;

    let v: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("invalid JSON: {}", e))?;

    let summary = match v
        .get("summary")
        .ok_or_else(|| "missing summary".to_string())?
    {
        serde_json::Value::Object(obj) => {
            let parts: Vec<String> = [
                ("Problem", "problem", false),
                ("Insight", "insight", false),
                ("Method", "method", false),
                ("Results", "results", false),
                ("Limitation", "limitation", true),
            ]
            .iter()
            .map(|(label, key, nullable)| {
                let val = obj
                    .get(*key)
                    .ok_or_else(|| format!("missing summary.{}", key))?;
                let text = match val {
                    serde_json::Value::String(s) if !s.trim().is_empty() && s.trim() != "null" => {
                        s.trim().to_string()
                    }
                    serde_json::Value::Null if *nullable => "Not discussed.".to_string(),
                    serde_json::Value::String(_) => {
                        return Err(format!("summary.{} must be non-empty", key));
                    }
                    serde_json::Value::Null => {
                        return Err(format!("summary.{} must not be null", key));
                    }
                    _ => return Err(format!("summary.{} must be a string", key)),
                };
                Ok(format!("**{}**: {}", label, text))
            })
            .collect::<Result<Vec<_>, String>>()?;

            let summary = parts.join("\n\n");
            if !is_valid_structured_summary(&summary) {
                return Err("summary did not normalize to the required five-part format".into());
            }
            summary
        }
        serde_json::Value::String(_) => {
            return Err("summary must be an object with five fixed fields".into());
        }
        _ => return Err("summary must be an object".into()),
    };

    let author_affiliations = v
        .get("author_affiliations")
        .ok_or_else(|| "missing author_affiliations".to_string())?
        .as_array()
        .ok_or_else(|| "author_affiliations must be an array".to_string())?
        .iter()
        .map(|item| {
            let obj = item
                .as_object()
                .ok_or_else(|| "author_affiliations item must be an object".to_string())?;
            let name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "author_affiliations item missing non-empty name".to_string())?;
            let affiliation = match obj.get("affiliation") {
                Some(serde_json::Value::String(s)) => {
                    let s = s.trim();
                    if s.is_empty() || s == "null" {
                        None
                    } else {
                        Some(s.to_string())
                    }
                }
                Some(serde_json::Value::Null) | None => None,
                Some(_) => return Err("author affiliation must be a string or null".to_string()),
            };
            Ok(AuthorAffiliation {
                name: name.to_string(),
                affiliation,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let parse_optional_url = |field: &str| -> Result<Option<String>, String> {
        match v.get(field) {
            Some(serde_json::Value::String(s)) => {
                let s = s.trim();
                if s.is_empty() || s == "null" {
                    Ok(None)
                } else {
                    Ok(Some(s.to_string()))
                }
            }
            Some(serde_json::Value::Null) | None => Ok(None),
            Some(_) => Err(format!("{} must be a string or null", field)),
        }
    };

    Ok(ParsedLlmOutput {
        summary,
        author_affiliations,
        project_url: parse_optional_url("project_url")?,
        code_url: parse_optional_url("code_url")?,
    })
}

/// Parse fallback TLDR JSON output and normalize it for report rendering.
pub fn parse_tldr_output(json_str: &str) -> Result<String, String> {
    let json_str = extract_json_payload(json_str)?;
    let v: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("invalid JSON: {}", e))?;
    let tldr = v
        .get("tldr")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing non-empty tldr".to_string())?;
    let summary = format!("**TLDR**: {}", tldr);
    if !is_valid_structured_summary(&summary) {
        return Err("TLDR did not normalize to the required fallback format".into());
    }
    Ok(summary)
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
        let prompt = build_system_prompt(None).unwrap();
        assert!(prompt.len() > 50);
    }

    #[test]
    fn load_template_from_file() {
        // When explicit path doesn't exist, should return error
        let result = build_system_prompt(Some("nonexistent/path/system_prompt.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_valid_llm_output() {
        let json = r#"{
            "summary": {
                "problem": "Agents fail in noisy environments.",
                "insight": "Train with noise injection.",
                "method": "NoiseAgent adds perturbations to training rollouts.",
                "results": "12% improvement on MMLU.",
                "limitation": "Only tested on text-based tasks."
            },
            "author_affiliations": [
                {"name": "Alice Smith", "affiliation": "MIT"},
                {"name": "Bob Jones", "affiliation": "Stanford"}
            ],
            "project_url": "https://example.com",
            "code_url": null
        }"#;

        let result = parse_llm_output(json).unwrap();
        assert!(result.summary.contains("**Problem**: Agents fail"));
        assert_eq!(result.author_affiliations.len(), 2);
        assert_eq!(result.author_affiliations[0].name, "Alice Smith");
        assert_eq!(
            result.author_affiliations[0].affiliation,
            Some("MIT".into())
        );
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
        assert!(is_valid_structured_summary(&result.summary));
    }

    #[test]
    fn parse_llm_output_with_markdown_code_block() {
        let json = r#"```json
{
    "summary": {
        "problem": "Agents fail in noisy environments.",
        "insight": "Train with noise injection.",
        "method": "NoiseAgent adds perturbations to training rollouts.",
        "results": "12% improvement on MMLU.",
        "limitation": null
    },
    "author_affiliations": [],
    "project_url": null,
    "code_url": null
}
```"#;

        let result = parse_llm_output(json).unwrap();
        assert!(result.summary.contains("**Limitation**: Not discussed."));
    }

    #[test]
    fn parse_rejects_string_summary() {
        let json = r#"{
            "summary": "This paper proposes a new method.",
            "author_affiliations": [],
            "project_url": null,
            "code_url": null
        }"#;

        assert!(parse_llm_output(json).is_err());
    }

    #[test]
    fn parse_rejects_missing_summary_field() {
        let json = r#"{
            "summary": {
                "problem": "Agents fail in noisy environments.",
                "insight": "Train with noise injection.",
                "method": "NoiseAgent adds perturbations to training rollouts.",
                "limitation": null
            },
            "author_affiliations": [],
            "project_url": null,
            "code_url": null
        }"#;

        assert!(parse_llm_output(json).is_err());
    }

    #[test]
    fn parse_rejects_extra_text_around_json() {
        let json = r#"Here is the JSON:
{
    "summary": {
        "problem": "Agents fail in noisy environments.",
        "insight": "Train with noise injection.",
        "method": "NoiseAgent adds perturbations to training rollouts.",
        "results": "12% improvement on MMLU.",
        "limitation": null
    },
    "author_affiliations": [],
    "project_url": null,
    "code_url": null
}"#;

        assert!(parse_llm_output(json).is_err());
    }

    #[test]
    fn validate_structured_summary_rejects_markdown_report() {
        let summary = "### 论文摘要\n本文提出一种方法。\n\n### 作者及所属机构\n1. Alice: MIT";
        assert!(!is_valid_structured_summary(summary));
    }

    #[test]
    fn parse_tldr_output_normalizes_summary() {
        let json = r#"{"tldr": "这篇论文提出了一种更稳健的方法，并在主要基准上取得提升。"}"#;

        let summary = parse_tldr_output(json).unwrap();

        assert_eq!(
            summary,
            "**TLDR**: 这篇论文提出了一种更稳健的方法，并在主要基准上取得提升。"
        );
        assert!(is_valid_structured_summary(&summary));
    }

    #[test]
    fn parse_tldr_output_rejects_empty_tldr() {
        assert!(parse_tldr_output(r#"{"tldr": ""}"#).is_err());
    }

    #[test]
    fn validate_structured_summary_accepts_tldr_fallback() {
        assert!(is_valid_structured_summary(
            "**TLDR**: This is a valid fallback summary."
        ));
    }
}
