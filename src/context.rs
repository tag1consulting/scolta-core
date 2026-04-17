//! Context extraction for LLM summarization.
//!
//! Extracts the most relevant portions of an article for use as LLM context,
//! combining a fixed intro with keyword-anchored snippets and merging
//! overlapping ranges.

use crate::common;

/// Configuration for context extraction.
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// Maximum total character length of the extracted context. Default: 6000.
    pub max_length: u32,
    /// Length of the mandatory intro section in characters. Default: 2000.
    pub intro_length: u32,
    /// Characters to include before and after each keyword match. Default: 500.
    pub snippet_radius: u32,
    /// Separator inserted between extracted sections. Default: `"\n\n[...]\n\n"`.
    pub separator: String,
}

impl Default for ContextConfig {
    fn default() -> Self {
        ContextConfig {
            max_length: 6000,
            intro_length: 2000,
            snippet_radius: 500,
            separator: "\n\n[...]\n\n".to_string(),
        }
    }
}

/// An input item for batch context extraction.
#[derive(Debug, Clone)]
pub struct ContextItem {
    pub content: String,
    pub url: String,
    pub title: String,
}

/// The result of extracting context from one item.
#[derive(Debug, Clone)]
pub struct ContextResult {
    pub url: String,
    pub title: String,
    pub context: String,
}

/// Extract the most relevant portion of `content` for the given `query`.
///
/// Algorithm:
/// 1. Return unchanged if content fits within `max_length`.
/// 2. Extract query terms; if none, return truncated-at-sentence content.
/// 3. Take the intro (first `intro_length` chars, truncated at sentence boundary).
/// 4. Find all keyword matches in the remainder; build ±`snippet_radius` ranges.
/// 5. Merge overlapping/adjacent ranges.
/// 6. Join intro + separator + snippets; trim to `max_length` at sentence boundary.
pub fn extract_context(content: &str, query: &str, config: &ContextConfig) -> String {
    let max_len = config.max_length as usize;
    let intro_len = config.intro_length as usize;
    let radius = config.snippet_radius as usize;
    let sep = &config.separator;

    if char_len(content) <= max_len {
        return content.to_string();
    }

    let terms = common::extract_terms(query, "en");
    if terms.is_empty() {
        return truncate_at_sentence(content, max_len).to_string();
    }

    // Extract intro at sentence boundary.
    let intro_raw = char_slice(content, 0, intro_len.min(char_len(content)));
    let intro = truncate_at_sentence(intro_raw, intro_len).to_string();

    // Work on the text after the intro.
    let intro_byte_end = intro.len();
    let remaining = &content[intro_byte_end..];

    // Find all keyword match ranges (byte offsets in `remaining`).
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let remaining_lower = remaining.to_lowercase();

    for term in &terms {
        let term_lower = term.to_lowercase();
        let mut search_from = 0usize;
        while search_from < remaining_lower.len() {
            match remaining_lower[search_from..].find(&term_lower) {
                None => break,
                Some(rel_pos) => {
                    let abs_pos = search_from + rel_pos;
                    // Expand by snippet_radius bytes, then adjust to word/char boundaries.
                    let start =
                        word_boundary_start(remaining, byte_sub(remaining, abs_pos, radius));
                    let end = word_boundary_end(
                        remaining,
                        byte_add(remaining, abs_pos + term_lower.len(), radius),
                    );
                    ranges.push((start, end));
                    // Advance by at least one character to prevent infinite loops.
                    search_from = abs_pos + term_lower.len().max(1);
                }
            }
        }
    }

    if ranges.is_empty() {
        // No matches found in the remainder — return truncated intro + all of remaining.
        let combined = format!("{}{}{}", intro, sep, remaining);
        return truncate_at_sentence(&combined, max_len).to_string();
    }

    // Sort and merge overlapping ranges.
    ranges.sort_by_key(|r| r.0);
    let merged = merge_ranges(ranges);

    // Build output.
    let mut result = intro.clone();
    for (s, e) in &merged {
        result.push_str(sep);
        result.push_str(&remaining[*s..*e]);
    }

    if char_len(&result) > max_len {
        return truncate_at_sentence(&result, max_len).to_string();
    }

    result
}

/// Extract context from multiple items in a single call.
pub fn batch_extract_context(
    items: Vec<ContextItem>,
    query: &str,
    config: &ContextConfig,
) -> Vec<ContextResult> {
    items
        .into_iter()
        .map(|item| ContextResult {
            context: extract_context(&item.content, query, config),
            url: item.url,
            title: item.title,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// String helpers (UTF-8 safe)
// ---------------------------------------------------------------------------

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Return a slice of `s` from character `start` up to character `end`.
fn char_slice(s: &str, start_chars: usize, end_chars: usize) -> &str {
    let start_byte = s
        .char_indices()
        .nth(start_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let end_byte = s
        .char_indices()
        .nth(end_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[start_byte..end_byte]
}

/// Subtract `n` bytes from `pos`, keeping the result on a char boundary.
fn byte_sub(s: &str, pos: usize, n: usize) -> usize {
    let raw = pos.saturating_sub(n);
    align_char_start(s, raw)
}

/// Add `n` bytes to `pos`, clamped to `s.len()` and on a char boundary.
fn byte_add(s: &str, pos: usize, n: usize) -> usize {
    let raw = (pos + n).min(s.len());
    align_char_end(s, raw)
}

/// Walk backward to the nearest char boundary.
fn align_char_start(s: &str, mut pos: usize) -> usize {
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Walk forward to the nearest char boundary.
fn align_char_end(s: &str, mut pos: usize) -> usize {
    while pos < s.len() && !s.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

/// Scan backward from `from` for a space; return the start of the word.
fn word_boundary_start(s: &str, from: usize) -> usize {
    let from = align_char_start(s, from);
    if let Some(sp) = s[..from].rfind(' ') {
        sp + 1
    } else {
        0
    }
}

/// Scan forward from `from` for a space; return its position.
fn word_boundary_end(s: &str, from: usize) -> usize {
    let from = align_char_end(s, from.min(s.len()));
    if from >= s.len() {
        return s.len();
    }
    match s[from..].find(' ') {
        Some(rel) => from + rel,
        None => s.len(),
    }
}

/// Truncate `s` at a sentence boundary no later than `max_chars` characters.
///
/// Scans backward through the last 200 characters looking for `. `, `! `, `? `.
/// Falls back to the last space if no sentence boundary is found within 200 chars.
pub fn truncate_at_sentence(s: &str, max_chars: usize) -> &str {
    if char_len(s) <= max_chars {
        return s;
    }

    // Find byte offset of max_chars.
    let max_byte = s
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());

    let truncated = &s[..max_byte];

    // Scan the last 200 characters of `truncated` for a sentence boundary.
    let scan_chars = 200usize;
    let scan_start_chars = max_chars.saturating_sub(scan_chars);
    let scan_start_byte = s
        .char_indices()
        .nth(scan_start_chars)
        .map(|(i, _)| i)
        .unwrap_or(0);

    let scan_slice = &truncated[scan_start_byte..];
    let mut best_end: Option<usize> = None;

    for (bi, ch) in scan_slice.char_indices() {
        if ch == '.' || ch == '!' || ch == '?' {
            let next = bi + ch.len_utf8();
            if scan_slice[next..].starts_with(' ') || next == scan_slice.len() {
                best_end = Some(scan_start_byte + next);
            }
        }
    }

    if let Some(end) = best_end {
        &s[..end]
    } else if let Some(sp) = truncated.rfind(' ') {
        &s[..sp]
    } else {
        truncated
    }
}

/// Merge overlapping or adjacent byte ranges (must be sorted by start).
fn merge_ranges(sorted: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in sorted {
        match merged.last_mut() {
            Some(last) if s <= last.1 => {
                last.1 = last.1.max(e);
            }
            _ => {
                merged.push((s, e));
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_context_short_content_returned_unchanged() {
        let cfg = ContextConfig::default();
        let content = "Short content.";
        assert_eq!(extract_context(content, "query", &cfg), content);
    }

    #[test]
    fn test_extract_context_no_terms_truncates() {
        let cfg = ContextConfig {
            max_length: 20,
            ..Default::default()
        };
        let content = "The quick brown fox jumps over the lazy dog and more words here.";
        let result = extract_context(content, "the", &cfg); // all stop words → no terms
        assert!(result.chars().count() <= 20);
    }

    #[test]
    fn test_extract_context_includes_intro() {
        let long = "A".repeat(3000) + " drupal " + &"B".repeat(3000);
        let cfg = ContextConfig {
            max_length: 6000,
            intro_length: 100,
            snippet_radius: 50,
            separator: "\n...\n".to_string(),
        };
        let result = extract_context(&long, "drupal", &cfg);
        // Result should include the intro (first 100 chars of A's)
        assert!(result.starts_with('A'));
    }

    #[test]
    fn test_extract_context_snippet_around_keyword() {
        let intro = "X".repeat(100);
        let body = format!("{}before drupal after{}", " ".repeat(600), " ".repeat(600));
        let content = intro + &body;
        let cfg = ContextConfig {
            max_length: 6000,
            intro_length: 50,
            snippet_radius: 30,
            separator: "|".to_string(),
        };
        let result = extract_context(&content, "drupal", &cfg);
        assert!(result.contains("drupal"));
    }

    #[test]
    fn test_truncate_at_sentence_at_period() {
        let s = "Hello world. This is extra content.";
        let result = truncate_at_sentence(s, 15);
        assert_eq!(result, "Hello world.");
    }

    #[test]
    fn test_truncate_at_sentence_fallback_to_space() {
        let s = "Hello worldextra content here yes";
        let result = truncate_at_sentence(s, 12);
        assert!(!result.ends_with(' '));
        assert!(result.len() <= 12);
    }

    #[test]
    fn test_merge_ranges_overlapping() {
        let ranges = vec![(0, 10), (5, 15), (20, 30)];
        let merged = merge_ranges(ranges);
        assert_eq!(merged, vec![(0, 15), (20, 30)]);
    }

    #[test]
    fn test_merge_ranges_adjacent() {
        let ranges = vec![(0, 10), (10, 20)];
        let merged = merge_ranges(ranges);
        assert_eq!(merged, vec![(0, 20)]);
    }

    #[test]
    fn test_batch_extract_context() {
        let cfg = ContextConfig::default();
        let items = vec![
            ContextItem {
                content: "Short content.".to_string(),
                url: "https://a.com".to_string(),
                title: "A".to_string(),
            },
            ContextItem {
                content: "Another short piece.".to_string(),
                url: "https://b.com".to_string(),
                title: "B".to_string(),
            },
        ];
        let results = batch_extract_context(items, "drupal", &cfg);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].url, "https://a.com");
        assert_eq!(results[1].url, "https://b.com");
    }
}
