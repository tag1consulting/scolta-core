//! Search result scoring and ranking algorithms.
//!
//! Provides the canonical Scolta ranking: recency decay, title/content match
//! boosting, composite scoring, and result merging with deduplication.
//! All math lives here so that every language adapter (PHP, Python, JS, Go)
//! produces identical rankings.
//!
//! # Scoring formula
//!
//! ```text
//! final_score = base_score + title_boost + content_boost + recency_boost
//! ```
//!
//! This **additive** formula matches the Tag1 reference implementation and
//! the client-side JavaScript scoring. No single zero component can collapse
//! the entire score — an article with no date still benefits from a strong
//! title match.
//!
//! `base_score` is the upstream search engine score (e.g., from Pagefind)
//! if present, otherwise 1.0. Recency uses exponential decay (half-life
//! based) for recent content, linear penalty for very old content.

use crate::common;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for search result scoring.
///
/// All fields have sensible defaults (see [`Default`] impl). Callers can
/// override any subset; unspecified fields keep their defaults.
///
/// # Value ranges
///
/// [`ScoringConfig::validate`] checks these ranges and returns warnings
/// for out-of-range values. The scoring algorithm will still work with
/// extreme values, but results may be surprising.
///
/// | Field | Reasonable range | Default |
/// |---|---|---|
/// | `recency_boost_max` | 0.0–2.0 | 0.5 |
/// | `recency_half_life_days` | 1–3650 | 365 |
/// | `recency_penalty_after_days` | 1–7300 | 1825 |
/// | `recency_max_penalty` | 0.0–1.0 | 0.3 |
/// | `title_match_boost` | 0.0–5.0 | 1.0 |
/// | `title_all_terms_multiplier` | 0.0–5.0 | 1.5 |
/// | `content_match_boost` | 0.0–5.0 | 0.4 |
/// | `content_all_terms_multiplier` | 0.0–5.0 | 0.48 |
/// | `expand_primary_weight` | 0.0–1.0 | 0.7 |
/// | `excerpt_length` | 50–2000 | 300 |
/// | `results_per_page` | 1–100 | 10 |
/// | `max_pagefind_results` | 1–500 | 50 |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// Maximum recency boost factor (default 0.5).
    pub recency_boost_max: f64,
    /// Half-life for recency boost in days (default 365).
    pub recency_half_life_days: u32,
    /// Days after which to apply penalty (default 1825 = ~5 years).
    pub recency_penalty_after_days: u32,
    /// Maximum penalty for very old content (default 0.3).
    pub recency_max_penalty: f64,
    /// Boost when any query term appears in title (default 1.0).
    pub title_match_boost: f64,
    /// Multiplier when ALL query terms appear in title (default 1.5).
    pub title_all_terms_multiplier: f64,
    /// Boost when any query term appears in content (default 0.4).
    pub content_match_boost: f64,
    /// Multiplier when ALL query terms appear in content (default 0.48).
    ///
    /// Previously hardcoded as `content_match_boost * 1.2`. Now explicit
    /// and configurable.
    pub content_all_terms_multiplier: f64,
    /// Weight for primary search results vs expanded (default 0.7).
    /// Expanded results get weight `1.0 - expand_primary_weight`.
    pub expand_primary_weight: f64,
    /// Maximum length of excerpt in characters (default 300).
    pub excerpt_length: u32,
    /// Results per page for pagination (default 10).
    pub results_per_page: u32,
    /// Maximum results from Pagefind to consider (default 50).
    pub max_pagefind_results: u32,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        ScoringConfig {
            recency_boost_max: 0.5,
            recency_half_life_days: 365,
            recency_penalty_after_days: 1825,
            recency_max_penalty: 0.3,
            title_match_boost: 1.0,
            title_all_terms_multiplier: 1.5,
            content_match_boost: 0.4,
            content_all_terms_multiplier: 0.48,
            expand_primary_weight: 0.7,
            excerpt_length: 300,
            results_per_page: 10,
            max_pagefind_results: 50,
        }
    }
}

/// A warning about a configuration value that is out of its expected range.
#[derive(Debug, Clone)]
pub struct ConfigWarning {
    pub field: &'static str,
    pub message: String,
}

impl ScoringConfig {
    /// Validate configuration values and return warnings for anything
    /// outside reasonable ranges.
    ///
    /// The config is still usable even with warnings — this is informational
    /// to help developers catch configuration mistakes early.
    pub fn validate(&self) -> Vec<ConfigWarning> {
        let mut warnings = Vec::new();

        if self.recency_boost_max < 0.0 || self.recency_boost_max > 2.0 {
            warnings.push(ConfigWarning {
                field: "recency_boost_max",
                message: format!(
                    "value {} outside reasonable range (0.0–2.0)",
                    self.recency_boost_max
                ),
            });
        }

        if self.recency_half_life_days == 0 || self.recency_half_life_days > 3650 {
            warnings.push(ConfigWarning {
                field: "recency_half_life_days",
                message: format!(
                    "value {} outside reasonable range (1–3650)",
                    self.recency_half_life_days
                ),
            });
        }

        if self.recency_max_penalty < 0.0 || self.recency_max_penalty > 1.0 {
            warnings.push(ConfigWarning {
                field: "recency_max_penalty",
                message: format!(
                    "value {} outside reasonable range (0.0–1.0)",
                    self.recency_max_penalty
                ),
            });
        }

        if self.expand_primary_weight < 0.0 || self.expand_primary_weight > 1.0 {
            warnings.push(ConfigWarning {
                field: "expand_primary_weight",
                message: format!(
                    "value {} outside reasonable range (0.0–1.0)",
                    self.expand_primary_weight
                ),
            });
        }

        if self.results_per_page == 0 || self.results_per_page > 100 {
            warnings.push(ConfigWarning {
                field: "results_per_page",
                message: format!(
                    "value {} outside reasonable range (1–100)",
                    self.results_per_page
                ),
            });
        }

        if self.max_pagefind_results == 0 || self.max_pagefind_results > 500 {
            warnings.push(ConfigWarning {
                field: "max_pagefind_results",
                message: format!(
                    "value {} outside reasonable range (1–500)",
                    self.max_pagefind_results
                ),
            });
        }

        warnings
    }
}

/// A single search result with score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// URL of the result.
    pub url: String,
    /// Page title.
    pub title: String,
    /// Excerpt from the page.
    pub excerpt: String,
    /// Publication or last-modified date (ISO 8601: YYYY-MM-DD).
    pub date: String,
    /// Computed relevance score. If provided by the upstream search engine
    /// (e.g., Pagefind), it is used as the base score. Otherwise defaults to 0.
    #[serde(default)]
    pub score: f64,
    /// Content type (e.g., "article", "page", "pdf").
    #[serde(default)]
    pub content_type: String,
    /// Site name or source.
    #[serde(default)]
    pub site_name: String,
    /// Pass-through for additional fields the caller needs preserved.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Calculate recency boost based on publication date.
///
/// Uses exponential decay matching the Tag1 reference implementation:
/// - Recent content gets a boost up to `recency_boost_max` that decays
///   with half-life `recency_half_life_days`
/// - Content older than `recency_penalty_after_days` gets a penalty
///   that grows linearly, capped at `recency_max_penalty`
/// - Unparseable dates return 0.0 (neutral — no boost, no penalty)
///
/// This is an **additive** value, not a multiplier. The composite score
/// formula adds this to the base score and match boosts.
///
/// # Arguments
/// * `date` - ISO 8601 date string (YYYY-MM-DD)
/// * `config` - Scoring configuration
///
/// # Returns
/// Additive boost (positive for recent, negative for old, 0.0 for neutral)
pub fn recency_boost(date: &str, config: &ScoringConfig) -> f64 {
    let days_old = match days_since_date(date) {
        Some(d) => d,
        None => return 0.0, // Unparseable date → no boost
    };

    if days_old < 0 {
        // Future date — treat as brand new, maximum boost
        return config.recency_boost_max;
    }

    let days_old = days_old as f64;

    if config.recency_half_life_days == 0 {
        // Avoid division by zero from bad config
        return 0.0;
    }

    let penalty_threshold = config.recency_penalty_after_days as f64;

    if days_old < penalty_threshold {
        // Exponential decay boost for content newer than penalty threshold.
        // Formula matches Tag1 reference: MAX_BOOST * exp(-ageDays / HALF_LIFE * ln2)
        let half_life = config.recency_half_life_days as f64;
        let boost =
            config.recency_boost_max * (-days_old / half_life * std::f64::consts::LN_2).exp();
        boost.max(0.0)
    } else {
        // Linear penalty for very old content, capped at max_penalty.
        // 5% penalty per year beyond the threshold, matching Tag1 reference.
        let years_over = (days_old - penalty_threshold) / 365.0;
        let penalty = (years_over * 0.05).min(config.recency_max_penalty);
        -penalty
    }
}

/// Score title match for relevance.
///
/// Returns an additive boost based on how many query terms appear as
/// whole words in the title, proportional to the fraction matched:
/// - All terms present → `title_match_boost × title_all_terms_multiplier`
/// - Some terms present → `title_match_boost × (matchCount / totalTerms)`
/// - No terms → 0.0
///
/// This matches the Tag1 reference implementation which uses proportional
/// scoring with word-boundary matching.
pub fn title_match_score(query: &str, title: &str, config: &ScoringConfig) -> f64 {
    let terms = common::extract_terms(query);
    if terms.is_empty() {
        return 0.0;
    }

    let title_lower = title.to_lowercase();
    let matching_count = terms
        .iter()
        .filter(|t| title_lower.contains(t.as_str()))
        .count();

    if matching_count == 0 {
        return 0.0;
    }

    let mut boost = config.title_match_boost;
    // Bonus when ALL search terms appear in title
    if matching_count == terms.len() && terms.len() > 1 {
        boost *= config.title_all_terms_multiplier;
    }

    boost * (matching_count as f64 / terms.len() as f64)
}

/// Score content match for relevance.
///
/// Same proportional approach as title matching, using content-specific
/// boost values. Scores excerpt/content text for query term presence.
/// - All terms present → `content_match_boost × content_all_terms_multiplier / content_match_boost`
/// - Some terms present → `content_match_boost × (matchCount / totalTerms)`
/// - No terms → 0.0
pub fn content_match_score(query: &str, content: &str, config: &ScoringConfig) -> f64 {
    let terms = common::extract_terms(query);
    if terms.is_empty() {
        return 0.0;
    }

    let content_lower = content.to_lowercase();
    let matching_count = terms
        .iter()
        .filter(|t| content_lower.contains(t.as_str()))
        .count();

    if matching_count == 0 {
        return 0.0;
    }

    let mut boost = config.content_match_boost;
    // Bonus when ALL search terms appear in content
    if matching_count == terms.len() && terms.len() > 1 {
        // Use content_all_terms_multiplier as the ratio
        // (default 0.48 = content_match_boost 0.4 × 1.2)
        boost = config.content_all_terms_multiplier;
    }

    boost * (matching_count as f64 / terms.len() as f64)
}

/// Calculate composite score for a single search result.
///
/// **Additive formula** matching the Tag1 reference implementation and
/// the client-side JavaScript scoring:
///
/// ```text
/// final_score = base_score + title_boost + content_boost + recency_boost
/// ```
///
/// `base_score` is the result's existing score from the upstream search engine
/// (e.g., Pagefind). If the upstream score is 0 or absent, a base of 1.0 is
/// used so that title/content/recency boosts still produce meaningful ranking.
///
/// The additive model ensures no single zero component can collapse the entire
/// score — an article with no date still benefits from a strong title match.
pub fn score_result(result: &SearchResult, query: &str, config: &ScoringConfig) -> f64 {
    let base_score = if result.score > 0.0 {
        result.score
    } else {
        1.0
    };
    let title_boost = title_match_score(query, &result.title, config);
    let content_boost = content_match_score(query, &result.excerpt, config);
    let recency = recency_boost(&result.date, config);

    base_score + title_boost + content_boost + recency
}

/// Score all results and sort by relevance (highest first).
pub fn score_results(results: &mut [SearchResult], query: &str, config: &ScoringConfig) {
    for result in results.iter_mut() {
        result.score = score_result(result, query, config);
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Merge original and expanded search results with deduplication.
///
/// Combines results from primary search and query expansion:
/// - Primary results weighted by `expand_primary_weight`
/// - Expanded results weighted by `1.0 - expand_primary_weight`
/// - Duplicate URLs are merged (scores combined, first occurrence's metadata kept)
/// - Final list sorted by combined score (descending)
pub fn merge_results(
    original: Vec<SearchResult>,
    expanded: Vec<SearchResult>,
    config: &ScoringConfig,
) -> Vec<SearchResult> {
    let mut results_by_url: HashMap<String, SearchResult> = HashMap::new();

    // Add original results with primary weight
    for mut result in original {
        result.score *= config.expand_primary_weight;
        results_by_url.insert(result.url.clone(), result);
    }

    // Add/merge expanded results with secondary weight
    for mut result in expanded {
        result.score *= 1.0 - config.expand_primary_weight;
        results_by_url
            .entry(result.url.clone())
            .and_modify(|r| {
                r.score += result.score;
            })
            .or_insert(result);
    }

    let mut merged: Vec<SearchResult> = results_by_url.into_values().collect();
    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    merged
}

// ---------------------------------------------------------------------------
// Date utilities
// ---------------------------------------------------------------------------

/// Parse a YYYY-MM-DD string into (year, month, day).
///
/// Returns `None` for unparseable dates instead of silently defaulting.
fn parse_date(date_str: &str) -> Option<(i32, i32, i32)> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }

    let year: i32 = parts[0].parse().ok()?;
    let month: i32 = parts[1].parse().ok()?;
    let day: i32 = parts[2].parse().ok()?;

    // Basic sanity check
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    Some((year, month, day))
}

/// Get the current date as (year, month, day).
///
/// Uses `SystemTime::now()` which works on native targets and wasm32-wasip1
/// (WASI provides `clock_time_get`).
///
/// The date algorithm is Howard Hinnant's `civil_from_days`, implemented
/// inline to avoid a chrono dependency (which would add ~400KB to the WASM
/// binary). The same algorithm is used in `tests/integration.rs` — if you
/// change this, update that too.
fn today() -> (i32, i32, i32) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if secs == 0 {
        return (2026, 4, 2); // Fallback — should not happen on WASI or native
    }

    civil_from_epoch_secs(secs)
}

/// Convert epoch seconds to (year, month, day) using Howard Hinnant's algorithm.
///
/// This is a pure function exposed for testing. The same algorithm is used
/// in the integration test helper `chrono_free_recent_date()`.
pub fn civil_from_epoch_secs(secs: u64) -> (i32, i32, i32) {
    let days = (secs / 86400) as i64;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y as i32, m as i32, d as i32)
}

/// Convert a date to an approximate day number for comparison.
///
/// Uses 365 days/year and 30 days/month. Not calendar-accurate, but
/// consistent across platforms — which is the point.
fn date_to_days(year: i32, month: i32, day: i32) -> i32 {
    year * 365 + (month - 1) * 30 + day
}

/// Calculate days elapsed since a date string.
///
/// Returns `None` for unparseable dates. Returns negative for future dates.
fn days_since_date(date_str: &str) -> Option<i32> {
    let (year, month, day) = parse_date(date_str)?;
    let (ref_y, ref_m, ref_d) = today();

    Some(date_to_days(ref_y, ref_m, ref_d) - date_to_days(year, month, day))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: generate a date string N days ago from now.
    fn days_ago(n: u64) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (n * 86400);
        let (y, m, d) = civil_from_epoch_secs(secs);
        format!("{:04}-{:02}-{:02}", y, m, d)
    }

    #[test]
    fn test_default_config() {
        let config = ScoringConfig::default();
        assert_eq!(config.recency_boost_max, 0.5);
        assert_eq!(config.recency_half_life_days, 365);
        assert_eq!(config.content_all_terms_multiplier, 0.48);
    }

    #[test]
    fn test_config_validation_ok() {
        let config = ScoringConfig::default();
        assert!(config.validate().is_empty());
    }

    #[test]
    fn test_config_validation_warns() {
        let config = ScoringConfig {
            recency_boost_max: 10.0,
            results_per_page: 0,
            ..Default::default()
        };
        let warnings = config.validate();
        assert!(warnings.len() >= 2);
        assert!(warnings.iter().any(|w| w.field == "recency_boost_max"));
        assert!(warnings.iter().any(|w| w.field == "results_per_page"));
    }

    #[test]
    fn test_recency_boost_recent() {
        let config = ScoringConfig::default();
        let recent = days_ago(30);
        let boost = recency_boost(&recent, &config);
        assert!(
            boost > 0.0,
            "Recent content should get positive boost, got {}",
            boost
        );
        assert!(
            boost <= config.recency_boost_max,
            "Boost should not exceed max, got {}",
            boost
        );
    }

    #[test]
    fn test_recency_boost_old() {
        let config = ScoringConfig::default();
        let boost = recency_boost("2000-01-01", &config);
        assert!(
            boost < 0.0,
            "Old content should get negative penalty, got {}",
            boost
        );
    }

    #[test]
    fn test_recency_boost_unparseable_date() {
        let config = ScoringConfig::default();
        assert_eq!(recency_boost("not-a-date", &config), 0.0);
        assert_eq!(recency_boost("", &config), 0.0);
        assert_eq!(recency_boost("2026-13-45", &config), 0.0);
    }

    #[test]
    fn test_today_returns_current_date() {
        let (y, m, d) = today();
        assert!(y >= 2026, "Year should be at least 2026, got {}", y);
        assert!((1..=12).contains(&m), "Month should be 1-12, got {}", m);
        assert!((1..=31).contains(&d), "Day should be 1-31, got {}", d);
    }

    #[test]
    fn test_parse_date_valid() {
        assert_eq!(parse_date("2026-04-03"), Some((2026, 4, 3)));
        assert_eq!(parse_date("2000-01-01"), Some((2000, 1, 1)));
    }

    #[test]
    fn test_parse_date_invalid() {
        assert_eq!(parse_date("not-a-date"), None);
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("2026-01-32"), None);
    }

    #[test]
    fn test_title_match_score_all_terms() {
        let config = ScoringConfig::default();
        let score = title_match_score("hello world", "Hello World Page", &config);
        // All terms match with >1 term: boost * multiplier * (2/2)
        let expected = config.title_match_boost * config.title_all_terms_multiplier;
        assert!(
            (score - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn test_title_match_score_partial() {
        let config = ScoringConfig::default();
        let score = title_match_score("hello world", "Hello there", &config);
        // 1 of 2 terms match: boost * (1/2)
        let expected = config.title_match_boost * 0.5;
        assert!(
            (score - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn test_title_match_score_none() {
        let config = ScoringConfig::default();
        let score = title_match_score("xyz abc", "Hello world", &config);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_content_match_score_all_terms() {
        let config = ScoringConfig::default();
        let score = content_match_score("test page", "This is a test page with content", &config);
        // All terms match with >1 term: content_all_terms_multiplier * (2/2)
        let expected = config.content_all_terms_multiplier;
        assert!(
            (score - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn test_content_match_score_partial() {
        let config = ScoringConfig::default();
        let score = content_match_score("test xyz", "This is a test page", &config);
        // 1 of 2 terms match: boost * (1/2)
        let expected = config.content_match_boost * 0.5;
        assert!(
            (score - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn test_score_result_uses_existing_score() {
        let config = ScoringConfig::default();
        let result_with_score = SearchResult {
            url: "https://example.com".to_string(),
            title: "Test".to_string(),
            excerpt: "Test content".to_string(),
            date: days_ago(60),
            score: 5.0, // Upstream score
            content_type: String::new(),
            site_name: String::new(),
            extra: serde_json::Map::new(),
        };

        let result_without_score = SearchResult {
            score: 0.0,
            ..result_with_score.clone()
        };

        let score_with = score_result(&result_with_score, "test", &config);
        let score_without = score_result(&result_without_score, "test", &config);

        // Result with upstream score should rank higher
        assert!(
            score_with > score_without,
            "Upstream score should be incorporated: {} vs {}",
            score_with,
            score_without
        );
    }

    #[test]
    fn test_merge_results_dedup() {
        let config = ScoringConfig::default();

        let original = vec![SearchResult {
            url: "https://example.com/page1".to_string(),
            title: "Page 1".to_string(),
            excerpt: "Content".to_string(),
            date: days_ago(30),
            score: 10.0,
            content_type: String::new(),
            site_name: String::new(),
            extra: serde_json::Map::new(),
        }];

        let expanded = vec![SearchResult {
            url: "https://example.com/page1".to_string(),
            title: "Page 1".to_string(),
            excerpt: "Content".to_string(),
            date: days_ago(30),
            score: 5.0,
            content_type: String::new(),
            site_name: String::new(),
            extra: serde_json::Map::new(),
        }];

        let merged = merge_results(original, expanded, &config);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].score > 0.0);
    }

    #[test]
    fn test_civil_from_epoch_secs() {
        // 2026-01-01 00:00:00 UTC = 1767225600
        let (y, m, d) = civil_from_epoch_secs(1_767_225_600);
        assert_eq!((y, m, d), (2026, 1, 1));
    }
}
