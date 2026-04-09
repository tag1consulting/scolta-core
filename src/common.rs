//! Shared constants and utilities used across multiple modules.
//!
//! This module prevents duplication of stop word lists, term validation
//! logic, and other cross-cutting concerns. Both `scoring` and `expansion`
//! import from here rather than maintaining their own copies.

/// Common English stop words filtered from search queries and expansion terms.
///
/// This is the single source of truth for stop words across the crate.
/// Scoring term extraction and expansion term validation both use this list.
///
/// The list covers: articles, conjunctions, prepositions, pronouns,
/// auxiliary verbs, question words, and common filler words.
pub const STOP_WORDS: &[&str] = &[
    // Articles
    "a", "an", "the", // Conjunctions
    "and", "but", "or", "nor", // Common prepositions
    "about", "as", "at", "by", "during", "for", "from", "if", "in", "into", "of", "on", "out",
    "through", "to", "up", "with", // Pronouns
    "he", "i", "it", "she", "that", "these", "they", "this", "those", "we", "what", "which", "who",
    "you", // Auxiliary/modal verbs
    "are", "be", "can", "could", "did", "do", "does", "has", "have", "is", "may", "might",
    "should", "was", "were", "will", "would", // Question words
    "how", "when", "where", "why", // Common filler
    "also", "just", "more", "most", "no", "not", "only", "same", "than", "very",
];

/// Check whether a term is a stop word (case-insensitive).
///
/// # Arguments
/// * `term` - The term to check
///
/// # Returns
/// `true` if the term is a stop word
pub fn is_stop_word(term: &str) -> bool {
    STOP_WORDS.contains(&term.to_lowercase().as_str())
}

/// Check whether a term is valid for search use.
///
/// Filters out empty strings, very short strings (< 2 chars),
/// pure numbers, and stop words.
///
/// # Arguments
/// * `term` - The term to validate
///
/// # Returns
/// `true` if the term is usable for search
pub fn is_valid_term(term: &str) -> bool {
    if term.is_empty() || term.len() < 2 {
        return false;
    }

    if term.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    !is_stop_word(term)
}

/// Extract meaningful search terms from a query string.
///
/// Splits on whitespace, lowercases, filters stop words and
/// single-character terms.
///
/// # Arguments
/// * `query` - The raw query string
///
/// # Returns
/// Vector of cleaned, lowercased search terms
pub fn extract_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .filter(|term| !is_stop_word(term) && term.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stop_word() {
        assert!(is_stop_word("the"));
        assert!(is_stop_word("The"));
        assert!(is_stop_word("THE"));
        assert!(!is_stop_word("drupal"));
        assert!(!is_stop_word("search"));
    }

    #[test]
    fn test_is_valid_term() {
        assert!(is_valid_term("search"));
        assert!(is_valid_term("multi-word term"));
        assert!(is_valid_term("term123"));
        assert!(!is_valid_term("the"));
        assert!(!is_valid_term("a"));
        assert!(!is_valid_term(""));
        assert!(!is_valid_term("x"));
        assert!(!is_valid_term("123"));
    }

    #[test]
    fn test_extract_terms() {
        let terms = extract_terms("what is drupal performance");
        assert_eq!(terms, vec!["drupal", "performance"]);

        let terms = extract_terms("the a an");
        assert!(terms.is_empty());

        let terms = extract_terms("Hello World");
        assert_eq!(terms, vec!["hello", "world"]);
    }

    #[test]
    fn test_extract_terms_preserves_order() {
        let terms = extract_terms("performance drupal optimization");
        assert_eq!(terms, vec!["performance", "drupal", "optimization"]);
    }
}
