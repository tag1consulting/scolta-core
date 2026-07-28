//! PII redaction for query analytics.
//!
//! Removes common sensitive data patterns (email, phone, SSN, credit card,
//! IPv4 and IPv6 address) before logging or sending queries to analytics
//! endpoints. Custom patterns use the `regex` crate.

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
    /// Redact US Social Security Numbers, in hyphenated (`123-45-6789`),
    /// space-separated (`123 45 6789`) and bare (`123456789`) form.
    /// Default: true.
    pub redact_ssn: bool,
    /// Redact 16-digit credit card numbers. Default: true.
    pub redact_credit_card: bool,
    /// Redact IPv4 and IPv6 addresses. Default: true.
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
///
/// Pattern order is load-bearing: the longest digit runs are consumed first so
/// a shorter pattern cannot claim a prefix of them. Credit card (16 digits)
/// runs before SSN (9 digits), which runs before phone (10 digits) — the SSN
/// pattern is `\b`-anchored at both ends, so a 10-digit phone number is never
/// mistaken for a 9-digit SSN and vice versa.
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
        result = ipv6_regex().replace_all(&result, "[IP]").into_owned();
        result = ipv4_regex().replace_all(&result, "[IP]").into_owned();
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

/// US SSN in every form people actually type it: `123-45-6789`, `123 45 6789`,
/// and the bare `123456789`. Both separators are independently optional, so
/// mixed forms (`123-45 6789`) match too.
///
/// The `\b` at each end keeps this off longer digit runs: a 10-digit phone
/// number or a 16-digit card has no word boundary after the ninth digit, so
/// neither is ever redacted as an SSN. The flip side is that any bare
/// nine-digit number in a query is now redacted — deliberate, since that is
/// exactly how an unformatted SSN reaches the analytics log.
fn ssn_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{3}[-\s]?\d{2}[-\s]?\d{4}\b").unwrap())
}

fn cc_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b").unwrap())
}

fn ipv4_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap())
}

/// IPv6, as two alternatives: the uncompressed form and the `::`-compressed
/// form.
///
/// The uncompressed branch requires exactly eight groups, because that is the
/// only valid uncompressed length — a looser `{2,7}` repetition would also
/// swallow `12:34:56` (a duration) and MAC addresses, neither of which this
/// function is meant to redact.
///
/// Forms with a leading `::` (`::1`, `::ffff:…`) are deliberately not matched.
/// A branch for them cannot carry a leading `\b`, so it would also match the
/// `::method` tail of namespace syntax — `Path::add`, `Duration::add` — which
/// is ordinary query text on a developer-documentation site. In the
/// `::ffff:192.0.2.1` case the identifying half is still redacted by the IPv4
/// pass.
fn ipv6_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"\b(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}\b",
            r"|\b(?:[0-9a-fA-F]{1,4}:){1,6}:(?:[0-9a-fA-F]{1,4}:){0,4}[0-9a-fA-F]{1,4}\b",
        ))
        .unwrap()
    })
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

    // ---- SSN forms (issue #53) ----

    #[test]
    fn test_redact_ssn_space_separated() {
        let result = sanitize_query("ssn 123 45 6789", &all_redact());
        assert_eq!(result, "ssn [SSN]");
    }

    #[test]
    fn test_redact_ssn_bare_nine_digits() {
        let result = sanitize_query("ssn 123456789", &all_redact());
        assert_eq!(result, "ssn [SSN]");
    }

    #[test]
    fn test_redact_ssn_mixed_separators() {
        let result = sanitize_query("ssn 123-45 6789 filed", &all_redact());
        assert_eq!(result, "ssn [SSN] filed");
    }

    #[test]
    fn test_ssn_pattern_does_not_swallow_phone() {
        // 10 digits: no word boundary after the 9th, so the SSN pass must miss
        // it and the phone pass must claim the whole number.
        for query in ["call 555-867-5309 now", "call 5558675309 now"] {
            let result = sanitize_query(query, &all_redact());
            assert_eq!(result, "call [PHONE] now", "query: {query}");
        }
    }

    #[test]
    fn test_ssn_pattern_does_not_swallow_credit_card() {
        for query in [
            "card 4111 1111 1111 1111 please",
            "card 4111111111111111 please",
        ] {
            let result = sanitize_query(query, &all_redact());
            assert_eq!(result, "card [CC] please", "query: {query}");
        }
    }

    #[test]
    fn test_ssn_pattern_ignores_eight_digit_run() {
        let result = sanitize_query("order 12345678 done", &all_redact());
        assert_eq!(result, "order 12345678 done");
    }

    // ---- IPv6 (issue #53) ----

    #[test]
    fn test_redact_ipv6_full() {
        let result = sanitize_query(
            "host 2001:0db8:85a3:0000:0000:8a2e:0370:7334 down",
            &all_redact(),
        );
        assert_eq!(result, "host [IP] down");
    }

    #[test]
    fn test_redact_ipv6_compressed() {
        let result = sanitize_query("host fe80::1 down", &all_redact());
        assert_eq!(result, "host [IP] down");
    }

    #[test]
    fn test_redact_ipv6_partially_compressed() {
        let result = sanitize_query("host 2001:db8::8a2e:370:7334 down", &all_redact());
        assert_eq!(result, "host [IP] down");
    }

    #[test]
    fn test_redact_ipv6_uppercase_hex() {
        let result = sanitize_query("host FE80::ABCD down", &all_redact());
        assert_eq!(result, "host [IP] down");
    }

    #[test]
    fn test_redact_ipv6_bracketed_with_port() {
        let result = sanitize_query("connect [2001:db8::1]:8080", &all_redact());
        assert_eq!(result, "connect [[IP]]:8080");
    }

    #[test]
    fn test_ipv6_pattern_ignores_time_of_day() {
        // A three-group colon run is not valid IPv6; redacting it would cost
        // analytics fidelity on ordinary queries.
        let result = sanitize_query("clip at 12:34:56 mark", &all_redact());
        assert_eq!(result, "clip at 12:34:56 mark");
    }

    #[test]
    fn test_ipv6_pattern_ignores_namespace_syntax() {
        for query in ["std::fmt::Debug", "Path::add", "Duration::as_secs"] {
            let result = sanitize_query(query, &all_redact());
            assert_eq!(result, query, "query: {query}");
        }
    }

    #[test]
    fn test_ip_flag_disables_both_families() {
        let config = SanitizationConfig {
            redact_ip: false,
            ..SanitizationConfig::default()
        };
        let query = "hosts 192.168.1.1 and fe80::1";
        assert_eq!(sanitize_query(query, &config), query);
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
