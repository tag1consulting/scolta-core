//! PII redaction for query analytics.
//!
//! Removes common sensitive data patterns (email, phone, SSN, credit card,
//! IPv4 and IPv6 address) before logging or sending queries to analytics
//! endpoints.
//!
//! The built-in patterns are matched by the small bounded matchers below, not
//! by a regex engine. They were `regex` crate patterns, and carrying that
//! engine for five fixed patterns was most of the browser WASM: the full
//! artifact still links it, but only for site-specific custom patterns, and
//! the slim artifact (built without the `custom-patterns` feature) does not
//! link it at all. Each matcher keeps its original pattern in its doc comment,
//! and the tests run every one against that pattern in the real `regex` crate,
//! so a change in behavior fails the build rather than reaching a log.

use crate::unicode_class::{is_digit, is_space, is_word};
#[cfg(feature = "custom-patterns")]
use regex::Regex;
#[cfg(feature = "custom-patterns")]
use std::cell::{Cell, RefCell};
#[cfg(feature = "custom-patterns")]
use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::str::FromStr;

/// Whether this build evaluates `custom_patterns`. False in the slim browser
/// artifact, which rejects them instead.
pub const CUSTOM_PATTERNS_SUPPORTED: bool = cfg!(feature = "custom-patterns");

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
/// silently reach `sanitize_query`. Without the `custom-patterns` feature it
/// cannot be constructed at all, so no pattern can be dropped unevaluated.
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    /// Compiled regex.
    #[cfg(feature = "custom-patterns")]
    pub regex: Regex,
    /// Replacement text (e.g., `"[PATIENT_ID]"`).
    pub replacement: String,
    #[cfg(not(feature = "custom-patterns"))]
    unconstructible: std::convert::Infallible,
}

/// Why [`SanitizationPattern::compile`] failed: the `regex` crate's parse
/// error when the `custom-patterns` feature is on.
#[cfg(feature = "custom-patterns")]
pub type PatternError = regex::Error;

/// Why [`SanitizationPattern::compile`] failed: without the
/// `custom-patterns` feature there is no engine to compile with.
#[cfg(not(feature = "custom-patterns"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError;

#[cfg(not(feature = "custom-patterns"))]
impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "custom patterns are not supported by this build of scolta-core \
             (the slim artifact); load the full artifact to use them",
        )
    }
}

impl SanitizationPattern {
    /// Validate and compile this pattern.
    ///
    /// Compiled regexes are cached per pattern string for the lifetime of the
    /// module instance, so repeated `sanitize_query` calls with the same
    /// config never recompile.
    ///
    /// # Errors
    /// Returns the `regex` crate's parse error when the pattern is invalid,
    /// and always errs in a build without the `custom-patterns` feature.
    pub fn compile(&self) -> Result<CompiledPattern, PatternError> {
        #[cfg(feature = "custom-patterns")]
        {
            Ok(CompiledPattern {
                regex: cached_custom_regex(&self.regex)?,
                replacement: self.replacement.clone(),
            })
        }
        #[cfg(not(feature = "custom-patterns"))]
        {
            Err(PatternError)
        }
    }
}

/// Distinct custom patterns kept compiled at once. A site configures a
/// handful; the bound only stops a caller that varies the pattern per call
/// from growing the cache for the life of the page.
#[cfg(feature = "custom-patterns")]
const CUSTOM_REGEX_CACHE_CAP: usize = 64;

#[cfg(feature = "custom-patterns")]
thread_local! {
    // WASM is single-threaded, so a thread_local map is effectively a
    // module-global cache there; on native (tests) each thread gets its own.
    static CUSTOM_REGEX_CACHE: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());
    static CUSTOM_REGEX_COMPILE_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(feature = "custom-patterns")]
fn cached_custom_regex(pattern: &str) -> Result<Regex, regex::Error> {
    CUSTOM_REGEX_CACHE.with(|cache| {
        if let Some(re) = cache.borrow().get(pattern) {
            return Ok(re.clone());
        }
        let re = Regex::new(pattern)?;
        CUSTOM_REGEX_COMPILE_COUNT.with(|c| c.set(c.get() + 1));
        let mut cache = cache.borrow_mut();
        if cache.len() >= CUSTOM_REGEX_CACHE_CAP {
            cache.clear();
        }
        cache.insert(pattern.to_string(), re.clone());
        Ok(re)
    })
}

/// Number of custom-pattern compilations (cache misses) on this thread.
/// Exposed so tests can assert that repeated calls do not recompile.
#[cfg(feature = "custom-patterns")]
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
    // Every built-in pattern needs a digit, `+`, `(`, `@` or `:`. Most
    // queries have none, and skip the passes entirely.
    if !query
        .chars()
        .any(|c| can_start_step_match(c) || c == '@' || c == ':')
    {
        return apply_custom_patterns(query.to_string(), &config.custom_patterns);
    }

    // The built-in passes share one decoded copy of the query; it is encoded
    // back to a string once, after the last of them.
    let mut text: Vec<char> = query.chars().collect();

    // Credit card before phone to avoid partial matches (16 digits vs 10).
    if config.redact_credit_card {
        replace_steps(&mut text, CREDIT_CARD, "[CC]");
    }

    if config.redact_ssn {
        replace_steps(&mut text, SSN, "[SSN]");
    }

    // Every address has an `@`; most queries do not.
    if config.redact_email && text.contains(&'@') {
        replace_matches(&mut text, "[EMAIL]", find_email);
    }

    if config.redact_phone {
        replace_steps(&mut text, PHONE, "[PHONE]");
    }

    if config.redact_ip {
        redact_ipv6(&mut text);
        replace_steps(&mut text, IPV4, "[IP]");
    }

    apply_custom_patterns(text.into_iter().collect(), &config.custom_patterns)
}

#[cfg(feature = "custom-patterns")]
fn apply_custom_patterns(mut text: String, patterns: &[CompiledPattern]) -> String {
    for pat in patterns {
        text = pat
            .regex
            .replace_all(&text, pat.replacement.as_str())
            .into_owned();
    }
    text
}

#[cfg(not(feature = "custom-patterns"))]
fn apply_custom_patterns(text: String, patterns: &[CompiledPattern]) -> String {
    // Without the feature a `CompiledPattern` cannot exist, so the list is
    // empty: nothing reaches here to be skipped.
    if let Some(pat) = patterns.first() {
        match pat.unconstructible {}
    }
    text
}

// ---------------------------------------------------------------------------
// Built-in patterns
//
// Each is a fixed sequence of steps with bounded repetition, matched with the
// same leftmost-first, greedy-then-backtrack priority the `regex` crate uses,
// so the match it reports at a position is the one that crate reported. With
// no unbounded repetition the work per start position is a small constant
// (at most a few dozen paths), so a whole pass is linear in the query.
// Email has an unbounded `+` and gets its own linear matcher below.
// ---------------------------------------------------------------------------

/// One element of a built-in pattern.
#[derive(Clone, Copy)]
enum Step {
    /// `\b`: a Unicode word boundary.
    Boundary,
    /// One character of the class.
    One(fn(char) -> bool),
    /// `x?`, greedy.
    Opt(fn(char) -> bool),
    /// `x{min,max}`, greedy.
    Rep(fn(char) -> bool, usize, usize),
    /// `(...)?`, greedy.
    Group(&'static [Step]),
    /// `(?:a|b|...)`, earlier branches first.
    Alt(&'static [&'static [Step]]),
}

use Step::{Alt, Boundary, Group, One, Opt, Rep};

fn is_hyphen(c: char) -> bool {
    c == '-'
}

fn is_dot(c: char) -> bool {
    c == '.'
}

/// `[\s\-]`
fn is_space_or_hyphen(c: char) -> bool {
    c == '-' || is_space(c)
}

/// `[-.\s]`
fn is_phone_separator(c: char) -> bool {
    c == '-' || c == '.' || is_space(c)
}

/// `\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b`
const CREDIT_CARD: &[Step] = &[
    Boundary,
    Rep(is_digit, 4, 4),
    Opt(is_space_or_hyphen),
    Rep(is_digit, 4, 4),
    Opt(is_space_or_hyphen),
    Rep(is_digit, 4, 4),
    Opt(is_space_or_hyphen),
    Rep(is_digit, 4, 4),
    Boundary,
];

/// US SSN as an explicit alternation over the three forms it is actually
/// written in: `\b(?:\d{3}-\d{2}-\d{4}|\d{3}\s\d{2}\s\d{4}|\d{9})\b`, that is
/// `123-45-6789`, `123 45 6789`, and the bare `123456789`.
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
const SSN: &[Step] = &[
    Boundary,
    Alt(&[
        &[
            Rep(is_digit, 3, 3),
            One(is_hyphen),
            Rep(is_digit, 2, 2),
            One(is_hyphen),
            Rep(is_digit, 4, 4),
        ],
        &[
            Rep(is_digit, 3, 3),
            One(is_space),
            Rep(is_digit, 2, 2),
            One(is_space),
            Rep(is_digit, 4, 4),
        ],
        &[Rep(is_digit, 9, 9)],
    ]),
    Boundary,
];

/// `(\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}`
const PHONE: &[Step] = &[
    Group(&[
        Opt(|c| c == '+'),
        One(|c| c == '1'),
        Opt(is_phone_separator),
    ]),
    Opt(|c| c == '('),
    Rep(is_digit, 3, 3),
    Opt(|c| c == ')'),
    Opt(is_phone_separator),
    Rep(is_digit, 3, 3),
    Opt(is_phone_separator),
    Rep(is_digit, 4, 4),
];

/// `\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b`
const IPV4: &[Step] = &[
    Boundary,
    Rep(is_digit, 1, 3),
    One(is_dot),
    Rep(is_digit, 1, 3),
    One(is_dot),
    Rep(is_digit, 1, 3),
    One(is_dot),
    Rep(is_digit, 1, 3),
    Boundary,
];

/// `\b` at `at`: exactly one side is a word character (`\w`), counting
/// outside the text as a non-word character.
fn is_boundary(text: &[char], at: usize) -> bool {
    let before = at > 0 && is_word(text[at - 1]);
    let after = text.get(at).is_some_and(|&c| is_word(c));
    before != after
}

fn class_at(text: &[char], at: usize, class: fn(char) -> bool) -> bool {
    text.get(at).is_some_and(|&c| class(c))
}

/// End of the highest-priority match of `steps` starting at `at` and
/// followed by a successful `then`, or `None`.
fn match_steps(
    steps: &[Step],
    text: &[char],
    at: usize,
    then: &dyn Fn(usize) -> Option<usize>,
) -> Option<usize> {
    let Some((&step, rest)) = steps.split_first() else {
        return then(at);
    };
    let next = |i: usize| match_steps(rest, text, i, then);
    match step {
        Boundary => is_boundary(text, at).then(|| next(at)).flatten(),
        One(class) => class_at(text, at, class).then(|| next(at + 1)).flatten(),
        Opt(class) => class_at(text, at, class)
            .then(|| next(at + 1))
            .flatten()
            .or_else(|| next(at)),
        Rep(class, min, max) => {
            let available = text[at.min(text.len())..]
                .iter()
                .take(max)
                .take_while(|&&c| class(c))
                .count();
            (min..=available).rev().find_map(|n| next(at + n))
        }
        Group(group) => match_steps(group, text, at, &next).or_else(|| next(at)),
        Alt(branches) => branches
            .iter()
            .find_map(|branch| match_steps(branch, text, at, &next)),
    }
}

/// Whether a match of any step pattern can start with `c`. Every one
/// consumes a digit, `+` or `(` first (a leading `\b` consumes nothing), so
/// other positions are skipped without running the matcher, and a query with
/// none of them skips the pass. Most queries have no digit at all.
fn can_start_step_match(c: char) -> bool {
    c == '+' || c == '(' || is_digit(c)
}

/// Replace every non-overlapping match of `steps`, leftmost first.
fn replace_steps(text: &mut Vec<char>, steps: &[Step], replacement: &str) {
    if !text.iter().any(|&c| can_start_step_match(c)) {
        return;
    }
    replace_matches(text, replacement, |chars, from| {
        (from..chars.len())
            .filter(|&start| can_start_step_match(chars[start]))
            .find_map(|start| match_steps(steps, chars, start, &Some).map(|end| (start, end)))
    });
}

/// Replace each match `find` reports, resuming the search at the end of the
/// previous one, as `regex::Regex::replace_all` does. Positions are char
/// indices. Every built-in pattern consumes at least one character, so each
/// match ends past where its search started. Leaves `text` untouched, with
/// no copy, when nothing matches.
fn replace_matches(
    text: &mut Vec<char>,
    replacement: &str,
    find: impl Fn(&[char], usize) -> Option<(usize, usize)>,
) {
    let Some(mut found) = find(text, 0) else {
        return;
    };
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    loop {
        let (start, end) = found;
        debug_assert!(at <= start && start < end);
        out.extend_from_slice(&text[at..start]);
        out.extend(replacement.chars());
        at = end;
        match find(text, at) {
            Some(next) => found = next,
            None => break,
        }
    }
    out.extend_from_slice(&text[at..]);
    *text = out;
}

/// End of the run of `class` characters starting at `at`.
fn run_end(text: &[char], at: usize, class: fn(char) -> bool) -> usize {
    at + text[at.min(text.len())..]
        .iter()
        .take_while(|&&c| class(c))
        .count()
}

/// `[a-zA-Z0-9._%+\-]`
fn is_email_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

/// `[a-zA-Z0-9.\-]`
fn is_email_domain(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-')
}

/// Leftmost match at or after `from` of
/// `[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}`.
///
/// The local part cannot contain `@`, so every start inside one run of
/// local-part characters reaches the same `@` and the same domain, and either
/// all of them match or none does: a failed run is skipped whole, which keeps
/// the scan linear where trying each start in turn would be quadratic.
fn find_email(text: &[char], from: usize) -> Option<(usize, usize)> {
    let mut start = from;
    while start < text.len() {
        let local_end = run_end(text, start, is_email_local);
        if local_end > start && text.get(local_end) == Some(&'@') {
            if let Some(end) = email_domain_end(text, local_end + 1) {
                return Some((start, end));
            }
        }
        start = local_end + 1;
    }
    None
}

/// Where `[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}` starting at `at` ends.
///
/// The greedy `+` gives the domain as much of the run as it can, so the match
/// takes the last `.` that has at least one domain character before it and
/// two or more letters after it, and then every letter that follows.
fn email_domain_end(text: &[char], at: usize) -> Option<usize> {
    let domain_end = run_end(text, at, is_email_domain);
    (at + 1..domain_end)
        .rev()
        .filter(|&dot| text[dot] == '.')
        .find_map(|dot| {
            let tld_end = run_end(text, dot + 1, |c| c.is_ascii_alphabetic());
            (tld_end - (dot + 1) >= 2).then_some(tld_end)
        })
}

/// The characters an IPv6 literal can be built from: `[0-9A-Fa-f.:]`.
///
/// Every maximal run of them is a candidate. Deliberately loose. Validity is
/// decided by [`Ipv6Addr::from_str`] in [`redact_ipv6`], not by a pattern,
/// because hand-rolling the IPv6 grammar into a regex is what produced the
/// two defects this replaced: a `{0,4}` group bound one short of the grammar,
/// which matched `1::2:3:4:5:6:7` only as far as `1::2:3:4:5:6` and left a
/// `:7` fragment behind, and a compressed branch that could not tell an
/// address from `namespace::member` syntax. A parser has the whole grammar
/// and cannot drift from it.
///
/// Runs without a colon (plain numbers, hex-looking words such as `decade`)
/// are discarded in [`longest_redactable_ipv6_prefix`] before any parse.
fn is_ipv6_candidate_char(c: char) -> bool {
    c.is_ascii_hexdigit() || c == '.' || c == ':'
}

/// Replace every IPv6 address in `text` with `[IP]`.
fn redact_ipv6(text: &mut Vec<char>) {
    // A candidate without a colon is never redacted.
    if !text.contains(&':') {
        return;
    }
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if !is_ipv6_candidate_char(text[at]) {
            out.push(text[at]);
            at += 1;
            continue;
        }
        let end = run_end(text, at, is_ipv6_candidate_char);
        let candidate = &text[at..end];
        // The candidate class is ASCII, so a prefix's byte length is its
        // length in chars.
        let redactable = if candidate.contains(&':') {
            longest_redactable_ipv6_prefix(&candidate.iter().collect::<String>())
        } else {
            None
        };
        match redactable {
            // The tail is whatever the address ran into: sentence
            // punctuation, a `:port`, the rest of a malformed address.
            Some(prefix) => {
                out.extend("[IP]".chars());
                out.extend_from_slice(&candidate[prefix..]);
            }
            None => out.extend_from_slice(candidate),
        }
        at = end;
    }
    *text = out;
}

/// Longest textual IPv6 address, in bytes: six 4-digit groups, six colons and
/// a dotted-quad tail, `ffff:ffff:ffff:ffff:ffff:ffff:255.255.255.255`. The
/// all-hex form is shorter at 39. No accepted form is longer, so a prefix past
/// this point cannot parse.
const MAX_IPV6_TEXT_LEN: usize = 45;

/// Byte length of the longest prefix of `candidate` that is a redactable IPv6
/// address, or `None` if no prefix is.
///
/// A prefix rather than the whole candidate because the candidate class
/// includes `.`, so `fe80::1.` (address at the end of a sentence) and
/// `1:2:3:4:5:6:7:8:9` (one group too many) both need a shorter prefix to
/// parse. Taking the longest keeps `::ffff:192.0.2.1` whole.
///
/// The scan is capped at [`MAX_IPV6_TEXT_LEN`] rather than run over the whole
/// candidate. Candidate runs are unbounded, and `from_str` is itself linear,
/// so scanning every prefix length made the cost quadratic in the length of a
/// hex-and-colon run — reachable from a search query, which is untrusted
/// text, in the function whose whole job is to handle it. Capping is
/// behavior-preserving: a longer prefix could never have parsed anyway.
fn longest_redactable_ipv6_prefix(candidate: &str) -> Option<usize> {
    if !candidate.contains(':') {
        return None;
    }
    // The candidate character class is ASCII, so every byte index is a char
    // boundary and slicing cannot panic.
    (1..=candidate.len().min(MAX_IPV6_TEXT_LEN))
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
    fn test_redact_ipv6_longest_textual_form() {
        // Pins MAX_IPV6_TEXT_LEN: this is the longest string `from_str`
        // accepts, so lowering the cap would stop redacting it.
        let addr = "ffff:ffff:ffff:ffff:ffff:ffff:255.255.255.255";
        assert_eq!(addr.len(), MAX_IPV6_TEXT_LEN);
        assert_eq!(
            sanitize_query(&format!("host {addr} down"), &all_redact()),
            "host [IP] down"
        );
    }

    #[test]
    fn test_long_hex_colon_run_is_not_quadratic() {
        // An unbounded prefix scan over this took milliseconds and grew
        // quadratically; capped, it is flat. Asserted for correctness — the
        // run contains no address and must survive untouched — with the length
        // chosen so a regression is felt in the suite runtime.
        //
        // Five-hex-digit groups are what make it addressless: no group may
        // exceed four, so no prefix parses. ("a:" repeated would NOT work —
        // `a:a:a:a:a:a:a:a` is a perfectly valid address.)
        let junk = "aaaaa:".repeat(20_000);
        let query = format!("q {junk} end");
        assert_eq!(sanitize_query(&query, &all_redact()), query);
    }

    #[test]
    fn test_redact_ipv6_after_long_junk_run() {
        // The cap must not stop a real address later in the query from being
        // found: each candidate run is scanned independently.
        let junk = "aaaaa:".repeat(5_000);
        let query = format!("{junk} then fe80::1");
        assert_eq!(
            sanitize_query(&query, &all_redact()),
            format!("{junk} then [IP]")
        );
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

    #[cfg(feature = "custom-patterns")]
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

    #[cfg(feature = "custom-patterns")]
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

/// The built-in matchers against the `regex` crate patterns they replaced.
///
/// The `regex` crate is a dev-dependency, so these run in every feature
/// combination, including the slim build that does not link it.
#[cfg(test)]
mod differential_tests {
    use super::*;
    use regex::Regex;
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::time::Duration;

    const CREDIT_CARD_RE: &str = r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b";
    const SSN_RE: &str = r"\b(?:\d{3}-\d{2}-\d{4}|\d{3}\s\d{2}\s\d{4}|\d{9})\b";
    const EMAIL_RE: &str = r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}";
    const PHONE_RE: &str = r"(\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}";
    const IPV4_RE: &str = r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b";
    const IPV6_CANDIDATE_RE: &str = r"[0-9A-Fa-f.:]+";

    /// Run a pass that works on decoded characters over a string.
    fn on_chars(input: &str, pass: impl FnOnce(&mut Vec<char>)) -> String {
        let mut text: Vec<char> = input.chars().collect();
        pass(&mut text);
        text.into_iter().collect()
    }

    /// The pre-matcher implementation, kept verbatim as the oracle.
    pub(super) fn reference_sanitize(query: &str, config: &SanitizationConfig) -> String {
        thread_local! {
            static COMPILED: std::cell::RefCell<HashMap<&'static str, Regex>> =
                std::cell::RefCell::new(HashMap::new());
        }
        let re = |p: &'static str| {
            COMPILED.with(|c| {
                c.borrow_mut()
                    .entry(p)
                    .or_insert_with(|| Regex::new(p).unwrap())
                    .clone()
            })
        };
        let mut result = query.to_string();
        if config.redact_credit_card {
            result = re(CREDIT_CARD_RE).replace_all(&result, "[CC]").into_owned();
        }
        if config.redact_ssn {
            result = re(SSN_RE).replace_all(&result, "[SSN]").into_owned();
        }
        if config.redact_email {
            result = re(EMAIL_RE).replace_all(&result, "[EMAIL]").into_owned();
        }
        if config.redact_phone {
            result = re(PHONE_RE).replace_all(&result, "[PHONE]").into_owned();
        }
        if config.redact_ip {
            result = re(IPV6_CANDIDATE_RE)
                .replace_all(&result, |caps: &regex::Captures| {
                    let candidate = &caps[0];
                    match longest_redactable_ipv6_prefix(candidate) {
                        Some(end) => format!("[IP]{}", &candidate[end..]),
                        None => candidate.to_string(),
                    }
                })
                .into_owned();
            result = re(IPV4_RE).replace_all(&result, "[IP]").into_owned();
        }
        result
    }

    /// Hand-picked inputs for the cases the classes make subtle: non-ASCII
    /// digits that `\d` accepts, Unicode whitespace that `\s` accepts, and
    /// letters, marks and connector punctuation that move a `\b`.
    pub(super) const UNICODE_CASES: &[&str] = &[
        // Full-width, Arabic-Indic and Devanagari digits.
        "card ４１１１ １１１１ １１１１ １１１１ end",
        "ssn ١٢٣-٤٥-٦٧٨٩ end",
        "ssn १२३ ४५ ६७८९ end",
        "ssn १२३४५६७८९ end",
        "call ५५५-८६७-५३०९ now",
        "ip ١٩٢.١٦٨.١.١ ok",
        "mixed 1२3-45-6७89 end",
        // NBSP, narrow NBSP, ideographic space and line separator.
        "ssn 123\u{a0}45\u{a0}6789",
        "card 4111\u{202f}1111\u{202f}1111\u{202f}1111",
        "ssn 123\u{3000}45\u{3000}6789",
        "call 555\u{2028}867\u{2028}5309",
        // Accented and CJK letters adjacent to digits: no `\b` between them.
        "é123456789",
        "123456789é",
        "中123456789",
        "123456789中",
        "中 123456789 中",
        "ü192.168.1.1",
        "192.168.1.1ü",
        "ip192.168.1.1",
        // Combining marks are `\w`, so they also suppress a boundary.
        "123456789\u{301}",
        "e\u{301}123456789",
        "4111111111111111\u{301}",
        // Connector punctuation and joiners.
        "_123456789",
        "123456789\u{203f}",
        "123456789\u{200d}",
        // Emoji are not `\w`, so they leave a boundary.
        "😀123456789😀",
        "😀4111-1111-1111-1111😀",
        "📞555-867-5309",
        // Emails next to non-ASCII text.
        "é.user@example.com",
        "user@exämple.com",
        "user@example.cöm",
        "中user@example.com中",
        "a@b.c.de-f",
        "a@b.cd.e",
        "x@.co",
        "@example.com",
        "a@b@c.com",
        "a@b.com@c.org",
        // Phone shapes.
        "+1 (555) 867-5309",
        "1-555-867-5309",
        "+15558675309",
        "1555867530",
        "(555)8675309",
        "555.867.5309",
        "11 555 867 5309",
        "+1+1-555-867-5309",
        // ZIP+4 and the namespace guards.
        "zip 12345-6789",
        "std::fmt::Debug db::add fe80::1 [2001:db8::1]:8080",
        "fe80::1.",
        "::ffff:192.0.2.1",
        "1.2.3.4.5",
        "1.2.3.4567",
        "1234.5.6.7",
        "",
        " ",
        "\u{feff}",
        "\u{fffd}\u{fffd}",
    ];

    /// A deterministic xorshift generator: no new dependency, same inputs on
    /// every run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            usize::try_from(self.next() % n as u64).unwrap()
        }
    }

    /// Characters weighted toward the ones the patterns care about, so random
    /// strings form near-miss addresses, numbers and boundaries often.
    const ALPHABET: &[char] = &[
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '1', '1', '5', '0', '9', '٣', '१', '４',
        '٠', '٩', '-', '-', '.', '.', ' ', ' ', '\u{a0}', '\t', '\u{3000}', '(', ')', '+', '@',
        '@', ':', ':', 'a', 'f', 'F', 'z', 'Q', 'é', '\u{301}', '中', '😀', '_', '%', ',', '/',
        '[', ']',
    ];

    fn random_string(rng: &mut Rng, max_len: usize) -> String {
        let len = rng.below(max_len + 1);
        (0..len)
            .map(|_| ALPHABET[rng.below(ALPHABET.len())])
            .collect()
    }

    /// Random strings, plus random strings spliced around well-formed values,
    /// so both near misses and real matches in odd contexts are covered.
    pub(super) fn fuzz_inputs(seed: u64, count: usize) -> Vec<String> {
        const SEEDS: &[&str] = &[
            "4111 1111 1111 1111",
            "123-45-6789",
            "123 45 6789",
            "123456789",
            "user.name+tag@example.co.uk",
            "+1 (555) 867-5309",
            "192.168.1.1",
            "2001:db8::1",
            "fe80::1",
        ];
        let mut rng = Rng(seed);
        (0..count)
            .map(|i| {
                if i % 2 == 0 {
                    random_string(&mut rng, 40)
                } else {
                    let seed = SEEDS[rng.below(SEEDS.len())];
                    format!(
                        "{}{}{}",
                        random_string(&mut rng, 6),
                        seed,
                        random_string(&mut rng, 6)
                    )
                }
            })
            .collect()
    }

    fn all_inputs() -> Vec<String> {
        let mut inputs: Vec<String> = UNICODE_CASES.iter().map(|s| s.to_string()).collect();
        inputs.extend(fuzz_inputs(0x5c01_7a5e_ed00_0001, 20_000));
        inputs
    }

    #[test]
    fn each_step_pattern_matches_its_regex() {
        let inputs = all_inputs();
        for (steps, pattern) in [
            (CREDIT_CARD, CREDIT_CARD_RE),
            (SSN, SSN_RE),
            (PHONE, PHONE_RE),
            (IPV4, IPV4_RE),
        ] {
            let re = Regex::new(pattern).unwrap();
            for input in &inputs {
                assert_eq!(
                    on_chars(input, |text| replace_steps(text, steps, "<>")),
                    re.replace_all(input, "<>"),
                    "pattern {pattern} on {input:?}"
                );
            }
        }
    }

    #[test]
    fn email_matcher_matches_its_regex() {
        let re = Regex::new(EMAIL_RE).unwrap();
        let mut inputs = all_inputs();
        // Email shapes are rare in the shared alphabet; add a denser one.
        let mut rng = Rng(0xe7a1_1000_0000_0003);
        const EMAIL_ALPHABET: &[char] =
            &['a', 'Z', '9', '.', '.', '-', '_', '+', '@', '@', ' ', 'é'];
        for _ in 0..20_000 {
            let len = rng.below(30);
            inputs.push(
                (0..len)
                    .map(|_| EMAIL_ALPHABET[rng.below(EMAIL_ALPHABET.len())])
                    .collect(),
            );
        }
        for input in &inputs {
            assert_eq!(
                on_chars(input, |text| replace_matches(text, "<>", find_email)),
                re.replace_all(input, "<>"),
                "email on {input:?}"
            );
        }
    }

    #[test]
    fn sanitize_query_matches_the_regex_implementation_for_every_flag_combination() {
        let inputs = all_inputs();
        for flags in 0u8..32 {
            let config = SanitizationConfig {
                redact_email: flags & 1 != 0,
                redact_phone: flags & 2 != 0,
                redact_ssn: flags & 4 != 0,
                redact_credit_card: flags & 8 != 0,
                redact_ip: flags & 16 != 0,
                custom_patterns: vec![],
            };
            for input in inputs.iter().step_by(if flags == 31 { 1 } else { 53 }) {
                assert_eq!(
                    sanitize_query(input, &config),
                    reference_sanitize(input, &config),
                    "flags {flags:05b} on {input:?}"
                );
            }
        }
    }

    #[test]
    fn non_ascii_digits_and_spaces_are_redacted_as_before() {
        // Pinned outright, not only differentially: these are the cases a
        // switch to an ASCII-only engine (regex-lite, a JavaScript `\d`)
        // silently stops redacting.
        for (query, expected) in [
            ("ssn १२३-४५-६७८९", "ssn [SSN]"),
            ("ssn ١٢٣٤٥٦٧٨٩", "ssn [SSN]"),
            ("card ４１１１ １１１１ １１１１ １１１１", "card [CC]"),
            ("ip ١٩٢.١٦٨.١.١", "ip [IP]"),
            ("ssn 123\u{a0}45\u{a0}6789", "ssn [SSN]"),
            ("call ५५५-८६७-५३०९", "call [PHONE]"),
            ("é123456789", "é123456789"),
            ("😀123456789😀", "😀[SSN]😀"),
        ] {
            assert_eq!(
                sanitize_query(query, &SanitizationConfig::default()),
                expected,
                "{query:?}"
            );
        }
    }

    /// Run `f` on another thread and fail if it is still running after
    /// `limit`. The deadline is enforced from outside the thread doing the
    /// work, which a timer inside it could not do.
    fn with_deadline(limit: Duration, f: impl FnOnce() + Send + 'static) {
        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            f();
            let _ = done.send(());
        });
        finished
            .recv_timeout(limit)
            .expect("sanitizer exceeded its deadline on hostile input");
    }

    #[test]
    fn hostile_inputs_finish_promptly() {
        let cases: Vec<String> = vec![
            // The 120 kB hex-and-colon run.
            "aaaaa:".repeat(20_000),
            // Long digit runs: every start position is a candidate for four
            // digit patterns.
            "1".repeat(120_000),
            "1-".repeat(60_000),
            "1.".repeat(60_000),
            "١".repeat(60_000),
            // Failing prefixes for the email scan: long local parts, many
            // `@`, long domains with no top-level domain.
            format!("{}@{}", "a".repeat(60_000), "b-".repeat(30_000)),
            "a@".repeat(60_000),
            "a.".repeat(60_000) + "@x",
            format!("a@{}", "b.1".repeat(40_000)),
            // Phone and card near misses.
            "+1 (555) 867-530 ".repeat(7_000),
            "4111 1111 1111 111 ".repeat(6_000),
        ];
        for case in cases {
            with_deadline(Duration::from_secs(5), move || {
                let _ = sanitize_query(&case, &SanitizationConfig::default());
            });
        }
    }

    #[cfg(feature = "custom-patterns")]
    #[test]
    fn hostile_custom_pattern_finishes_promptly() {
        // Catastrophic for a backtracking engine; linear in the regex crate.
        with_deadline(Duration::from_secs(5), || {
            let pattern = SanitizationPattern {
                regex: r"(a+)+$".to_string(),
                replacement: "[X]".to_string(),
            };
            let config = SanitizationConfig {
                custom_patterns: vec![pattern.compile().unwrap()],
                ..SanitizationConfig::default()
            };
            let query = format!("{}!", "a".repeat(50_000));
            assert_eq!(sanitize_query(&query, &config), query);
        });
    }

    #[cfg(feature = "custom-patterns")]
    #[test]
    fn custom_pattern_cache_is_bounded() {
        for i in 0..(CUSTOM_REGEX_CACHE_CAP * 3) {
            SanitizationPattern {
                regex: format!("bounded-{i}"),
                replacement: String::new(),
            }
            .compile()
            .unwrap();
        }
        CUSTOM_REGEX_CACHE.with(|cache| assert!(cache.borrow().len() <= CUSTOM_REGEX_CACHE_CAP));
    }

    #[cfg(not(feature = "custom-patterns"))]
    const _: () = assert!(!CUSTOM_PATTERNS_SUPPORTED);

    #[cfg(not(feature = "custom-patterns"))]
    #[test]
    fn slim_build_rejects_custom_patterns() {
        let err = SanitizationPattern {
            regex: r"\bMRN-\d{5}\b".to_string(),
            replacement: "[PATIENT_ID]".to_string(),
        }
        .compile()
        .unwrap_err();
        assert!(err.to_string().contains("not supported"));
    }
}

/// Writes and checks `tests/fixtures/sanitize-differential.json`, the cases
/// the browser tests replay against both built artifacts. Expected outputs
/// come from the `regex` crate reference, so the browser tests check the
/// shipped WASM, through its JavaScript string boundary, against the
/// implementation it replaced. Regenerate with
/// `SCOLTA_WRITE_FIXTURES=1 cargo test --lib sanitize_browser_fixture`.
#[cfg(test)]
mod browser_fixture {
    use super::differential_tests::{fuzz_inputs, reference_sanitize, UNICODE_CASES};
    use super::*;

    const PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/sanitize-differential.json"
    );

    fn render() -> String {
        let configs: [(&str, SanitizationConfig); 3] = [
            ("{}", SanitizationConfig::default()),
            (
                r#"{"redact_email":false,"redact_phone":false,"redact_ssn":false,"redact_credit_card":false}"#,
                SanitizationConfig {
                    redact_email: false,
                    redact_phone: false,
                    redact_ssn: false,
                    redact_credit_card: false,
                    ..SanitizationConfig::default()
                },
            ),
            (
                r#"{"redact_ip":false,"redact_ssn":false}"#,
                SanitizationConfig {
                    redact_ip: false,
                    redact_ssn: false,
                    ..SanitizationConfig::default()
                },
            ),
        ];
        let mut lines = Vec::new();
        let mut push = |query: &str, config_json: &str, config: &SanitizationConfig| {
            let config_value: serde_json::Value = serde_json::from_str(config_json).unwrap();
            lines.push(
                serde_json::json!({
                    "query": query,
                    "config": config_value,
                    "expected": reference_sanitize(query, config),
                })
                .to_string(),
            );
        };
        for query in UNICODE_CASES {
            for (json, config) in &configs {
                push(query, json, config);
            }
        }
        let (json, config) = &configs[0];
        for query in fuzz_inputs(0xb5_0e5e_d000_0002, 400) {
            push(&query, json, config);
        }
        format!(
            "{{\n \"$comment\": \"Generated by SCOLTA_WRITE_FIXTURES=1 cargo test --lib sanitize_browser_fixture. Expected outputs are the regex-crate implementation's. Do not edit by hand.\",\n \"cases\": [\n{}\n ]\n}}\n",
            lines.join(",\n")
        )
    }

    #[test]
    fn sanitize_browser_fixture_is_current() {
        let rendered = render();
        if std::env::var_os("SCOLTA_WRITE_FIXTURES").is_some() {
            std::fs::write(PATH, &rendered).unwrap();
        }
        let committed = std::fs::read_to_string(PATH).unwrap_or_default();
        assert!(
            committed == rendered,
            "tests/fixtures/sanitize-differential.json is stale; regenerate with \
             SCOLTA_WRITE_FIXTURES=1 cargo test --lib sanitize_browser_fixture"
        );
    }
}
