//! Parse and process LLM expansion responses.
//!
//! LLM responses arrive in unpredictable formats — JSON arrays, markdown
//! code blocks, or plain text with newlines and commas. This module
//! handles all three with a graceful fallback chain, then filters the
//! results through the shared stop word list.

use crate::common;

/// Parse an LLM response containing expansion terms, using English stop words.
///
/// This is a convenience wrapper for [`parse_expansion_with_language`] that
/// defaults to English (`"en"`). Use it when no language context is available.
///
/// # Arguments
/// * `response` - Raw LLM response text
///
/// # Returns
/// Vector of cleaned expansion terms (may be empty if everything was filtered)
pub fn parse_expansion(response: &str) -> Vec<String> {
    parse_expansion_with_language(response, "en")
}

/// Parse an LLM response containing expansion terms for the given language.
///
/// Handles multiple input formats with a fallback chain:
/// 1. **Markdown-wrapped JSON**: `` ```json ["term1", "term2"] ``` ``
/// 2. **Bare JSON array**: `["term1", "term2", "term3"]`
/// 3. **Fallback**: Split by newlines/commas if JSON fails
///
/// All results are filtered through [`common::is_valid_term`] with the given
/// language's stop word list to remove stop words, very short strings, and
/// pure numbers.
///
/// # Arguments
/// * `response` - Raw LLM response text
/// * `language` - ISO 639-1 language code (e.g. `"en"`, `"de"`)
///
/// # Returns
/// Vector of cleaned expansion terms (may be empty if everything was filtered)
pub fn parse_expansion_with_language(response: &str, language: &str) -> Vec<String> {
    // Try to extract JSON from markdown code blocks first
    let json_text = extract_json_from_markdown(response).unwrap_or_else(|| response.to_string());

    // Try to parse as JSON array
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&json_text) {
        if let Some(array) = json_value.as_array() {
            let terms: Vec<String> = array
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| common::is_valid_term(s, language))
                .collect();

            // Return even if empty — the JSON was valid, so falling through
            // to the fallback parser would re-parse the JSON syntax as text
            // and produce garbage terms from brackets and quotes.
            return terms;
        }
    }

    // Fallback: split by newlines and commas
    fallback_parse_with_language(&json_text, language)
}

/// Extract JSON content from markdown code blocks.
///
/// Looks for `` ```json ... ``` `` or `` ``` ... ``` ``.
fn extract_json_from_markdown(text: &str) -> Option<String> {
    let trimmed = text.trim();

    // Look for ```json ... ```
    if let Some(start) = trimmed.find("```json") {
        let content_start = start + 7;
        if let Some(end) = trimmed[content_start..].find("```") {
            let content = &trimmed[content_start..content_start + end];
            return Some(content.trim().to_string());
        }
    }

    // Look for ``` ... ```
    if let Some(start) = trimmed.find("```") {
        let content_start = start + 3;
        if let Some(end) = trimmed[content_start..].find("```") {
            let content = &trimmed[content_start..content_start + end];
            return Some(content.trim().to_string());
        }
    }

    None
}

/// Fallback parsing when JSON fails, using the given language's stop words.
///
/// Splits by newlines and commas, cleans whitespace and quotes.
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
        let response = r#"["term1", "term2", "term3"]"#;
        let terms = parse_expansion(response);
        assert_eq!(terms.len(), 3);
        assert_eq!(terms[0], "term1");
        assert_eq!(terms[1], "term2");
        assert_eq!(terms[2], "term3");
    }

    #[test]
    fn test_parse_expansion_markdown_json() {
        let response = "```json\n[\"search term\", \"another term\"]\n```";
        let terms = parse_expansion(response);
        assert_eq!(terms.len(), 2);
        assert_eq!(terms[0], "search term");
        assert_eq!(terms[1], "another term");
    }

    #[test]
    fn test_parse_expansion_markdown_generic() {
        let response = "```\n[\"item1\", \"item2\"]\n```";
        let terms = parse_expansion(response);
        assert_eq!(terms.len(), 2);
    }

    #[test]
    fn test_parse_expansion_fallback() {
        let response = "term1\nterm2, term3";
        let terms = parse_expansion(response);
        assert_eq!(terms.len(), 3);
        assert!(terms.contains(&"term1".to_string()));
        assert!(terms.contains(&"term2".to_string()));
        assert!(terms.contains(&"term3".to_string()));
    }

    #[test]
    fn test_parse_expansion_filters_stop_words() {
        let response = r#"["the", "search term", "a", "test"]"#;
        let terms = parse_expansion(response);
        assert!(!terms.contains(&"the".to_string()));
        assert!(!terms.contains(&"a".to_string()));
        assert!(terms.contains(&"search term".to_string()));
        assert!(terms.contains(&"test".to_string()));
    }

    #[test]
    fn test_parse_expansion_filters_numbers() {
        let response = r#"["123", "term123", "test"]"#;
        let terms = parse_expansion(response);
        assert!(!terms.contains(&"123".to_string()));
        assert!(terms.contains(&"term123".to_string()));
        assert!(terms.contains(&"test".to_string()));
    }

    #[test]
    fn test_parse_expansion_filters_short() {
        let response = r#"["a", "ab", "abc"]"#;
        let terms = parse_expansion(response);
        assert!(!terms.iter().any(|t| t.len() < 2));
    }

    #[test]
    fn test_extract_json_from_markdown_json() {
        let text = "```json\n[\"term1\", \"term2\"]\n```";
        let extracted = extract_json_from_markdown(text);
        assert!(extracted.is_some());
        assert!(extracted.unwrap().contains("term1"));
    }

    #[test]
    fn test_extract_json_from_markdown_generic() {
        let text = "```\n[\"item1\"]\n```";
        let extracted = extract_json_from_markdown(text);
        assert!(extracted.is_some());
    }

    #[test]
    fn test_extract_json_from_markdown_no_code_block() {
        let text = "Just some text without code blocks";
        assert!(extract_json_from_markdown(text).is_none());
    }

    #[test]
    fn test_fallback_parse_newlines() {
        let terms = fallback_parse_with_language("term1\nterm2\nterm3", "en");
        assert_eq!(terms.len(), 3);
    }

    #[test]
    fn test_fallback_parse_commas() {
        let terms = fallback_parse_with_language("term1, term2, term3", "en");
        assert_eq!(terms.len(), 3);
    }

    #[test]
    fn test_fallback_parse_quoted() {
        let terms = fallback_parse_with_language(r#""term1", "term2""#, "en");
        assert_eq!(terms.len(), 2);
        assert_eq!(terms[0], "term1");
        assert_eq!(terms[1], "term2");
    }

    #[test]
    fn test_parse_expansion_with_whitespace() {
        let response = r#"[ "term1" , "term2" , "term3" ]"#;
        let terms = parse_expansion(response);
        assert_eq!(terms.len(), 3);
        assert_eq!(terms[0], "term1");
    }

    #[test]
    fn test_parse_expansion_with_language_de() {
        // "und" is a German stop word, "drupal" is not
        let response = r#"["und", "drupal", "suche"]"#;
        let terms = parse_expansion_with_language(response, "de");
        assert!(!terms.contains(&"und".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
        assert!(terms.contains(&"suche".to_string()));
    }

    #[test]
    fn test_parse_expansion_with_language_en_same_as_default() {
        let response = r#"["the", "search", "engine"]"#;
        let en_terms = parse_expansion_with_language(response, "en");
        let default_terms = parse_expansion(response);
        assert_eq!(en_terms, default_terms);
    }

    #[test]
    fn test_parse_expansion_unknown_lang_keeps_english_stop_words_as_terms() {
        // For unknown language, English stop words are NOT filtered
        let response = r#"["the", "search"]"#;
        let terms = parse_expansion_with_language(response, "xx");
        // "the" is 3 chars, not a stop word for unknown language → included
        assert!(terms.contains(&"the".to_string()));
        assert!(terms.contains(&"search".to_string()));
    }
}
