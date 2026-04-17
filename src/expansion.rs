//! Parse and process LLM expansion responses.
//!
//! Handles three input formats with a fallback chain, filters results through
//! stop word lists, applies generic-term filtering, and merges with existing
//! term sets.

use crate::common;
use crate::stop_words;

/// Configuration for `parse_expansion_with_config`.
///
/// All fields are optional — callers that do not supply a field get the default
/// behavior (same as calling `parse_expansion`).
#[derive(Debug, Clone, Default)]
pub struct ExpansionConfig {
    /// ISO 639-1 language code for stop word filtering. Default: `"en"`.
    pub language: String,
    /// Site-specific generic terms to filter from single-word results.
    /// When empty, generic filtering is disabled entirely.
    /// Example: `["team", "platform", "solution"]`
    pub generic_terms: Vec<String>,
    /// Remove single-word results that match `generic_terms`. Default: true
    /// (only relevant when `generic_terms` is non-empty).
    pub filter_single_word_generic: bool,
    /// Keep terms ≤4 characters regardless of generic status (acronyms like
    /// LLM, API, MoE). Default: true.
    pub keep_acronyms: bool,
    /// Keep terms containing an uppercase letter regardless of generic status
    /// (proper nouns like Drupal, GoLang). Default: true.
    pub keep_proper_nouns: bool,
    /// Minimum term length in characters. Default: 2.
    pub min_term_length: u32,
    /// Terms to merge into the output. Deduplicated case-insensitively against
    /// parsed terms; first occurrence's casing is preserved.
    pub existing_terms: Vec<String>,
}

impl ExpansionConfig {
    pub fn new(language: &str) -> Self {
        ExpansionConfig {
            language: language.to_string(),
            filter_single_word_generic: true,
            keep_acronyms: true,
            keep_proper_nouns: true,
            min_term_length: 2,
            ..Default::default()
        }
    }
}

/// Parse an LLM response into expansion terms using English stop words.
pub fn parse_expansion(response: &str) -> Vec<String> {
    parse_expansion_with_language(response, "en")
}

/// Parse an LLM response into expansion terms for the given language.
///
/// Fallback chain: markdown JSON block → bare JSON array → newline/comma split.
/// All results filtered through `common::is_valid_term`.
pub fn parse_expansion_with_language(response: &str, language: &str) -> Vec<String> {
    let cfg = ExpansionConfig::new(language);
    parse_expansion_with_config(response, &cfg)
}

/// Parse an LLM response with full configuration for filtering and merging.
///
/// # Generic term filtering
///
/// Only active when `config.generic_terms` is non-empty. Rules applied in order:
/// 1. Terms shorter than `min_term_length` are removed.
/// 2. Terms ≤4 characters pass if `keep_acronyms` is true.
/// 3. Terms with any uppercase letter pass if `keep_proper_nouns` is true.
/// 4. Single-word generic terms are removed.
/// 5. Multi-word phrases pass if at least one word is not a stop word and not generic.
///
/// # Term merging
///
/// When `existing_terms` is non-empty, parsed terms and existing terms are merged
/// (case-insensitive dedup, first occurrence's casing preserved).
pub fn parse_expansion_with_config(text: &str, config: &ExpansionConfig) -> Vec<String> {
    let language = if config.language.is_empty() {
        "en"
    } else {
        &config.language
    };

    let json_text = extract_json_from_markdown(text).unwrap_or_else(|| text.to_string());

    let parsed: Vec<String> = if let Ok(json_value) =
        serde_json::from_str::<serde_json::Value>(&json_text)
    {
        if let Some(array) = json_value.as_array() {
            array
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| common::is_valid_term(s, language))
                .collect()
        } else {
            fallback_parse_with_language(&json_text, language)
        }
    } else {
        fallback_parse_with_language(&json_text, language)
    };

    // Always apply min_term_length filter.
    let min_len = config.min_term_length as usize;
    let parsed: Vec<String> = parsed
        .into_iter()
        .filter(|t| t.chars().count() >= min_len)
        .collect();

    // Apply generic-term filtering (only active when generic_terms is non-empty).
    let filtered = if config.generic_terms.is_empty() {
        parsed
    } else {
        parsed
            .into_iter()
            .filter(|term| should_keep_term(term, config, language, min_len))
            .collect()
    };

    // Merge with existing_terms, deduplicating case-insensitively.
    if config.existing_terms.is_empty() {
        filtered
    } else {
        merge_dedup(filtered, &config.existing_terms)
    }
}

/// Decide whether to keep a term under generic-term filtering rules.
fn should_keep_term(
    term: &str,
    config: &ExpansionConfig,
    language: &str,
    min_len: usize,
) -> bool {
    let char_count = term.chars().count();

    // Remove below-minimum-length terms.
    if char_count < min_len {
        return false;
    }

    // Acronym exception: ≤3 chars pass unconditionally (LLM, API, MoE).
    if config.keep_acronyms && char_count <= 3 {
        return true;
    }

    // Proper noun exception: any uppercase letter passes unconditionally.
    if config.keep_proper_nouns && term.chars().any(|c| c.is_uppercase()) {
        return true;
    }

    let generic_lower: Vec<String> = config
        .generic_terms
        .iter()
        .map(|g| g.to_lowercase())
        .collect();

    let stop_words = stop_words::get_stop_words(language);

    let words: Vec<&str> = term.split_whitespace().collect();

    if words.len() == 1 {
        // Single word: remove if it matches a generic term.
        let lower = term.to_lowercase();
        !generic_lower.contains(&lower)
    } else {
        // Multi-word phrase: keep if at least one word is not a stop word and not generic.
        words.iter().any(|w| {
            let wl = w.to_lowercase();
            !stop_words.contains(&wl.as_str()) && !generic_lower.contains(&wl)
        })
    }
}

/// Merge two term lists, deduplicating case-insensitively.
///
/// First occurrence (from `primary`) wins when the same term appears in both.
fn merge_dedup(primary: Vec<String>, secondary: &[String]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result: Vec<String> = Vec::new();

    for term in primary.into_iter().chain(secondary.iter().cloned()) {
        let lower = term.to_lowercase();
        if seen.insert(lower) {
            result.push(term);
        }
    }

    result
}

fn extract_json_from_markdown(text: &str) -> Option<String> {
    let trimmed = text.trim();

    if let Some(start) = trimmed.find("```json") {
        let content_start = start + 7;
        if let Some(end) = trimmed[content_start..].find("```") {
            return Some(trimmed[content_start..content_start + end].trim().to_string());
        }
    }

    if let Some(start) = trimmed.find("```") {
        let content_start = start + 3;
        if let Some(end) = trimmed[content_start..].find("```") {
            return Some(trimmed[content_start..content_start + end].trim().to_string());
        }
    }

    None
}

fn fallback_parse_with_language(text: &str, language: &str) -> Vec<String> {
    text.split(['\n', ','])
        .map(|s| s.trim())
        .map(|s| s.trim_matches('"').trim_matches('\'').trim())
        .map(|s| s.to_string())
        .filter(|s| common::is_valid_term(s, language))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_expansion_json_array() {
        let terms = parse_expansion(r#"["term1", "term2", "term3"]"#);
        assert_eq!(terms, vec!["term1", "term2", "term3"]);
    }

    #[test]
    fn test_parse_expansion_markdown_json() {
        let terms = parse_expansion("```json\n[\"search term\", \"another term\"]\n```");
        assert_eq!(terms.len(), 2);
    }

    #[test]
    fn test_parse_expansion_fallback() {
        let terms = parse_expansion("term1\nterm2, term3");
        assert!(terms.contains(&"term1".to_string()));
    }

    #[test]
    fn test_parse_expansion_filters_stop_words() {
        let terms = parse_expansion(r#"["the", "search term", "a", "test"]"#);
        assert!(!terms.contains(&"the".to_string()));
        assert!(!terms.contains(&"a".to_string()));
        assert!(terms.contains(&"search term".to_string()));
    }

    #[test]
    fn test_parse_expansion_with_language_de() {
        let terms = parse_expansion_with_language(r#"["und", "drupal", "suche"]"#, "de");
        assert!(!terms.contains(&"und".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
    }

    #[test]
    fn test_generic_terms_single_word_removed() {
        let config = ExpansionConfig {
            language: "en".to_string(),
            generic_terms: vec!["team".to_string(), "platform".to_string()],
            filter_single_word_generic: true,
            keep_acronyms: true,
            keep_proper_nouns: true,
            min_term_length: 2,
            existing_terms: vec![],
        };

        let terms = parse_expansion_with_config(r#"["team", "drupal", "platform"]"#, &config);
        assert!(!terms.contains(&"team".to_string()));
        assert!(!terms.contains(&"platform".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
    }

    #[test]
    fn test_generic_terms_multi_word_kept_with_specific_word() {
        let config = ExpansionConfig {
            language: "en".to_string(),
            generic_terms: vec!["team".to_string()],
            filter_single_word_generic: true,
            keep_acronyms: true,
            keep_proper_nouns: true,
            min_term_length: 2,
            existing_terms: vec![],
        };

        // "drupal team" has "drupal" which is specific → keep
        let terms = parse_expansion_with_config(r#"["drupal team", "team"]"#, &config);
        assert!(terms.contains(&"drupal team".to_string()));
        assert!(!terms.contains(&"team".to_string()));
    }

    #[test]
    fn test_keep_acronyms() {
        let config = ExpansionConfig {
            language: "en".to_string(),
            generic_terms: vec!["llm".to_string(), "api".to_string()],
            filter_single_word_generic: true,
            keep_acronyms: true,
            keep_proper_nouns: true,
            min_term_length: 2,
            existing_terms: vec![],
        };

        // Even though "llm" and "api" are in generic_terms, they're ≤3 chars (acronym exception)
        let terms = parse_expansion_with_config(r#"["llm", "api", "integration"]"#, &config);
        assert!(terms.contains(&"llm".to_string()));
        assert!(terms.contains(&"api".to_string()));
    }

    #[test]
    fn test_keep_proper_nouns() {
        let config = ExpansionConfig {
            language: "en".to_string(),
            generic_terms: vec!["drupal".to_string()],
            filter_single_word_generic: true,
            keep_acronyms: true,
            keep_proper_nouns: true,
            min_term_length: 2,
            existing_terms: vec![],
        };

        // "Drupal" has uppercase → keep (even though "drupal" is in generic_terms)
        let terms = parse_expansion_with_config(r#"["Drupal", "drupal"]"#, &config);
        assert!(terms.contains(&"Drupal".to_string()));
        assert!(!terms.contains(&"drupal".to_string()));
    }

    #[test]
    fn test_existing_terms_merged() {
        let config = ExpansionConfig {
            language: "en".to_string(),
            generic_terms: vec![],
            filter_single_word_generic: false,
            keep_acronyms: true,
            keep_proper_nouns: true,
            min_term_length: 2,
            existing_terms: vec!["drupal".to_string(), "migration".to_string()],
        };

        let terms = parse_expansion_with_config(r#"["performance", "optimization"]"#, &config);
        assert!(terms.contains(&"drupal".to_string()));
        assert!(terms.contains(&"migration".to_string()));
        assert!(terms.contains(&"performance".to_string()));
    }

    #[test]
    fn test_existing_terms_dedup_case_insensitive() {
        let config = ExpansionConfig {
            language: "en".to_string(),
            generic_terms: vec![],
            filter_single_word_generic: false,
            keep_acronyms: true,
            keep_proper_nouns: true,
            min_term_length: 2,
            existing_terms: vec!["Drupal".to_string()],
        };

        let terms = parse_expansion_with_config(r#"["drupal", "migration"]"#, &config);
        // "drupal" from parsed comes first, "Drupal" from existing is a dup → skip
        let drupal_occurrences: Vec<_> = terms.iter().filter(|t| t.to_lowercase() == "drupal").collect();
        assert_eq!(drupal_occurrences.len(), 1);
        assert_eq!(drupal_occurrences[0], "drupal"); // parsed casing wins (first)
    }

    #[test]
    fn test_pure_merge_empty_text() {
        // Empty text + existing_terms = just pass-through existing_terms
        let config = ExpansionConfig {
            language: "en".to_string(),
            generic_terms: vec![],
            filter_single_word_generic: false,
            keep_acronyms: true,
            keep_proper_nouns: true,
            min_term_length: 2,
            existing_terms: vec!["drupal".to_string(), "migration".to_string()],
        };

        let terms = parse_expansion_with_config("", &config);
        assert!(terms.contains(&"drupal".to_string()));
        assert!(terms.contains(&"migration".to_string()));
    }

    #[test]
    fn test_min_term_length() {
        let config = ExpansionConfig {
            language: "en".to_string(),
            generic_terms: vec![],
            filter_single_word_generic: false,
            keep_acronyms: false,
            keep_proper_nouns: false,
            min_term_length: 5,
            existing_terms: vec![],
        };

        let terms = parse_expansion_with_config(r#"["ab", "abcd", "abcde", "abcdef"]"#, &config);
        assert!(!terms.contains(&"ab".to_string()));
        assert!(!terms.contains(&"abcd".to_string()));
        assert!(terms.contains(&"abcde".to_string()));
    }
}
