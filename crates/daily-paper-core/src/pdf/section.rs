use crate::models::pdf::PaperSection;
use regex::Regex;

/// Find the largest byte index <= `max` that falls on a UTF-8 char boundary.
fn safe_char_boundary(text: &str, max: usize) -> usize {
    let mut boundary = max.min(text.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

/// Section header patterns commonly found in academic papers.
const SECTION_PATTERNS: &[&str] = &[
    r"(?i)^\s*(\d+\.?\s+)?(introduction|background)\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(related\s+work|prior\s+work)\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(method|methods|methodology|approach|model|architecture)\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(experiment|experiments|results|evaluation)\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(discussion|analysis)\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(conclusion|conclusions|summary)\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(abstract)\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(acknowledgment|acknowledgment|acknowledgement)s?\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(reference|references|bibliography)\s*$",
    r"(?i)^\s*(\d+\.?\s+)?(appendix|appendices)\s*$",
];

/// Normalize a section title to a canonical form.
fn normalize_title(title: &str) -> String {
    let lower = title.trim().to_lowercase();
    // Strip leading number
    let stripped = lower.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
    stripped.trim().to_string()
}

/// Parse section boundaries from extracted text.
///
/// Returns a list of PaperSection with byte offsets.
pub fn parse_sections(text: &str) -> Vec<PaperSection> {
    let patterns: Vec<Regex> = SECTION_PATTERNS
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect();

    let mut sections = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        for pattern in &patterns {
            if pattern.is_match(trimmed) {
                // Find byte offset of this line
                let byte_offset = text
                    .lines()
                    .take(line_idx)
                    .map(|l| l.len() + 1) // +1 for newline
                    .sum::<usize>();

                sections.push(PaperSection {
                    title: trimmed.to_string(),
                    normalized_title: normalize_title(trimmed),
                    start_byte: byte_offset,
                    end_byte: 0, // filled later
                });
                break;
            }
        }
    }

    // Fill end_byte: each section ends where the next one starts
    for i in 0..sections.len() {
        if i + 1 < sections.len() {
            sections[i].end_byte = sections[i + 1].start_byte;
        } else {
            sections[i].end_byte = text.len();
        }
    }

    sections
}

/// Select text sections for LLM input, prioritizing key sections.
///
/// Returns the selected text within the configured token/char budget.
/// Always includes the beginning of the paper (where authors/affiliations typically are).
pub fn select_sections_for_reading(
    text: &str,
    sections: &[PaperSection],
    max_chars: usize,
) -> String {
    // Priority sections for deep reading
    let priority = [
        "introduction",
        "method",
        "methods",
        "methodology",
        "approach",
        "model",
        "architecture",
        "experiment",
        "experiments",
        "results",
        "evaluation",
        "conclusion",
        "conclusions",
        "discussion",
    ];

    // Always include the beginning of the paper (first ~2000 chars)
    // This typically contains title, authors, affiliations, and abstract
    let header_chars = safe_char_boundary(text, 2000usize.min(max_chars / 3));
    let header_text = &text[..header_chars];

    let mut selected_sections: Vec<&PaperSection> = Vec::new();
    let mut total_chars = header_chars;

    // First pass: add priority sections (skip if they overlap with header)
    for p in &priority {
        for section in sections {
            if section.normalized_title.contains(p) && section.start_byte >= header_chars {
                let section_text = &text[section.start_byte..section.end_byte];
                if total_chars + section_text.len() <= max_chars {
                    selected_sections.push(section);
                    total_chars += section_text.len();
                }
            }
        }
    }

    // If we have room, add remaining sections (skip header area and references)
    if total_chars < max_chars {
        for section in sections {
            if !selected_sections.contains(&section) && section.start_byte >= header_chars {
                let section_text = &text[section.start_byte..section.end_byte];
                if total_chars + section_text.len() <= max_chars {
                    selected_sections.push(section);
                    total_chars += section_text.len();
                }
            }
        }
    }

    // Sort by original position
    selected_sections.sort_by_key(|s| s.start_byte);

    // Concatenate: header first, then sections
    let mut result = String::new();
    result.push_str(header_text);

    for section in selected_sections {
        let section_text = &text[section.start_byte..section.end_byte];
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(section_text);
    }

    // If no sections found and no header, take the first max_chars of text
    if result.is_empty() {
        let end = safe_char_boundary(text, max_chars);
        result = text[..end].to_string();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_sections() {
        let text = "Some preamble text.\n\n\
                    1 Introduction\n\
                    This is the intro.\n\n\
                    2 Methods\n\
                    We did stuff.\n\n\
                    3 Results\n\
                    Here are results.\n\n\
                    References\n\
                    [1] A paper.";

        let sections = parse_sections(text);
        assert!(sections.len() >= 3);

        let titles: Vec<&str> = sections
            .iter()
            .map(|s| s.normalized_title.as_str())
            .collect();
        assert!(titles.contains(&"introduction"));
        assert!(titles.contains(&"methods"));
        assert!(titles.contains(&"results"));
    }

    #[test]
    fn section_byte_offsets_valid() {
        let text = "preamble\n\n1 Introduction\nHello world\n\n2 Methods\nDone.";
        let sections = parse_sections(text);

        for section in &sections {
            assert!(section.start_byte < section.end_byte);
            assert!(section.end_byte <= text.len());
        }
    }

    #[test]
    fn select_sections_respects_budget() {
        let text = "a".repeat(1000)
            + "\n\n1 Introduction\n"
            + &"b".repeat(500)
            + "\n\n2 Methods\n"
            + &"c".repeat(500);
        let sections = parse_sections(&text);
        let selected = select_sections_for_reading(&text, &sections, 600);
        assert!(selected.len() <= 600);
    }

    #[test]
    fn select_fallback_when_no_sections() {
        let text = "just plain text without any section headers at all";
        let sections = parse_sections(text);
        let selected = select_sections_for_reading(text, &sections, 100);
        assert!(!selected.is_empty());
    }

    #[test]
    fn normalize_title_strips_number() {
        assert_eq!(normalize_title("1 Introduction"), "introduction");
        assert_eq!(normalize_title("3.2 Methods"), "methods");
        assert_eq!(normalize_title("  Abstract  "), "abstract");
    }
}
