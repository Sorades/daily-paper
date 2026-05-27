use sha2::Digest;

/// Default system prompt for deep reading.
const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a research paper analyst. Given a paper's title, abstract, and selected text sections, produce a concise summary in the specified language.

The summary should:
1. Explain what the paper does
2. Describe the method or technical approach
3. State the key results or contributions
4. Explain why it might be relevant to the reader's interests
5. Mention notable institutions, authors, project URLs, or code links if available

Output a single cohesive paragraph. Do not produce bullet-point reading notes."#;

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

Please summarize this paper in {language}."#
    )
}

/// Build the system prompt (can be overridden by template file).
pub fn build_system_prompt(custom_template: Option<&str>) -> String {
    custom_template
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string())
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
    fn custom_template_overrides() {
        let prompt = build_system_prompt(Some("custom prompt"));
        assert_eq!(prompt, "custom prompt");
    }
}
