//! Shared utilities used across multiple modules.
//!
//! Provides language-aware stop word filtering, term validation, and term
//! extraction. Both `scoring` and `expansion` import from here rather than
//! maintaining their own copies.
//!
//! All public functions accept an ISO 639-1 `language` parameter so that
//! stop word filtering respects the content language. Custom stop word lists
//! can be layered on top via the `_with_custom` variants.

use crate::stop_words;

/// Check whether a term is a stop word for the given language (case-insensitive).
///
/// Looks up the stop word list for `language` from [`stop_words::get_stop_words`].
/// For unknown languages and CJK the list is empty, so this always returns
/// `false` for those codes.
///
/// # Arguments
/// * `term` - The term to check
/// * `language` - ISO 639-1 language code (e.g. `"en"`, `"de"`)
///
/// # Returns
/// `true` if the term is a stop word in the given language
pub fn is_stop_word(term: &str, language: &str) -> bool {
    let lower = term.to_lowercase();
    stop_words::get_stop_words(language).contains(&lower.as_str())
}

/// Check whether a term is a stop word, also testing a custom list.
///
/// Equivalent to [`is_stop_word`] but additionally checks `custom`, a
/// caller-supplied list (already lowercase) of extra stop words.
///
/// # Arguments
/// * `term` - The term to check
/// * `language` - ISO 639-1 language code
/// * `custom` - Additional stop words (case-insensitive comparison)
///
/// # Returns
/// `true` if the term is in the language stop list or the custom list
pub fn is_stop_word_with_custom(term: &str, language: &str, custom: &[String]) -> bool {
    if is_stop_word(term, language) {
        return true;
    }
    let lower = term.to_lowercase();
    custom.iter().any(|w| w.to_lowercase() == lower)
}

/// Check whether a term is valid for search use.
///
/// Filters out empty strings, very short strings (< 2 chars), pure numbers,
/// and language-specific stop words.
///
/// # Arguments
/// * `term` - The term to validate
/// * `language` - ISO 639-1 language code
///
/// # Returns
/// `true` if the term is usable for search
pub fn is_valid_term(term: &str, language: &str) -> bool {
    if term.is_empty() || term.len() < 2 {
        return false;
    }

    if term.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    !is_stop_word(term, language)
}

/// Check whether a term is valid, also excluding custom stop words.
///
/// Equivalent to [`is_valid_term`] but additionally checks `custom`.
///
/// # Arguments
/// * `term` - The term to validate
/// * `language` - ISO 639-1 language code
/// * `custom` - Additional stop words
///
/// # Returns
/// `true` if the term is usable for search
pub fn is_valid_term_with_custom(term: &str, language: &str, custom: &[String]) -> bool {
    if term.is_empty() || term.len() < 2 {
        return false;
    }

    if term.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    !is_stop_word_with_custom(term, language, custom)
}

/// Extract meaningful search terms from a query string.
///
/// Splits on whitespace, lowercases, and filters stop words and
/// single-character terms for the given language.
///
/// # Arguments
/// * `query` - The raw query string
/// * `language` - ISO 639-1 language code
///
/// # Returns
/// Vector of cleaned, lowercased search terms
pub fn extract_terms(query: &str, language: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .filter(|term| !is_stop_word(term, language) && term.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

/// Extract meaningful search terms, also excluding custom stop words.
///
/// Equivalent to [`extract_terms`] but additionally filters `custom`.
///
/// # Arguments
/// * `query` - The raw query string
/// * `language` - ISO 639-1 language code
/// * `custom` - Additional stop words
///
/// # Returns
/// Vector of cleaned, lowercased search terms
pub fn extract_terms_with_custom(query: &str, language: &str, custom: &[String]) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .filter(|term| !is_stop_word_with_custom(term, language, custom) && term.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stop_word_en() {
        assert!(is_stop_word("the", "en"));
        assert!(is_stop_word("The", "en"));
        assert!(is_stop_word("THE", "en"));
        assert!(!is_stop_word("drupal", "en"));
        assert!(!is_stop_word("search", "en"));
    }

    #[test]
    fn test_is_stop_word_de() {
        assert!(is_stop_word("der", "de"));
        assert!(is_stop_word("und", "de"));
        assert!(!is_stop_word("drupal", "de"));
    }

    #[test]
    fn test_is_stop_word_unknown_lang() {
        // Unknown language: nothing is a stop word
        assert!(!is_stop_word("the", "xx"));
        assert!(!is_stop_word("und", "xx"));
    }

    #[test]
    fn test_is_stop_word_with_custom() {
        let custom = vec!["scolta".to_string(), "pagefind".to_string()];
        assert!(is_stop_word_with_custom("the", "en", &custom));
        assert!(is_stop_word_with_custom("scolta", "en", &custom));
        assert!(is_stop_word_with_custom("Pagefind", "en", &custom));
        assert!(!is_stop_word_with_custom("drupal", "en", &custom));
    }

    #[test]
    fn test_is_valid_term_en() {
        assert!(is_valid_term("search", "en"));
        assert!(is_valid_term("multi-word term", "en"));
        assert!(is_valid_term("term123", "en"));
        assert!(!is_valid_term("the", "en"));
        assert!(!is_valid_term("a", "en"));
        assert!(!is_valid_term("", "en"));
        assert!(!is_valid_term("x", "en"));
        assert!(!is_valid_term("123", "en"));
    }

    #[test]
    fn test_is_valid_term_with_custom() {
        let custom = vec!["forbidden".to_string()];
        assert!(!is_valid_term_with_custom("forbidden", "en", &custom));
        assert!(is_valid_term_with_custom("allowed", "en", &custom));
    }

    #[test]
    fn test_extract_terms_en() {
        let terms = extract_terms("what is drupal performance", "en");
        assert_eq!(terms, vec!["drupal", "performance"]);

        let terms = extract_terms("the a an", "en");
        assert!(terms.is_empty());

        let terms = extract_terms("Hello World", "en");
        assert_eq!(terms, vec!["hello", "world"]);
    }

    #[test]
    fn test_extract_terms_de() {
        let terms = extract_terms("der schnelle braune Fuchs", "de");
        // "der" is a German stop word, but "schnelle", "braune", "fuchs" are not
        assert!(!terms.contains(&"der".to_string()));
        assert!(terms.contains(&"schnelle".to_string()));
        assert!(terms.contains(&"fuchs".to_string()));
    }

    #[test]
    fn test_extract_terms_preserves_order() {
        let terms = extract_terms("performance drupal optimization", "en");
        assert_eq!(terms, vec!["performance", "drupal", "optimization"]);
    }

    #[test]
    fn test_extract_terms_with_custom() {
        let custom = vec!["drupal".to_string()];
        let terms = extract_terms_with_custom("drupal performance optimization", "en", &custom);
        assert!(!terms.contains(&"drupal".to_string()));
        assert!(terms.contains(&"performance".to_string()));
        assert!(terms.contains(&"optimization".to_string()));
    }

    #[test]
    fn test_extract_terms_unknown_lang_no_filtering() {
        // Unknown language: no stop words filtered (except very short)
        let terms = extract_terms("the and or drupal", "xx");
        // "the", "and", "or" are not filtered for unknown language
        assert!(terms.contains(&"the".to_string()));
        assert!(terms.contains(&"and".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
        // "or" is 2 chars so passes length check, should be included
        assert!(terms.contains(&"or".to_string()));
    }
}
