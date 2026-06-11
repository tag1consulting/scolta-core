//! PII redaction for query analytics.
//!
//! Removes common sensitive data patterns (email, phone, SSN, credit card,
//! IP address) before logging or sending queries to analytics endpoints.
//! Custom patterns use the `regex` crate.

use regex::Regex;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::OnceLock;

/// A custom PII redaction pattern as supplied in config JSON (uncompiled).
#[derive(Debug, Clone)]
pub struct SanitizationPattern {
    /// Regex pattern string.
    pub regex: String,
    /// Replacement text (e.g., `"[PATIENT_ID]"`).
    pub replacement: String,
}

/// A custom pattern whose regex has been validated and compiled.
///
/// Constructed via [`SanitizationPattern::compile`], which is the only path
/// into [`SanitizationConfig::custom_patterns`] — an invalid pattern can never
/// silently reach `sanitize_query`.
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    /// Compiled regex.
    pub regex: Regex,
    /// Replacement text (e.g., `"[PATIENT_ID]"`).
    pub replacement: String,
}

impl SanitizationPattern {
    /// Validate and compile this pattern.
    ///
    /// Compiled regexes are cached per pattern string for the lifetime of the
    /// module instance, so repeated `sanitize_query` calls with the same
    /// config never recompile (built-in patterns get the same treatment via
    /// `OnceLock` below).
    ///
    /// # Errors
    /// Returns the `regex` crate's parse error when the pattern is invalid.
    pub fn compile(&self) -> Result<CompiledPattern, regex::Error> {
        Ok(CompiledPattern {
            regex: cached_custom_regex(&self.regex)?,
            replacement: self.replacement.clone(),
        })
    }
}

thread_local! {
    // WASM is single-threaded, so a thread_local map is effectively a
    // module-global cache there; on native (tests) each thread gets its own.
    static CUSTOM_REGEX_CACHE: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());
    static CUSTOM_REGEX_COMPILE_COUNT: Cell<usize> = const { Cell::new(0) };
}

fn cached_custom_regex(pattern: &str) -> Result<Regex, regex::Error> {
    CUSTOM_REGEX_CACHE.with(|cache| {
        if let Some(re) = cache.borrow().get(pattern) {
            return Ok(re.clone());
        }
        let re = Regex::new(pattern)?;
        CUSTOM_REGEX_COMPILE_COUNT.with(|c| c.set(c.get() + 1));
        cache.borrow_mut().insert(pattern.to_string(), re.clone());
        Ok(re)
    })
}

/// Number of custom-pattern compilations (cache misses) on this thread.
/// Exposed so tests can assert that repeated calls do not recompile.
pub fn custom_regex_compile_count() -> usize {
    CUSTOM_REGEX_COMPILE_COUNT.with(Cell::get)
}

/// Configuration for `sanitize_query`.
#[derive(Debug, Clone)]
pub struct SanitizationConfig {
    /// Redact email addresses. Default: true.
    pub redact_email: bool,
    /// Redact US phone numbers. Default: true.
    pub redact_phone: bool,
    /// Redact US Social Security Numbers. Default: true.
    pub redact_ssn: bool,
    /// Redact 16-digit credit card numbers. Default: true.
    pub redact_credit_card: bool,
    /// Redact IPv4 addresses. Default: true.
    pub redact_ip: bool,
    /// Additional site-specific redaction patterns, validated and compiled
    /// up front via [`SanitizationPattern::compile`].
    pub custom_patterns: Vec<CompiledPattern>,
}

impl Default for SanitizationConfig {
    fn default() -> Self {
        SanitizationConfig {
            redact_email: true,
            redact_phone: true,
            redact_ssn: true,
            redact_credit_card: true,
            redact_ip: true,
            custom_patterns: vec![],
        }
    }
}

/// Redact PII from a search query before analytics logging.
///
/// Applies the enabled built-in patterns and any `custom_patterns` in order.
/// Built-in patterns match Tag1's reference implementation exactly.
pub fn sanitize_query(query: &str, config: &SanitizationConfig) -> String {
    let mut result = query.to_string();

    // Credit card before phone to avoid partial matches (16 digits vs 10).
    if config.redact_credit_card {
        result = cc_regex().replace_all(&result, "[CC]").into_owned();
    }

    if config.redact_ssn {
        result = ssn_regex().replace_all(&result, "[SSN]").into_owned();
    }

    if config.redact_email {
        result = email_regex().replace_all(&result, "[EMAIL]").into_owned();
    }

    if config.redact_phone {
        result = phone_regex().replace_all(&result, "[PHONE]").into_owned();
    }

    if config.redact_ip {
        result = ip_regex().replace_all(&result, "[IP]").into_owned();
    }

    for pat in &config.custom_patterns {
        result = pat
            .regex
            .replace_all(&result, pat.replacement.as_str())
            .into_owned();
    }

    result
}

// ---------------------------------------------------------------------------
// Compiled regex singletons (compiled once, reused)
// ---------------------------------------------------------------------------

fn email_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap())
}

fn phone_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").unwrap())
}

fn ssn_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap())
}

fn cc_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b").unwrap())
}

fn ip_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_redact() -> SanitizationConfig {
        SanitizationConfig::default()
    }

    #[test]
    fn test_redact_email() {
        let result = sanitize_query("contact user@example.com today", &all_redact());
        assert!(!result.contains('@'));
        assert!(result.contains("[EMAIL]"));
    }

    #[test]
    fn test_redact_phone_us() {
        let result = sanitize_query("call 555-867-5309 now", &all_redact());
        assert!(result.contains("[PHONE]"));
        assert!(!result.contains("5309"));
    }

    #[test]
    fn test_redact_ssn() {
        let result = sanitize_query("my SSN is 123-45-6789", &all_redact());
        assert!(result.contains("[SSN]"));
        assert!(!result.contains("6789"));
    }

    #[test]
    fn test_redact_credit_card() {
        let result = sanitize_query("card 4111 1111 1111 1111 please", &all_redact());
        assert!(result.contains("[CC]"));
        assert!(!result.contains("1111 1111"));
    }

    #[test]
    fn test_redact_ip() {
        let result = sanitize_query("server at 192.168.1.1 port 80", &all_redact());
        assert!(result.contains("[IP]"));
        assert!(!result.contains("192.168"));
    }

    #[test]
    fn test_clean_query_unchanged() {
        let query = "drupal performance optimization";
        let result = sanitize_query(query, &all_redact());
        assert_eq!(result, query);
    }

    #[test]
    fn test_selective_redaction() {
        let config = SanitizationConfig {
            redact_email: true,
            redact_phone: false,
            redact_ssn: false,
            redact_credit_card: false,
            redact_ip: false,
            custom_patterns: vec![],
        };
        let query = "contact user@example.com or call 555-867-5309";
        let result = sanitize_query(query, &config);
        assert!(result.contains("[EMAIL]"));
        assert!(result.contains("555-867-5309")); // phone not redacted
    }

    #[test]
    fn test_custom_pattern() {
        let config = SanitizationConfig {
            redact_email: false,
            redact_phone: false,
            redact_ssn: false,
            redact_credit_card: false,
            redact_ip: false,
            custom_patterns: vec![SanitizationPattern {
                regex: r"\bMRN-\d{5}\b".to_string(),
                replacement: "[PATIENT_ID]".to_string(),
            }
            .compile()
            .unwrap()],
        };
        let result = sanitize_query("patient MRN-12345 admitted", &config);
        assert!(result.contains("[PATIENT_ID]"));
        assert!(!result.contains("MRN-12345"));
    }

    #[test]
    fn test_invalid_custom_pattern_is_compile_error() {
        let pattern = SanitizationPattern {
            regex: r"\b(MRN-\d{5}\b".to_string(), // unbalanced paren
            replacement: "[PATIENT_ID]".to_string(),
        };
        assert!(pattern.compile().is_err());
    }

    #[test]
    fn test_custom_pattern_compiled_once() {
        let pattern = SanitizationPattern {
            regex: r"\bCACHE-ONCE-\d{4}\b".to_string(),
            replacement: "[X]".to_string(),
        };
        pattern.compile().unwrap();
        let count_after_first = custom_regex_compile_count();
        pattern.compile().unwrap();
        pattern.compile().unwrap();
        assert_eq!(
            custom_regex_compile_count(),
            count_after_first,
            "repeated compiles of the same pattern must hit the cache"
        );
    }

    #[test]
    fn test_multiple_redactions_in_one_query() {
        let query = "email user@test.com ip 10.0.0.1 ssn 987-65-4321";
        let result = sanitize_query(query, &all_redact());
        assert!(result.contains("[EMAIL]"));
        assert!(result.contains("[IP]"));
        assert!(result.contains("[SSN]"));
    }
}
