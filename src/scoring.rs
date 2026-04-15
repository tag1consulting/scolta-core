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
/// | `recency_strategy` | "exponential"\|"linear"\|"step"\|"none"\|"custom" | "exponential" |
/// | `recency_curve` | sorted `[[days,boost],…]` | [] |
/// | `title_match_boost` | 0.0–5.0 | 1.0 |
/// | `title_all_terms_multiplier` | 0.0–5.0 | 1.5 |
/// | `content_match_boost` | 0.0–5.0 | 0.4 |
/// | `content_all_terms_multiplier` | 0.0–5.0 | 0.48 |
/// | `expand_primary_weight` | 0.0–1.0 | 0.7 |
/// | `excerpt_length` | 50–2000 | 300 |
/// | `results_per_page` | 1–100 | 10 |
/// | `max_pagefind_results` | 1–500 | 50 |
/// | `language` | ISO 639-1 code | "en" |
/// | `custom_stop_words` | list of lowercase tokens | [] |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// Maximum recency boost factor (default 0.5).
    pub recency_boost_max: f64,
    /// Half-life for recency boost in days (default 365).
    /// Also used as the step boundary for the `"step"` strategy.
    pub recency_half_life_days: u32,
    /// Days after which to apply penalty (default 1825 = ~5 years).
    pub recency_penalty_after_days: u32,
    /// Maximum penalty for very old content (default 0.3).
    pub recency_max_penalty: f64,
    /// Recency decay strategy. One of:
    /// - `"exponential"` — exponential decay (default, matches Tag1 reference)
    /// - `"linear"` — linear decay from max at day 0 to 0 at penalty threshold
    /// - `"step"` — full boost until `recency_half_life_days`, then 0
    /// - `"none"` — no recency adjustment (always 0.0)
    /// - `"custom"` — piecewise linear from `recency_curve` control points
    pub recency_strategy: String,
    /// Control points for the `"custom"` recency strategy.
    ///
    /// Each entry is `[days_old, boost_value]`. Points must be sorted
    /// ascending by `days_old`. Values outside the range are clamped to the
    /// nearest boundary point. Ignored for all other strategies.
    pub recency_curve: Vec<[f64; 2]>,
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
    /// ISO 639-1 language code for stop word filtering (default "en").
    pub language: String,
    /// Additional stop words beyond the language defaults (default empty).
    ///
    /// Comparison is case-insensitive. These are applied on top of the
    /// language stop word list, not instead of it.
    pub custom_stop_words: Vec<String>,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        ScoringConfig {
            recency_boost_max: 0.5,
            recency_half_life_days: 365,
            recency_penalty_after_days: 1825,
            recency_max_penalty: 0.3,
            recency_strategy: "exponential".to_string(),
            recency_curve: Vec::new(),
            title_match_boost: 1.0,
            title_all_terms_multiplier: 1.5,
            content_match_boost: 0.4,
            content_all_terms_multiplier: 0.48,
            expand_primary_weight: 0.7,
            excerpt_length: 300,
            results_per_page: 10,
            max_pagefind_results: 50,
            language: "en".to_string(),
            custom_stop_words: Vec::new(),
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

        let valid_strategies = ["exponential", "linear", "step", "none", "custom"];
        if !valid_strategies.contains(&self.recency_strategy.as_str()) {
            warnings.push(ConfigWarning {
                field: "recency_strategy",
                message: format!(
                    "unknown strategy '{}'; valid: exponential, linear, step, none, custom",
                    self.recency_strategy
                ),
            });
        }

        if self.recency_strategy == "custom" && self.recency_curve.is_empty() {
            warnings.push(ConfigWarning {
                field: "recency_curve",
                message: "recency_strategy is 'custom' but recency_curve is empty; will return 0.0"
                    .to_string(),
            });
        }

        if self.recency_curve.len() >= 2 {
            let unsorted = self.recency_curve.windows(2).any(|w| w[0][0] >= w[1][0]);
            if unsorted {
                warnings.push(ConfigWarning {
                    field: "recency_curve",
                    message: "recency_curve points must be sorted ascending by days_old"
                        .to_string(),
                });
            }
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
/// Dispatches to the strategy named in `config.recency_strategy`:
///
/// | Strategy | Behaviour |
/// |---|---|
/// | `"exponential"` | Exponential decay matching the Tag1 reference (default) |
/// | `"linear"` | Linear decay from max at day 0 to 0 at penalty threshold |
/// | `"step"` | Full boost until `recency_half_life_days`, then drops to 0 |
/// | `"none"` | Always 0.0 — disables recency entirely |
/// | `"custom"` | Piecewise-linear from `recency_curve` control points |
/// | unknown | Falls back to `"exponential"` |
///
/// All strategies apply the same old-content linear penalty once the content
/// age exceeds `recency_penalty_after_days` (except `"none"` and `"custom"`).
///
/// Returns 0.0 for unparseable dates (neutral — no boost, no penalty).
/// Returns `recency_boost_max` for future dates (brand-new content).
///
/// This is an **additive** value, not a multiplier.
///
/// # Arguments
/// * `date` - ISO 8601 date string (YYYY-MM-DD)
/// * `config` - Scoring configuration
///
/// # Returns
/// Additive boost (positive for recent, negative for old, 0.0 for neutral)
pub fn recency_boost(date: &str, config: &ScoringConfig) -> f64 {
    let days_since = match days_since_date(date) {
        Some(d) => d,
        None => return 0.0, // Unparseable date → neutral
    };

    if days_since < 0 {
        // Future date — treat as brand new, maximum boost
        return config.recency_boost_max;
    }

    let days_old = days_since as f64;

    match config.recency_strategy.as_str() {
        "linear" => recency_linear(days_old, config),
        "step" => recency_step(days_old, config),
        "none" => 0.0,
        "custom" => recency_custom(days_old, config),
        _ => recency_exponential(days_old, config), // "exponential" and unknown
    }
}

/// Exponential decay — matches the Tag1 reference implementation.
///
/// `MAX_BOOST × exp(-age / HALF_LIFE × ln2)` for content newer than
/// `recency_penalty_after_days`; linear penalty beyond that threshold.
fn recency_exponential(days_old: f64, config: &ScoringConfig) -> f64 {
    if config.recency_half_life_days == 0 {
        return 0.0;
    }

    let penalty_threshold = config.recency_penalty_after_days as f64;

    if days_old < penalty_threshold {
        let half_life = config.recency_half_life_days as f64;
        let boost =
            config.recency_boost_max * (-days_old / half_life * std::f64::consts::LN_2).exp();
        boost.max(0.0)
    } else {
        recency_old_penalty(days_old, penalty_threshold, config)
    }
}

/// Linear decay — from `recency_boost_max` at day 0 to 0.0 at the penalty
/// threshold; linear penalty beyond that threshold.
fn recency_linear(days_old: f64, config: &ScoringConfig) -> f64 {
    let penalty_threshold = config.recency_penalty_after_days as f64;

    if days_old < penalty_threshold {
        let fraction = 1.0 - (days_old / penalty_threshold);
        (config.recency_boost_max * fraction).max(0.0)
    } else {
        recency_old_penalty(days_old, penalty_threshold, config)
    }
}

/// Step function — full boost until `recency_half_life_days`, drops to 0.0
/// until the penalty threshold, then applies the linear old-content penalty.
fn recency_step(days_old: f64, config: &ScoringConfig) -> f64 {
    let half_life = config.recency_half_life_days as f64;
    let penalty_threshold = config.recency_penalty_after_days as f64;

    if days_old < half_life {
        config.recency_boost_max
    } else if days_old < penalty_threshold {
        0.0
    } else {
        recency_old_penalty(days_old, penalty_threshold, config)
    }
}

/// Piecewise-linear interpolation over `config.recency_curve` control points.
///
/// Each point is `[days_old, boost_value]`. Values before the first point or
/// after the last point are clamped to the boundary values. Returns 0.0 if
/// the curve is empty.
fn recency_custom(days_old: f64, config: &ScoringConfig) -> f64 {
    let curve = &config.recency_curve;

    if curve.is_empty() {
        return 0.0;
    }

    // Clamp to boundaries
    if days_old <= curve[0][0] {
        return curve[0][1];
    }
    if days_old >= curve[curve.len() - 1][0] {
        return curve[curve.len() - 1][1];
    }

    // Find the surrounding segment and interpolate
    for w in curve.windows(2) {
        let (x0, y0) = (w[0][0], w[0][1]);
        let (x1, y1) = (w[1][0], w[1][1]);
        if days_old >= x0 && days_old <= x1 {
            let span = x1 - x0;
            if span == 0.0 {
                return y0;
            }
            let t = (days_old - x0) / span;
            return y0 + t * (y1 - y0);
        }
    }

    0.0
}

/// Shared old-content linear penalty: 5% per year beyond the threshold,
/// capped at `recency_max_penalty`. Used by exponential, linear, and step.
fn recency_old_penalty(days_old: f64, penalty_threshold: f64, config: &ScoringConfig) -> f64 {
    let years_over = (days_old - penalty_threshold) / 365.0;
    let penalty = (years_over * 0.05).min(config.recency_max_penalty);
    -penalty
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
///
/// This is a thin wrapper that calls [`extract_terms`] then delegates to
/// [`title_match_score_with_terms`]. Prefer `_with_terms` when scoring many
/// results for the same query to avoid repeated term extraction.
pub fn title_match_score(query: &str, title: &str, config: &ScoringConfig) -> f64 {
    let terms = common::extract_terms(query, &config.language);
    title_match_score_with_terms(&terms, title, config)
}

/// Score title match using pre-extracted terms.
///
/// Same logic as [`title_match_score`] but takes terms that have already been
/// extracted by [`common::extract_terms`]. Use this when scoring many results
/// against the same query to avoid extracting terms on every call.
pub fn title_match_score_with_terms(terms: &[String], title: &str, config: &ScoringConfig) -> f64 {
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
///
/// This is a thin wrapper that calls [`extract_terms`] then delegates to
/// [`content_match_score_with_terms`]. Prefer `_with_terms` when scoring many
/// results for the same query to avoid repeated term extraction.
pub fn content_match_score(query: &str, content: &str, config: &ScoringConfig) -> f64 {
    let terms = common::extract_terms(query, &config.language);
    content_match_score_with_terms(&terms, content, config)
}

/// Score content match using pre-extracted terms.
///
/// Same logic as [`content_match_score`] but takes terms that have already
/// been extracted by [`common::extract_terms`]. Use this when scoring many
/// results against the same query to avoid extracting terms on every call.
pub fn content_match_score_with_terms(
    terms: &[String],
    content: &str,
    config: &ScoringConfig,
) -> f64 {
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
///
/// This is a thin wrapper; prefer [`score_result_with_terms`] when scoring
/// many results for the same query.
pub fn score_result(result: &SearchResult, query: &str, config: &ScoringConfig) -> f64 {
    let terms = common::extract_terms(query, &config.language);
    score_result_with_terms(result, &terms, config)
}

/// Calculate composite score using pre-extracted terms.
///
/// Same logic as [`score_result`] but takes terms that have already been
/// extracted by [`common::extract_terms`]. Use this (via [`score_results`])
/// to avoid extracting terms once per result.
pub fn score_result_with_terms(
    result: &SearchResult,
    terms: &[String],
    config: &ScoringConfig,
) -> f64 {
    let base_score = if result.score > 0.0 {
        result.score
    } else {
        1.0
    };
    let title_boost = title_match_score_with_terms(terms, &result.title, config);
    let content_boost = content_match_score_with_terms(terms, &result.excerpt, config);
    let recency = recency_boost(&result.date, config);

    base_score + title_boost + content_boost + recency
}

/// Score all results and sort by relevance (highest first).
///
/// Extracts query terms once before the loop and reuses them for each result,
/// avoiding redundant [`common::extract_terms`] calls.
pub fn score_results(results: &mut [SearchResult], query: &str, config: &ScoringConfig) {
    let terms = if config.custom_stop_words.is_empty() {
        common::extract_terms(query, &config.language)
    } else {
        common::extract_terms_with_custom(query, &config.language, &config.custom_stop_words)
    };
    for result in results.iter_mut() {
        result.score = score_result_with_terms(result, &terms, config);
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
/// In the browser (`wasm32-unknown-unknown`): uses `js_sys::Date::now()`.
/// On native (test runner): uses `SystemTime::now()`.
///
/// The date algorithm is Howard Hinnant's `civil_from_days`, implemented
/// inline to avoid a chrono dependency (which would add ~400KB to the WASM
/// binary).
#[cfg(target_arch = "wasm32")]
fn today() -> (i32, i32, i32) {
    let millis = js_sys::Date::now();
    let secs = (millis / 1000.0) as u64;
    if secs == 0 {
        return (2026, 4, 2);
    }
    civil_from_epoch_secs(secs)
}

#[cfg(not(target_arch = "wasm32"))]
fn today() -> (i32, i32, i32) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if secs == 0 {
        return (2026, 4, 2);
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

/// Convert a calendar date to a day number using Howard Hinnant's
/// `days_from_civil` algorithm.
///
/// This is the inverse of [`civil_from_epoch_secs`]: it maps a proleptic
/// Gregorian calendar date to the number of days since 1970-01-01 (Unix
/// epoch). Handles leap years and month-length differences exactly.
///
/// Using the Hinnant algorithm here ensures that `days_since_date()` and
/// `today()` (which uses `civil_from_epoch_secs`) are consistent inverses
/// of each other — no approximation error accumulates.
fn date_to_days(year: i32, month: i32, day: i32) -> i32 {
    let y = if month <= 2 {
        year as i64 - 1
    } else {
        year as i64
    };
    let m = if month <= 2 {
        month as i64 + 9
    } else {
        month as i64 - 3
    };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * m as u32 + 2) / 5 + day as u32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe as i64 - 719468) as i32
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

    // --- Recency strategy tests ---

    #[test]
    fn test_recency_strategy_exponential_is_default() {
        let config = ScoringConfig::default();
        assert_eq!(config.recency_strategy, "exponential");
        let boost = recency_boost(&days_ago(30), &config);
        assert!(boost > 0.0);
    }

    #[test]
    fn test_recency_strategy_none() {
        let config = ScoringConfig {
            recency_strategy: "none".to_string(),
            ..Default::default()
        };
        assert_eq!(recency_boost(&days_ago(1), &config), 0.0);
        assert_eq!(recency_boost(&days_ago(3650), &config), 0.0);
        // Note: future dates still return max boost before strategy dispatch
        // (handled in recency_boost guard)
    }

    #[test]
    fn test_recency_strategy_linear_recent() {
        let config = ScoringConfig {
            recency_strategy: "linear".to_string(),
            ..Default::default()
        };
        let boost_new = recency_boost(&days_ago(1), &config);
        let boost_old = recency_boost(&days_ago(900), &config);
        // Very recent → close to max; 900 days is under the penalty threshold (1825)
        // but boost should be near zero (900/1825 ≈ 0.49 through, so ~50% of max)
        assert!(boost_new > 0.0);
        assert!(boost_new > boost_old);
    }

    #[test]
    fn test_recency_strategy_linear_at_threshold() {
        let config = ScoringConfig {
            recency_strategy: "linear".to_string(),
            recency_penalty_after_days: 365,
            ..Default::default()
        };
        // At the threshold, linear boost should be ~0
        let boost = recency_boost("2025-04-15", &config); // ~365 days ago from 2026-04-15
        assert!(boost.abs() < 0.1);
    }

    #[test]
    fn test_recency_strategy_step() {
        let config = ScoringConfig {
            recency_strategy: "step".to_string(),
            recency_half_life_days: 180,
            ..Default::default()
        };
        let boost_new = recency_boost(&days_ago(30), &config);
        let boost_mid = recency_boost(&days_ago(300), &config);
        assert_eq!(boost_new, config.recency_boost_max);
        assert_eq!(boost_mid, 0.0); // Between half_life and penalty threshold
    }

    #[test]
    fn test_recency_strategy_custom_empty_curve() {
        let config = ScoringConfig {
            recency_strategy: "custom".to_string(),
            recency_curve: vec![],
            ..Default::default()
        };
        assert_eq!(recency_boost(&days_ago(30), &config), 0.0);
    }

    #[test]
    fn test_recency_strategy_custom_interpolation() {
        let config = ScoringConfig {
            recency_strategy: "custom".to_string(),
            recency_curve: vec![[0.0, 1.0], [365.0, 0.5], [730.0, 0.0]],
            ..Default::default()
        };
        // At day 0: boost = 1.0 (clamped to first point)
        let b0 = recency_custom(0.0, &config);
        assert!((b0 - 1.0).abs() < 0.001);

        // At day 182.5: midpoint between [0,1.0] and [365,0.5] → ~0.75
        let b_mid = recency_custom(182.5, &config);
        assert!((b_mid - 0.75).abs() < 0.01);

        // At day 365: exactly 0.5
        let b365 = recency_custom(365.0, &config);
        assert!((b365 - 0.5).abs() < 0.001);

        // Beyond last point: clamped to 0.0
        let b_beyond = recency_custom(1000.0, &config);
        assert!((b_beyond - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_recency_strategy_unknown_falls_back_to_exponential() {
        let exponential = ScoringConfig {
            recency_strategy: "exponential".to_string(),
            ..Default::default()
        };
        let unknown = ScoringConfig {
            recency_strategy: "foobar".to_string(),
            ..Default::default()
        };
        let date = days_ago(60);
        assert_eq!(
            recency_boost(&date, &exponential),
            recency_boost(&date, &unknown)
        );
    }

    #[test]
    fn test_config_validate_unknown_strategy() {
        let config = ScoringConfig {
            recency_strategy: "invalid".to_string(),
            ..Default::default()
        };
        let warnings = config.validate();
        assert!(warnings.iter().any(|w| w.field == "recency_strategy"));
    }

    #[test]
    fn test_config_validate_custom_empty_curve() {
        let config = ScoringConfig {
            recency_strategy: "custom".to_string(),
            recency_curve: vec![],
            ..Default::default()
        };
        let warnings = config.validate();
        assert!(warnings.iter().any(|w| w.field == "recency_curve"));
    }

    #[test]
    fn test_config_validate_unsorted_curve() {
        let config = ScoringConfig {
            recency_strategy: "custom".to_string(),
            recency_curve: vec![[365.0, 0.5], [0.0, 1.0]], // reversed
            ..Default::default()
        };
        let warnings = config.validate();
        assert!(warnings.iter().any(|w| w.field == "recency_curve"));
    }

    #[test]
    fn test_language_config_default() {
        let config = ScoringConfig::default();
        assert_eq!(config.language, "en");
        assert!(config.custom_stop_words.is_empty());
    }

    #[test]
    fn test_score_results_respects_language() {
        // Use German language — "und" should be filtered as a stop word
        let config = ScoringConfig {
            language: "de".to_string(),
            ..Default::default()
        };
        // "und" is a German stop word; "drupal" is not
        // Score results with a query containing a German stop word
        let mut results = vec![SearchResult {
            url: "https://example.com".to_string(),
            title: "Drupal Guide".to_string(),
            excerpt: "Alles über Drupal".to_string(),
            date: days_ago(30),
            score: 1.0,
            content_type: String::new(),
            site_name: String::new(),
            extra: serde_json::Map::new(),
        }];
        // Should not panic; query with only German stop words → no terms → title boost is 0
        score_results(&mut results, "und der die", &config);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_custom_stop_words() {
        let config = ScoringConfig {
            custom_stop_words: vec!["drupal".to_string()],
            ..Default::default()
        };
        let mut results = vec![SearchResult {
            url: "https://example.com".to_string(),
            title: "Drupal Guide".to_string(),
            excerpt: "All about Drupal CMS".to_string(),
            date: days_ago(30),
            score: 1.0,
            content_type: String::new(),
            site_name: String::new(),
            extra: serde_json::Map::new(),
        }];
        // "drupal" is now a custom stop word; "guide" remains
        score_results(&mut results, "drupal guide", &config);
        assert_eq!(results.len(), 1);
    }
}
