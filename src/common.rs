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
    is_stop_word_lower(&term.to_lowercase(), language)
}

/// [`is_stop_word`] for a term the caller has already lowercased, avoiding a
/// second allocation on hot paths like [`extract_terms`].
fn is_stop_word_lower(term_lower: &str, language: &str) -> bool {
    stop_words::get_stop_words(language).contains(&term_lower)
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
    let lower = term.to_lowercase();
    is_stop_word_with_custom_lower(&lower, language, custom)
}

/// [`is_stop_word_with_custom`] for an already-lowercased term.
fn is_stop_word_with_custom_lower(term_lower: &str, language: &str, custom: &[String]) -> bool {
    is_stop_word_lower(term_lower, language)
        || custom.iter().any(|w| w.to_lowercase() == term_lower)
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

/// Parsed representation of a search query with phrase intent signals.
///
/// Produced by [`extract_query`] and [`extract_query_with_custom`]. Downstream
/// scoring functions use `is_phrase` and `forced_phrase` to decide whether to
/// apply phrase-proximity multipliers.
pub struct QueryInfo {
    /// Meaningful, lowercased terms after stop-word filtering.
    pub terms: Vec<String>,
    /// True when two or more terms remain after filtering (multi-word query).
    pub is_phrase: bool,
    /// True when the original query was wrapped in double-quotes (e.g.
    /// `"hello world"`), indicating the user expects an exact phrase match.
    pub forced_phrase: bool,
}

/// Parse a query string into terms plus phrase intent signals.
///
/// Detects quoted phrases (`"hello world"` → `forced_phrase = true`),
/// strips the quotes, and extracts the usual meaningful terms.
///
/// # Arguments
/// * `query` - The raw query string (may be wrapped in `"…"`)
/// * `language` - ISO 639-1 language code for stop-word filtering
pub fn extract_query(query: &str, language: &str) -> QueryInfo {
    let stripped = query.trim();
    let forced_phrase = stripped.starts_with('"') && stripped.ends_with('"') && stripped.len() > 2;
    let inner = if forced_phrase {
        &stripped[1..stripped.len() - 1]
    } else {
        stripped
    };
    let terms = extract_terms(inner, language);
    let is_phrase = terms.len() >= 2;
    QueryInfo {
        terms,
        is_phrase,
        forced_phrase,
    }
}

/// Parse a query string into terms plus phrase intent signals, also excluding
/// custom stop words.
///
/// Equivalent to [`extract_query`] but additionally filters `custom`.
pub fn extract_query_with_custom(query: &str, language: &str, custom: &[String]) -> QueryInfo {
    let stripped = query.trim();
    let forced_phrase = stripped.starts_with('"') && stripped.ends_with('"') && stripped.len() > 2;
    let inner = if forced_phrase {
        &stripped[1..stripped.len() - 1]
    } else {
        stripped
    };
    let terms = extract_terms_with_custom(inner, language, custom);
    let is_phrase = terms.len() >= 2;
    QueryInfo {
        terms,
        is_phrase,
        forced_phrase,
    }
}

/// Strip the punctuation a query word carries from its sentence: "sailboat?"
/// becomes "sailboat", "(drupal," becomes "drupal", "¿qué?" becomes "qué".
/// Only opening brackets and quotes are trimmed from
/// the front, so ".net", "-wordpress" and "$100" keep their leading character;
/// only closing punctuation is trimmed from the back, so "c++", "c#" and "50%"
/// are kept. Guillemets and the “ ‘ marks are trimmed from both ends, since
/// German opens with » or „ and closes with « or “, and French the reverse.
fn trim_term_punctuation(word: &str) -> &str {
    // Closures over `matches!` rather than char arrays: the array patterns
    // generated more code in the slim module for the same characters.
    word.trim_start_matches(|c| {
        matches!(
            c,
            '(' | '['
                | '{'
                | '"'
                | '\''
                | '\u{201C}'
                | '\u{2018}'
                | '\u{201E}'
                | '\u{201A}'
                | '\u{00AB}'
                | '\u{00BB}'
                | '\u{2039}'
                | '\u{203A}'
                | '\u{00BF}'
                | '\u{00A1}'
        )
    })
    .trim_end_matches(|c| {
        matches!(
            c,
            '.' | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | ')'
                | ']'
                | '}'
                | '"'
                | '\''
                | '\u{201D}'
                | '\u{2019}'
                | '\u{201C}'
                | '\u{2018}'
                | '\u{00BB}'
                | '\u{00AB}'
                | '\u{203A}'
                | '\u{2039}'
                | '\u{2026}'
        )
    })
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
        .map(trim_term_punctuation)
        // The query is already lowercased — skip the per-term re-lowercasing.
        .filter(|term| !is_stop_word_lower(term, language) && term.len() > 1)
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
        .map(trim_term_punctuation)
        // The query is already lowercased — skip the per-term re-lowercasing.
        .filter(|term| !is_stop_word_with_custom_lower(term, language, custom) && term.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_terms_trims_sentence_punctuation() {
        assert_eq!(
            extract_terms("Did Jeremy live on a sailboat?", "en"),
            vec!["jeremy", "live", "sailboat"]
        );
        // Each trimmed character, once.
        let trimmed = extract_terms(
            "(aa) [bb] {cc} \"dd\" 'ee' \u{201C}ff\u{201D} \u{2018}gg\u{2019} \u{201E}hh\u{201C} \u{201A}ii\u{2018} \
             \u{00AB}jj\u{00BB} \u{00BB}kk\u{00AB} \u{2039}ll\u{203A} \u{203A}mm\u{2039} \u{00BF}nn? \u{00A1}oo! \
             pp. qq, rr; ss: tt\u{2026}",
            "en",
        );
        let expected: Vec<String> = [
            "aa", "bb", "cc", "dd", "ee", "ff", "gg", "hh", "ii", "jj", "kk", "ll", "mm", "nn",
            "oo", "pp", "qq", "rr", "ss", "tt",
        ]
        .iter()
        .map(|t| t.to_string())
        .collect();
        assert_eq!(trimmed, expected);
        // What stays as it was.
        assert_eq!(
            extract_terms(
                "c++ c# node.js .net -wordpress $100 u.s. 50% foo( pre- x* ?!",
                "en"
            ),
            vec![
                "c++",
                "c#",
                "node.js",
                ".net",
                "-wordpress",
                "$100",
                "u.s",
                "50%",
                "foo(",
                "pre-",
                "x*"
            ]
        );
        assert_eq!(
            extract_terms_with_custom("news on sailboats?", "en", &["news".to_string()]),
            vec!["sailboats"]
        );
        // Trimmed before the stop word and length checks: "it?" is "it".
        // Runs of marks go too.
        assert_eq!(
            extract_terms(
                "really?! (drupal). sailboat... \u{00BF}\u{00AB}qu\u{00E9}\u{00BB}?",
                "en"
            ),
            vec!["really", "drupal", "sailboat", "qu\u{00E9}"]
        );
        assert!(extract_terms("what is it?", "en").is_empty());
        assert!(extract_terms_with_custom("what is it?", "en", &[]).is_empty());
    }

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
