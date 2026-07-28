//! PII redaction for query analytics.
//!
//! Removes common sensitive data patterns (email, phone, SSN, credit card,
//! IPv4 and IPv6 address) before logging or sending queries to analytics
//! endpoints. Custom patterns use the `regex` crate.

use regex::Regex;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::str::FromStr;
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
        result = redact_ipv6(&result);
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

/// US SSN as an explicit alternation over the three forms it is actually
/// written in: `123-45-6789`, `123 45 6789`, and the bare `123456789`.
///
/// The alternation is load-bearing, not stylistic. Writing this as one pattern
/// with independently optional separators (`\d{3}[-\s]?\d{2}[-\s]?\d{4}`) does
/// not express "3-2-4" at all — it expresses "nine digits with optional breaks
/// after the third and fifth", which also matches every 5+4 grouping, so ZIP+4
/// (`12345-6789`) and part numbers were redacted as SSNs. Each branch here
/// pins its own separator positions, so a 5+4 split matches no branch.
///
/// Mixed separators (`123-45 6789`) are deliberately not supported: no such
/// form was reported, and admitting it reintroduces the independent-optional
/// structure that caused the ZIP+4 collision.
///
/// The `\b` at each end keeps this off longer digit runs: a 10-digit phone
/// number or a 16-digit card has no word boundary after the ninth digit, so
/// neither is ever redacted as an SSN. The bare branch does mean any isolated
/// nine-digit run is redacted, including a 9-digit ZIP — unavoidable, since an
/// unformatted SSN is byte-identical to one, and that is the form the issue
/// asked for.
fn ssn_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:\d{3}-\d{2}-\d{4}|\d{3}\s\d{2}\s\d{4}|\d{9})\b").unwrap())
}

fn cc_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b").unwrap())
}

fn ipv4_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap())
}

/// Every maximal run of characters an IPv6 literal can be built from.
///
/// Deliberately loose. Validity is decided by [`Ipv6Addr::from_str`] in
/// [`redact_ipv6`], not by this pattern, because hand-rolling the IPv6 grammar
/// into a regex is what produced the two defects this replaced: a `{0,4}`
/// group bound one short of the grammar, which matched `1::2:3:4:5:6:7` only
/// as far as `1::2:3:4:5:6` and left a `:7` fragment behind, and a compressed
/// branch that could not tell an address from `namespace::member` syntax.
/// A parser has the whole grammar and cannot drift from it.
///
/// Runs without a colon (plain numbers, hex-looking words such as `decade`)
/// are discarded in [`longest_redactable_ipv6_prefix`] before any parse.
fn ipv6_candidate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[0-9A-Fa-f.:]+").unwrap())
}

/// Replace every IPv6 address in `text` with `[IP]`.
fn redact_ipv6(text: &str) -> String {
    ipv6_candidate_regex()
        .replace_all(text, |caps: &regex::Captures| {
            let candidate = &caps[0];
            match longest_redactable_ipv6_prefix(candidate) {
                // The tail is whatever the address ran into: sentence
                // punctuation, a `:port`, the rest of a malformed address.
                Some(end) => format!("[IP]{}", &candidate[end..]),
                None => candidate.to_string(),
            }
        })
        .into_owned()
}

/// Byte length of the longest prefix of `candidate` that is a redactable IPv6
/// address, or `None` if no prefix is.
///
/// A prefix rather than the whole candidate because the candidate class
/// includes `.`, so `fe80::1.` (address at the end of a sentence) and
/// `1:2:3:4:5:6:7:8:9` (one group too many) both need a shorter prefix to
/// parse. Taking the longest keeps `::ffff:192.0.2.1` whole.
fn longest_redactable_ipv6_prefix(candidate: &str) -> Option<usize> {
    if !candidate.contains(':') {
        return None;
    }
    // The candidate character class is ASCII, so every byte index is a char
    // boundary and slicing cannot panic.
    (1..=candidate.len())
        .rev()
        .find(|&end| is_redactable_ipv6(&candidate[..end]))
}

/// Whether `s` is an IPv6 address that can be told apart from source-code
/// namespace syntax.
///
/// Parsing is necessary but *not* sufficient. `abc::def`, `db::add`,
/// `ec::add` and `cafe::babe` are all syntactically valid IPv6 addresses, so
/// a parser accepts every one of them — and on a developer-documentation
/// search index that is live corpus, not a hypothetical. The second condition
/// separates the two readings: `namespace::member` can only ever produce a
/// single `::` and, being an identifier, contains no decimal digit. So a
/// candidate is redacted when it parses AND either
///
/// - it contains a digit (`fe80::1`, `::1`, `2001:db8::1`), or
/// - it contains a colon outside the `::` run (`dead:beef::cafe`), which no
///   namespace path can produce.
///
/// The trade is a digit-free two-group address such as `cafe::babe` passing
/// through unredacted. That form is a documentation example rather than a
/// host anyone is assigned: every real-world prefix (link-local `fe80::/10`,
/// global unicast `2000::/3`, loopback `::1`, multicast `ff02::/16`) carries a
/// digit in its very first group. The reverse trade — redacting `db::add` on a
/// Rust or C++ documentation site — is the one that destroys real queries, and
/// it is not detectable after the fact because the log only keeps `[IP]`.
///
/// Multi-level paths (`std::fmt::Debug`) need none of this: two `::` runs are
/// not valid IPv6, so the parser rejects them outright.
fn is_redactable_ipv6(s: &str) -> bool {
    if Ipv6Addr::from_str(s).is_err() {
        return false;
    }
    s.bytes().any(|b| b.is_ascii_digit()) || s.replace("::", "").contains(':')
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
    fn test_ssn_pattern_does_not_swallow_zip_plus_four() {
        // ZIP+4 is a 5+4 split. An SSN pattern built from independently
        // optional separators reads it as "nine digits with a break after the
        // fifth" and redacts it, taking address and store-locator queries with
        // it; the explicit 3-2-4 alternation cannot.
        for query in [
            "zip 12345-6789",
            "ship to 90210-1234",
            "part no 12345-6789",
            "zip 12345 6789",
        ] {
            let result = sanitize_query(query, &all_redact());
            assert_eq!(result, query, "query: {query}");
        }
    }

    #[test]
    fn test_ssn_pattern_does_not_accept_mixed_separators() {
        // Mixed separators are out of scope and their support is what made
        // ZIP+4 match. Pinned so a future "simplification" back to optional
        // separators fails here.
        let query = "ssn 123-45 6789 filed";
        assert_eq!(sanitize_query(query, &all_redact()), query);
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
        for query in [
            // Non-hex letters, or more than one `::`: rejected by the parser.
            "std::fmt::Debug",
            "Path::add",
            "Duration::as_secs",
            "Vec::dedup",
            // Hex-only identifiers on both sides. These ARE syntactically
            // valid IPv6 addresses, so the parser accepts them and only the
            // digit/single-colon check keeps them out of the redaction. This
            // is the live corpus on a Rust or C++ documentation site.
            "db::add docs",
            "abc::def namespace",
            "ec::add curve",
            "cafe::babe example",
            "dead::beef",
            "::add",
            "::acc",
        ] {
            let result = sanitize_query(query, &all_redact());
            assert_eq!(result, query, "query: {query}");
        }
    }

    #[test]
    fn test_redact_ipv6_leading_double_colon() {
        // Reachable now that a parser rather than a `\b`-anchored regex
        // decides validity: the digit check, not the pattern shape, is what
        // keeps `::add` out.
        for (query, expected) in [
            ("localhost ::1 up", "localhost [IP] up"),
            ("mapped ::ffff:192.0.2.1 up", "mapped [IP] up"),
        ] {
            let result = sanitize_query(query, &all_redact());
            assert_eq!(result, expected, "query: {query}");
        }
    }

    #[test]
    fn test_redact_ipv6_leaves_no_fragment() {
        // A hand-rolled group bound one short of the grammar matched this
        // only as far as `1::2:3:4:5:6` and left `:7` in the log.
        for (query, expected) in [
            ("host 1::2:3:4:5:6:7 down", "host [IP] down"),
            ("host 1:2:3:4:5:6:7:8 down", "host [IP] down"),
        ] {
            let result = sanitize_query(query, &all_redact());
            assert_eq!(result, expected, "query: {query}");
        }
    }

    #[test]
    fn test_redact_ipv6_at_end_of_sentence() {
        // The candidate class includes `.`, so the trailing stop must fall
        // outside the match rather than defeating it.
        let result = sanitize_query("the host is fe80::1.", &all_redact());
        assert_eq!(result, "the host is [IP].");
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
