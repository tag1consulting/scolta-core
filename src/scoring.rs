//! Search result scoring and ranking algorithms.
//!
//! Provides the canonical Scolta ranking: recency decay, title/content match
//! boosting, priority page boosting, composite scoring, and result merging
//! with deduplication. All math lives here so that every language adapter
//! (PHP, Python, JS, Go) produces identical rankings.
//!
//! # Scoring formula
//!
//! ```text
//! final_score = (base_score × source_weight) + title_boost + content_boost + recency_boost + priority_boost
//! ```
//!
//! `base_score` is the upstream search engine score (e.g., from Pagefind).
//! `source_weight` dampens results from secondary sources (e.g., expanded terms
//! in SAYT). `priority_boost` is added when the result URL matches a configured
//! priority page and the query contains that page's keywords.

use crate::common;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A priority page entry that receives a score boost when query keywords match.
///
/// Priority pages surface specific results for branded or high-value queries
/// (e.g., `/team/` for queries containing "team" or "leadership").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityPage {
    /// URL path to match against result URLs (e.g., `"/team/"`).
    pub url_pattern: String,
    /// Query keywords that trigger the boost. Case-insensitive substring match.
    /// Multi-word keywords (e.g., `"why tag1"`) match as phrases.
    pub keywords: Vec<String>,
    /// Score boost to apply when a query keyword matches.
    pub boost: f64,
    /// Optional replacement excerpt shown instead of the Pagefind-generated one.
    #[serde(default)]
    pub custom_excerpt: Option<String>,
    /// Optional identifier for client-side use.
    #[serde(default)]
    pub page_id: Option<String>,
}

/// Configuration for search result scoring.
///
/// All fields have sensible defaults (see [`Default`] impl). Callers can
/// override any subset; unspecified fields keep their defaults.
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
/// | `phrase_adjacent_multiplier` | 1.0–10.0 | 2.5 |
/// | `phrase_near_multiplier` | 1.0–5.0 | 1.5 |
/// | `phrase_near_window` | 1–50 | 5 |
/// | `phrase_window` | 1–200 | 15 |
/// | `excerpt_length` | 50–2000 | 300 |
/// | `results_per_page` | 1–100 | 10 |
/// | `max_pagefind_results` | 1–500 | 50 |
/// | `language` | ISO 639-1 code | "en" |
/// | `custom_stop_words` | list of lowercase tokens | [] |
/// | `priority_pages` | list of PriorityPage | [] |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    pub recency_boost_max: f64,
    pub recency_half_life_days: u32,
    pub recency_penalty_after_days: u32,
    pub recency_max_penalty: f64,
    pub recency_strategy: String,
    pub recency_curve: Vec<[f64; 2]>,
    pub title_match_boost: f64,
    pub title_all_terms_multiplier: f64,
    pub content_match_boost: f64,
    pub content_all_terms_multiplier: f64,
    /// Multiplier applied to `content_boost` when all query terms appear
    /// adjacent to each other (span ≤ terms−1 word positions apart).
    pub phrase_adjacent_multiplier: f64,
    /// Multiplier applied to `content_boost` when all query terms appear
    /// within `phrase_near_window` word positions of each other.
    pub phrase_near_multiplier: f64,
    /// Maximum word-position span for the "near phrase" bonus.
    pub phrase_near_window: u32,
    /// Maximum word-position span for a modest phrase bonus (larger than
    /// near; no boost applied beyond this distance).
    pub phrase_window: u32,
    pub excerpt_length: u32,
    pub results_per_page: u32,
    pub max_pagefind_results: u32,
    pub language: String,
    pub custom_stop_words: Vec<String>,
    /// Priority pages receive a score boost when query keywords match.
    /// Default: empty (no priority pages).
    #[serde(default)]
    pub priority_pages: Vec<PriorityPage>,
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
            phrase_adjacent_multiplier: 2.5,
            phrase_near_multiplier: 1.5,
            phrase_near_window: 5,
            phrase_window: 15,
            excerpt_length: 300,
            results_per_page: 10,
            max_pagefind_results: 50,
            language: "en".to_string(),
            custom_stop_words: Vec::new(),
            priority_pages: Vec::new(),
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
    /// outside reasonable ranges. The config is still usable with warnings.
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

    /// Clamp out-of-range values to their documented boundaries and return a
    /// warning for each clamped field.
    ///
    /// Called by [`crate::config::from_json_validated`] after parsing, so
    /// misconfigured sites (e.g. `recency_boost_max: 100.0`) get a corrected
    /// config and a logged warning rather than silently broken scoring.
    ///
    /// Fields that cannot be meaningfully clamped to a range — string enums
    /// (`recency_strategy`) and structural checks (`recency_curve` sort order)
    /// — are left unchanged; their warnings come from `validate()` instead.
    pub fn clamp_and_validate(&mut self) -> Vec<ConfigWarning> {
        let mut warnings = Vec::new();

        if self.recency_boost_max < 0.0 || self.recency_boost_max > 2.0 {
            let clamped = self.recency_boost_max.clamp(0.0, 2.0);
            warnings.push(ConfigWarning {
                field: "recency_boost_max",
                message: format!(
                    "value {} outside range (0.0–2.0), clamped to {clamped}",
                    self.recency_boost_max
                ),
            });
            self.recency_boost_max = clamped;
        }

        if self.recency_half_life_days == 0 || self.recency_half_life_days > 3650 {
            let clamped = self.recency_half_life_days.clamp(1, 3650);
            warnings.push(ConfigWarning {
                field: "recency_half_life_days",
                message: format!(
                    "value {} outside range (1–3650), clamped to {clamped}",
                    self.recency_half_life_days
                ),
            });
            self.recency_half_life_days = clamped;
        }

        if self.recency_max_penalty < 0.0 || self.recency_max_penalty > 1.0 {
            let clamped = self.recency_max_penalty.clamp(0.0, 1.0);
            warnings.push(ConfigWarning {
                field: "recency_max_penalty",
                message: format!(
                    "value {} outside range (0.0–1.0), clamped to {clamped}",
                    self.recency_max_penalty
                ),
            });
            self.recency_max_penalty = clamped;
        }

        if self.results_per_page == 0 || self.results_per_page > 100 {
            let clamped = self.results_per_page.clamp(1, 100);
            warnings.push(ConfigWarning {
                field: "results_per_page",
                message: format!(
                    "value {} outside range (1–100), clamped to {clamped}",
                    self.results_per_page
                ),
            });
            self.results_per_page = clamped;
        }

        if self.max_pagefind_results == 0 || self.max_pagefind_results > 500 {
            let clamped = self.max_pagefind_results.clamp(1, 500);
            warnings.push(ConfigWarning {
                field: "max_pagefind_results",
                message: format!(
                    "value {} outside range (1–500), clamped to {clamped}",
                    self.max_pagefind_results
                ),
            });
            self.max_pagefind_results = clamped;
        }

        // Append non-clampable warnings from validate().
        warnings.extend(self.validate());
        warnings
    }
}

/// A single search result with score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub excerpt: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub site_name: String,
    /// Per-result weight applied to the base score before adding boosts.
    /// Used to dampen results from secondary sources (e.g., expanded SAYT
    /// terms). Default: 1.0 (no dampening). Callers that don't supply this
    /// field get the same scoring as before.
    #[serde(default)]
    pub source_weight: Option<f64>,
    /// Word-position array from Pagefind for all query-matched terms in this
    /// document. Populated by the JS layer from `result.data().locations`.
    /// When present, enables phrase-proximity scoring. Absent for results
    /// that pre-date this field or come from non-Pagefind sources.
    #[serde(default)]
    pub locations: Option<Vec<u32>>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// One input set for `merge_results` — a slice of results and the weight to
/// apply to all their scores before merging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSet {
    pub results: Vec<SearchResult>,
    pub weight: f64,
}

/// Options for `merge_results`.
#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    pub sets: Vec<MergeSet>,
    /// Field to deduplicate by: `"url"`, `"title"`, or `None` (no dedup).
    pub deduplicate_by: Option<String>,
    /// Case-sensitive deduplication for `"title"`. Default: false.
    pub case_sensitive: bool,
    /// URLs to exclude from the merged output.
    pub exclude_urls: Vec<String>,
    /// Normalize URLs for comparison (strip protocol, trailing slash, lowercase domain).
    pub normalize_urls: bool,
}

/// Return the priority pages whose keywords match the given query.
///
/// Performs case-insensitive substring matching. Multi-word keywords must
/// appear as a contiguous substring in the (lowercased) query.
pub fn match_priority_pages<'a>(query: &str, pages: &'a [PriorityPage]) -> Vec<&'a PriorityPage> {
    let query_lower = query.to_lowercase();
    pages
        .iter()
        .filter(|pp| {
            pp.keywords
                .iter()
                .any(|kw| query_lower.contains(&kw.to_lowercase()))
        })
        .collect()
}

pub fn recency_boost(date: &str, config: &ScoringConfig) -> f64 {
    let days_since = match days_since_date(date) {
        Some(d) => d,
        None => return 0.0,
    };

    if days_since < 0 {
        return config.recency_boost_max;
    }

    let days_old = days_since as f64;

    match config.recency_strategy.as_str() {
        "linear" => recency_linear(days_old, config),
        "step" => recency_step(days_old, config),
        "none" => 0.0,
        "custom" => recency_custom(days_old, config),
        _ => recency_exponential(days_old, config),
    }
}

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

fn recency_linear(days_old: f64, config: &ScoringConfig) -> f64 {
    let penalty_threshold = config.recency_penalty_after_days as f64;

    if days_old < penalty_threshold {
        let fraction = 1.0 - (days_old / penalty_threshold);
        (config.recency_boost_max * fraction).max(0.0)
    } else {
        recency_old_penalty(days_old, penalty_threshold, config)
    }
}

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

pub fn recency_custom(days_old: f64, config: &ScoringConfig) -> f64 {
    let curve = &config.recency_curve;

    if curve.is_empty() {
        return 0.0;
    }

    if days_old <= curve[0][0] {
        return curve[0][1];
    }
    if days_old >= curve[curve.len() - 1][0] {
        return curve[curve.len() - 1][1];
    }

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

fn recency_old_penalty(days_old: f64, penalty_threshold: f64, config: &ScoringConfig) -> f64 {
    let years_over = (days_old - penalty_threshold) / 365.0;
    let penalty = (years_over * 0.05).min(config.recency_max_penalty);
    -penalty
}

pub fn title_match_score(query: &str, title: &str, config: &ScoringConfig) -> f64 {
    let terms = common::extract_terms(query, &config.language);
    title_match_score_with_terms(&terms, title, config)
}

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
    if matching_count == terms.len() && terms.len() > 1 {
        boost *= config.title_all_terms_multiplier;
    }

    boost * (matching_count as f64 / terms.len() as f64)
}

pub fn content_match_score(query: &str, content: &str, config: &ScoringConfig) -> f64 {
    let terms = common::extract_terms(query, &config.language);
    content_match_score_with_terms(&terms, content, config)
}

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
    if matching_count == terms.len() && terms.len() > 1 {
        boost = config.content_all_terms_multiplier;
    }

    boost * (matching_count as f64 / terms.len() as f64)
}

/// Compute a phrase-proximity multiplier from Pagefind word positions.
///
/// Returns a multiplier applied to `content_boost` when all query terms land
/// within a small position window, rewarding results where the terms appear
/// together rather than scattered across the document.
///
/// | Span | Multiplier |
/// |------|------------|
/// | ≤ `terms.len() − 1` (adjacent) | `config.phrase_adjacent_multiplier` |
/// | ≤ `config.phrase_near_window` | `config.phrase_near_multiplier` |
/// | anything wider | `1.0` (no bonus) |
///
/// `locations` is the flat array of word-position integers from Pagefind's
/// `result.data().locations` — all matched-term positions combined. Sorting
/// and a sliding window of size `n` find the tightest cluster of `n` hits.
fn phrase_proximity_multiplier(terms: &[String], locations: &[u32], config: &ScoringConfig) -> f64 {
    let n = terms.len();
    if n < 2 || locations.len() < n {
        return 1.0;
    }
    let mut sorted = locations.to_vec();
    sorted.sort_unstable();
    // Sliding window of size n → find minimum span (max − min of window).
    let min_span = sorted
        .windows(n)
        .map(|w| w[n - 1] - w[0])
        .min()
        .unwrap_or(u32::MAX);
    if min_span < n as u32 {
        config.phrase_adjacent_multiplier
    } else if min_span <= config.phrase_near_window {
        config.phrase_near_multiplier
    } else {
        1.0
    }
}

/// Calculate composite score using a [`QueryInfo`] and a pre-computed priority boost.
///
/// Extends [`score_result_with_terms`] with phrase-proximity scoring: when
/// `query_info.is_phrase` is true and the result carries Pagefind `locations`,
/// the content boost is multiplied by the phrase-proximity factor.
///
/// Formula:
/// ```text
/// final = (base × source_weight) + title_boost + (content_boost × phrase_mult) + recency + priority
/// ```
pub fn score_result_with_query_info(
    result: &SearchResult,
    query_info: &common::QueryInfo,
    config: &ScoringConfig,
    priority_boost: f64,
) -> f64 {
    let base_score = if result.score > 0.0 {
        result.score
    } else {
        1.0
    };
    let source_weight = result.source_weight.unwrap_or(1.0);
    let title_boost = title_match_score_with_terms(&query_info.terms, &result.title, config);
    let content_boost = content_match_score_with_terms(&query_info.terms, &result.excerpt, config);
    let recency = recency_boost(&result.date, config);
    let phrase_mult = if query_info.is_phrase {
        result
            .locations
            .as_deref()
            .map(|locs| phrase_proximity_multiplier(&query_info.terms, locs, config))
            .unwrap_or(1.0)
    } else {
        1.0
    };
    (base_score * source_weight)
        + title_boost
        + (content_boost * phrase_mult)
        + recency
        + priority_boost
}

/// Calculate composite score using pre-extracted terms and a pre-computed priority boost.
///
/// Formula: `(base_score × source_weight) + title_boost + content_boost + recency_boost + priority_boost`
pub fn score_result_with_terms(
    result: &SearchResult,
    terms: &[String],
    config: &ScoringConfig,
    priority_boost: f64,
) -> f64 {
    let base_score = if result.score > 0.0 {
        result.score
    } else {
        1.0
    };
    let source_weight = result.source_weight.unwrap_or(1.0);
    let title_boost = title_match_score_with_terms(terms, &result.title, config);
    let content_boost = content_match_score_with_terms(terms, &result.excerpt, config);
    let recency = recency_boost(&result.date, config);

    (base_score * source_weight) + title_boost + content_boost + recency + priority_boost
}

/// Calculate composite score for a single result.
pub fn score_result(result: &SearchResult, query: &str, config: &ScoringConfig) -> f64 {
    let terms = if config.custom_stop_words.is_empty() {
        common::extract_terms(query, &config.language)
    } else {
        common::extract_terms_with_custom(query, &config.language, &config.custom_stop_words)
    };
    let query_lower = query.to_lowercase();
    let priority_boost: f64 = config
        .priority_pages
        .iter()
        .filter(|pp| {
            pp.keywords
                .iter()
                .any(|kw| query_lower.contains(&kw.to_lowercase()))
        })
        .filter(|pp| result.url.contains(&pp.url_pattern))
        .map(|pp| pp.boost)
        .sum();
    score_result_with_terms(result, &terms, config, priority_boost)
}

/// Score all results and sort by relevance (highest first).
///
/// Pre-computes matched priority pages for the query once, then applies
/// per-result priority boosts and optional custom excerpt overrides.
/// Phrase-proximity scoring is applied when the query contains two or more
/// terms and each result carries Pagefind `locations` data.
pub fn score_results(results: &mut [SearchResult], query: &str, config: &ScoringConfig) {
    let query_info = if config.custom_stop_words.is_empty() {
        common::extract_query(query, &config.language)
    } else {
        common::extract_query_with_custom(query, &config.language, &config.custom_stop_words)
    };

    let query_lower = query.to_lowercase();
    let matched_priority_pages: Vec<&PriorityPage> = config
        .priority_pages
        .iter()
        .filter(|pp| {
            pp.keywords
                .iter()
                .any(|kw| query_lower.contains(&kw.to_lowercase()))
        })
        .collect();

    for result in results.iter_mut() {
        let priority_boost: f64 = matched_priority_pages
            .iter()
            .filter(|pp| result.url.contains(&pp.url_pattern))
            .map(|pp| pp.boost)
            .sum();

        // Apply custom excerpt from the first matching priority page that has one.
        if !matched_priority_pages.is_empty() {
            for pp in &matched_priority_pages {
                if result.url.contains(&pp.url_pattern) {
                    if let Some(custom) = &pp.custom_excerpt {
                        result.excerpt = custom.clone();
                        break;
                    }
                }
            }
        }

        result.score = score_result_with_query_info(result, &query_info, config, priority_boost);
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Merge N result sets with per-set weights, optional deduplication, and URL filtering.
///
/// Algorithm:
/// 1. Apply each set's weight to all result scores.
/// 2. Combine all results and sort by score descending.
/// 3. If `deduplicate_by` is set, keep only the first occurrence (highest-scored)
///    for each distinct key.
/// 4. Drop results whose URLs appear in `exclude_urls`.
pub fn merge_results(options: MergeOptions) -> Vec<SearchResult> {
    // Step 1: Apply weights and flatten into a single vec.
    let mut all: Vec<SearchResult> = options
        .sets
        .into_iter()
        .flat_map(|set| {
            let w = set.weight;
            set.results.into_iter().map(move |mut r| {
                r.score *= w;
                r
            })
        })
        .collect();

    // Step 2: Sort by score descending.
    all.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Step 3: Deduplicate (keep first = highest-scored occurrence).
    if let Some(ref field) = options.deduplicate_by {
        let mut seen: HashSet<String> = HashSet::new();
        all.retain(|r| {
            let key = match field.as_str() {
                "title" => {
                    if options.case_sensitive {
                        r.title.clone()
                    } else {
                        r.title.to_lowercase()
                    }
                }
                _ => normalize_url_key(&r.url, options.normalize_urls),
            };
            seen.insert(key)
        });
    }

    // Step 4: Drop excluded URLs.
    if !options.exclude_urls.is_empty() {
        let excluded: HashSet<String> = options
            .exclude_urls
            .iter()
            .map(|u| normalize_url_key(u, options.normalize_urls))
            .collect();
        all.retain(|r| !excluded.contains(&normalize_url_key(&r.url, options.normalize_urls)));
    }

    all
}

/// Normalize a URL for comparison.
///
/// When `normalize` is true: strips trailing slash, lowercases the domain,
/// and treats http and https as equivalent.
fn normalize_url_key(url: &str, normalize: bool) -> String {
    if !normalize {
        return url.to_string();
    }
    let s = url.trim_end_matches('/');
    let without_proto = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    // Lowercase domain only (path is case-sensitive on most servers).
    match without_proto.split_once('/') {
        Some((domain, path)) => format!("{}/{}", domain.to_lowercase(), path),
        None => without_proto.to_lowercase(),
    }
}

// ---------------------------------------------------------------------------
// Date utilities
// ---------------------------------------------------------------------------

fn parse_date(date_str: &str) -> Option<(i32, i32, i32)> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }

    let year: i32 = parts[0].parse().ok()?;
    let month: i32 = parts[1].parse().ok()?;
    let day: i32 = parts[2].parse().ok()?;

    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    Some((year, month, day))
}

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

fn days_since_date(date_str: &str) -> Option<i32> {
    let (year, month, day) = parse_date(date_str)?;
    let (ref_y, ref_m, ref_d) = today();

    Some(date_to_days(ref_y, ref_m, ref_d) - date_to_days(year, month, day))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn make_result(url: &str, title: &str, score: f64) -> SearchResult {
        SearchResult {
            url: url.to_string(),
            title: title.to_string(),
            excerpt: "content".to_string(),
            date: days_ago(30),
            score,
            content_type: String::new(),
            site_name: String::new(),
            source_weight: None,
            locations: None,
            extra: serde_json::Map::new(),
        }
    }

    fn make_result_with_excerpt_and_locations(
        url: &str,
        title: &str,
        excerpt: &str,
        score: f64,
        locations: Option<Vec<u32>>,
    ) -> SearchResult {
        SearchResult {
            url: url.to_string(),
            title: title.to_string(),
            excerpt: excerpt.to_string(),
            date: days_ago(30),
            score,
            content_type: String::new(),
            site_name: String::new(),
            source_weight: None,
            locations,
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn test_default_config() {
        let config = ScoringConfig::default();
        assert_eq!(config.recency_boost_max, 0.5);
        assert_eq!(config.recency_half_life_days, 365);
        assert_eq!(config.content_all_terms_multiplier, 0.48);
        assert!(config.priority_pages.is_empty());
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
        assert!(boost > 0.0);
        assert!(boost <= config.recency_boost_max);
    }

    #[test]
    fn test_recency_boost_old() {
        let config = ScoringConfig::default();
        assert!(recency_boost("2000-01-01", &config) < 0.0);
    }

    #[test]
    fn test_recency_boost_unparseable_date() {
        let config = ScoringConfig::default();
        assert_eq!(recency_boost("not-a-date", &config), 0.0);
        assert_eq!(recency_boost("", &config), 0.0);
    }

    #[test]
    fn test_title_match_score_all_terms() {
        let config = ScoringConfig::default();
        let score = title_match_score("hello world", "Hello World Page", &config);
        let expected = config.title_match_boost * config.title_all_terms_multiplier;
        assert!((score - expected).abs() < 0.001);
    }

    #[test]
    fn test_title_match_score_partial() {
        let config = ScoringConfig::default();
        let score = title_match_score("hello world", "Hello there", &config);
        let expected = config.title_match_boost * 0.5;
        assert!((score - expected).abs() < 0.001);
    }

    #[test]
    fn test_source_weight_applied() {
        let config = ScoringConfig::default();
        let mut r_full = make_result("https://a.com", "Test", 1.0);
        let mut r_damped = make_result("https://a.com", "Test", 1.0);
        r_damped.source_weight = Some(0.3);

        r_full.score = score_result_with_terms(&r_full, &[], &config, 0.0);
        r_damped.score = score_result_with_terms(&r_damped, &[], &config, 0.0);
        assert!(r_full.score > r_damped.score);
    }

    #[test]
    fn test_priority_page_boost() {
        let config = ScoringConfig {
            priority_pages: vec![PriorityPage {
                url_pattern: "/team/".to_string(),
                keywords: vec!["team".to_string(), "leadership".to_string()],
                boost: 100.0,
                custom_excerpt: None,
                page_id: None,
            }],
            ..Default::default()
        };

        let mut results = vec![
            make_result("https://example.com/team/", "Team Page", 1.0),
            make_result("https://example.com/blog/", "Blog Post", 1.0),
        ];

        score_results(&mut results, "team members", &config);

        // Team page should rank first due to priority boost
        assert_eq!(results[0].url, "https://example.com/team/");
        assert!(results[0].score > results[1].score + 90.0);
    }

    #[test]
    fn test_priority_page_no_boost_when_query_no_keyword() {
        let config = ScoringConfig {
            priority_pages: vec![PriorityPage {
                url_pattern: "/team/".to_string(),
                keywords: vec!["team".to_string()],
                boost: 100.0,
                custom_excerpt: None,
                page_id: None,
            }],
            ..Default::default()
        };

        let mut results = vec![make_result("https://example.com/team/", "Team", 1.0)];
        score_results(&mut results, "drupal migration", &config);
        // No keyword match → no boost → normal score
        assert!(results[0].score < 10.0);
    }

    #[test]
    fn test_priority_page_custom_excerpt() {
        let config = ScoringConfig {
            priority_pages: vec![PriorityPage {
                url_pattern: "/team/".to_string(),
                keywords: vec!["team".to_string()],
                boost: 100.0,
                custom_excerpt: Some("Meet our expert team.".to_string()),
                page_id: None,
            }],
            ..Default::default()
        };

        let mut results = vec![make_result("https://example.com/team/", "Team", 1.0)];
        score_results(&mut results, "team leadership", &config);
        assert_eq!(results[0].excerpt, "Meet our expert team.");
    }

    #[test]
    fn test_match_priority_pages() {
        let pages = vec![
            PriorityPage {
                url_pattern: "/team/".to_string(),
                keywords: vec!["team".to_string(), "leadership".to_string()],
                boost: 100.0,
                custom_excerpt: None,
                page_id: None,
            },
            PriorityPage {
                url_pattern: "/contact/".to_string(),
                keywords: vec!["contact".to_string(), "reach out".to_string()],
                boost: 50.0,
                custom_excerpt: None,
                page_id: None,
            },
        ];

        let matched = match_priority_pages("who is on the team", &pages);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].url_pattern, "/team/");

        let matched_phrase = match_priority_pages("how to reach out to us", &pages);
        assert_eq!(matched_phrase.len(), 1);
        assert_eq!(matched_phrase[0].url_pattern, "/contact/");

        let matched_none = match_priority_pages("drupal migration guide", &pages);
        assert!(matched_none.is_empty());
    }

    #[test]
    fn test_merge_results_two_sets_url_dedup() {
        let options = MergeOptions {
            sets: vec![
                MergeSet {
                    results: vec![make_result("https://a.com", "A", 10.0)],
                    weight: 0.7,
                },
                MergeSet {
                    results: vec![
                        make_result("https://a.com", "A", 5.0),
                        make_result("https://b.com", "B", 3.0),
                    ],
                    weight: 0.3,
                },
            ],
            deduplicate_by: Some("url".to_string()),
            case_sensitive: false,
            exclude_urls: vec![],
            normalize_urls: false,
        };

        let merged = merge_results(options);
        assert_eq!(merged.len(), 2);
        // Highest-scored https://a.com (from set1 at 0.7) survives
        assert_eq!(merged[0].url, "https://a.com");
        assert!((merged[0].score - 7.0).abs() < 0.001);
    }

    #[test]
    fn test_merge_results_title_dedup() {
        let options = MergeOptions {
            sets: vec![MergeSet {
                results: vec![
                    make_result("https://a.com", "Drupal Guide", 10.0),
                    make_result("https://b.com", "drupal guide", 5.0), // same title, lowercase
                ],
                weight: 1.0,
            }],
            deduplicate_by: Some("title".to_string()),
            case_sensitive: false,
            exclude_urls: vec![],
            normalize_urls: false,
        };

        let merged = merge_results(options);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].url, "https://a.com"); // higher score wins
    }

    #[test]
    fn test_merge_results_exclude_urls() {
        let options = MergeOptions {
            sets: vec![MergeSet {
                results: vec![
                    make_result("https://a.com/", "A", 10.0),
                    make_result("https://b.com/page", "B", 5.0),
                ],
                weight: 1.0,
            }],
            deduplicate_by: None,
            case_sensitive: false,
            exclude_urls: vec!["https://a.com".to_string()], // strip trailing slash
            normalize_urls: true,
        };

        let merged = merge_results(options);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].url, "https://b.com/page");
    }

    #[test]
    fn test_merge_results_normalize_urls() {
        let options = MergeOptions {
            sets: vec![MergeSet {
                results: vec![make_result("http://Example.com/page/", "A", 10.0)],
                weight: 1.0,
            }],
            deduplicate_by: Some("url".to_string()),
            case_sensitive: false,
            exclude_urls: vec!["https://example.com/page".to_string()],
            normalize_urls: true,
        };

        let merged = merge_results(options);
        assert!(merged.is_empty()); // normalized URL matches exclude list
    }

    #[test]
    fn test_merge_results_no_dedup() {
        let options = MergeOptions {
            sets: vec![MergeSet {
                results: vec![
                    make_result("https://a.com", "A", 10.0),
                    make_result("https://a.com", "A", 5.0),
                ],
                weight: 1.0,
            }],
            deduplicate_by: None,
            ..Default::default()
        };

        let merged = merge_results(options);
        assert_eq!(merged.len(), 2); // duplicates kept when no dedup
    }

    #[test]
    fn test_score_results_uses_existing_score() {
        let config = ScoringConfig::default();
        let result_with_score = SearchResult {
            url: "https://example.com".to_string(),
            title: "Test".to_string(),
            excerpt: "Test content".to_string(),
            date: days_ago(60),
            score: 5.0,
            content_type: String::new(),
            site_name: String::new(),
            source_weight: None,
            locations: None,
            extra: serde_json::Map::new(),
        };

        let result_without_score = SearchResult {
            score: 0.0,
            ..result_with_score.clone()
        };

        let score_with = score_result(&result_with_score, "test", &config);
        let score_without = score_result(&result_without_score, "test", &config);
        assert!(score_with > score_without);
    }

    #[test]
    fn test_civil_from_epoch_secs() {
        let (y, m, d) = civil_from_epoch_secs(1_767_225_600);
        assert_eq!((y, m, d), (2026, 1, 1));
    }

    #[test]
    fn test_recency_strategy_none() {
        let config = ScoringConfig {
            recency_strategy: "none".to_string(),
            ..Default::default()
        };
        assert_eq!(recency_boost(&days_ago(1), &config), 0.0);
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
        assert_eq!(boost_mid, 0.0);
    }

    #[test]
    fn test_recency_strategy_custom_interpolation() {
        let config = ScoringConfig {
            recency_strategy: "custom".to_string(),
            recency_curve: vec![[0.0, 1.0], [365.0, 0.5], [730.0, 0.0]],
            ..Default::default()
        };
        let b0 = recency_custom(0.0, &config);
        assert!((b0 - 1.0).abs() < 0.001);
        let b365 = recency_custom(365.0, &config);
        assert!((b365 - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_recency_strategy_unknown_falls_back_to_exponential() {
        let exp = ScoringConfig {
            recency_strategy: "exponential".to_string(),
            ..Default::default()
        };
        let unk = ScoringConfig {
            recency_strategy: "foobar".to_string(),
            ..Default::default()
        };
        let date = days_ago(60);
        assert_eq!(recency_boost(&date, &exp), recency_boost(&date, &unk));
    }

    #[test]
    fn test_config_validate_unknown_strategy() {
        let config = ScoringConfig {
            recency_strategy: "invalid".to_string(),
            ..Default::default()
        };
        assert!(config
            .validate()
            .iter()
            .any(|w| w.field == "recency_strategy"));
    }

    #[test]
    fn test_normalize_url_key() {
        assert_eq!(
            normalize_url_key("https://Example.com/Page/", true),
            "example.com/Page"
        );
        assert_eq!(
            normalize_url_key("http://example.com/page", true),
            "example.com/page"
        );
        assert_eq!(
            normalize_url_key("https://example.com/page", false),
            "https://example.com/page"
        );
    }

    // --- Phrase-proximity regression tests ---

    // Test 1: Adjacent phrase in body must outrank a single-term title hit.
    //
    // r1 has "hello" in the title (one of two terms) → title boost only.
    // r2 has "hello world" adjacent in the excerpt (positions 0, 1) → phrase
    // multiplier fires and content boost beats title dominance.
    #[test]
    fn test_phrase_adjacent_ranks_above_single_term_title() {
        let config = ScoringConfig::default();
        let r1 = make_result_with_excerpt_and_locations(
            "https://example.com/1",
            "Hello Integrations",
            "Some content about modules",
            1.0,
            Some(vec![0]), // only "hello" matched, at position 0
        );
        let r2 = make_result_with_excerpt_and_locations(
            "https://example.com/2",
            "Module Integration Guide",
            "hello world module documentation",
            1.0,
            Some(vec![0, 1]), // "hello" at 0, "world" at 1 — adjacent phrase
        );
        let mut results = vec![r1, r2];
        score_results(&mut results, "hello world", &config);
        assert_eq!(
            results[0].url, "https://example.com/2",
            "Adjacent phrase in body must rank first; got {}",
            results[0].url
        );
    }

    // Test 2: Near phrase (within window) must outrank scattered terms.
    //
    // r1 has "hello" and "world" scattered 50 positions apart (no phrase bonus).
    // r2 has them within the near window (positions 0, 4) → near multiplier.
    #[test]
    fn test_phrase_near_ranks_above_scattered() {
        let config = ScoringConfig::default();
        let r1 = make_result_with_excerpt_and_locations(
            "https://example.com/scattered",
            "Title",
            "hello many words world",
            1.0,
            Some(vec![0, 50]), // scattered — span 50 > phrase_window (15)
        );
        let r2 = make_result_with_excerpt_and_locations(
            "https://example.com/near",
            "Title",
            "hello quick world",
            1.0,
            Some(vec![0, 4]), // span 4 ≤ phrase_near_window (5)
        );
        let mut results = vec![r1, r2];
        score_results(&mut results, "hello world", &config);
        assert_eq!(
            results[0].url, "https://example.com/near",
            "Near phrase must rank above scattered; got {}",
            results[0].url
        );
    }

    // Test 3: Single-term queries are unaffected by phrase scoring.
    #[test]
    fn test_single_term_query_unchanged() {
        let config = ScoringConfig::default();
        // r1 has a title match (higher boost), r2 only content match.
        // With no phrase scoring, title wins — same as pre-phrase behavior.
        let r1 = make_result_with_excerpt_and_locations(
            "https://example.com/title",
            "Hello Page",
            "some body text",
            1.0,
            Some(vec![0]),
        );
        let r2 = make_result_with_excerpt_and_locations(
            "https://example.com/body",
            "Other Page",
            "hello in content",
            1.0,
            Some(vec![0]),
        );
        let mut results = vec![r1.clone(), r2];
        score_results(&mut results, "hello", &config);
        // Title match (r1) must still win for a single-term query.
        assert_eq!(
            results[0].url, "https://example.com/title",
            "Single-term query: title match must still rank first"
        );
    }

    // Test 4: No locations data → falls back to existing term scoring (no crash).
    #[test]
    fn test_phrase_scoring_without_locations_no_crash() {
        let config = ScoringConfig::default();
        let r1 = make_result_with_excerpt_and_locations(
            "https://example.com/1",
            "Hello World Page",
            "hello world content",
            1.0,
            None, // no locations — phrase multiplier must not fire
        );
        let r2 = make_result_with_excerpt_and_locations(
            "https://example.com/2",
            "Other Page",
            "other content",
            0.5,
            None,
        );
        let mut results = vec![r1, r2];
        score_results(&mut results, "hello world", &config);
        // r1 must still rank first via title + content term matching.
        assert_eq!(results[0].url, "https://example.com/1");
    }

    // Test 5: Forced-phrase (quoted) query must detect forced_phrase = true,
    // strip quotes for term extraction, and apply phrase scoring.
    #[test]
    fn test_forced_phrase_quoted_query() {
        let config = ScoringConfig::default();
        // r1: title "Hello Integrations" — single term in title, neither adjacent.
        let r1 = make_result_with_excerpt_and_locations(
            "https://example.com/1",
            "Hello Integrations",
            "Some content about modules",
            1.0,
            Some(vec![0]),
        );
        // r2: "hello world" adjacent in excerpt.
        let r2 = make_result_with_excerpt_and_locations(
            "https://example.com/2",
            "Module Guide",
            "hello world documentation",
            1.0,
            Some(vec![0, 1]),
        );
        let mut results = vec![r1, r2];
        // Quoted query → forced_phrase = true; terms = ["hello", "world"].
        score_results(&mut results, r#""hello world""#, &config);
        assert_eq!(
            results[0].url, "https://example.com/2",
            "Forced-phrase (quoted) query: exact phrase result must rank first; got {}",
            results[0].url
        );
    }

    // -------------------------------------------------------------------
    // Ranking sensitivity — changing a config parameter must flip ranking
    // -------------------------------------------------------------------

    mod ranking_sensitivity {
        use super::*;

        #[test]
        fn title_match_boost_changes_ranking() {
            // A: content match only. B: title match only.
            // Low boost → content (A) wins; high boost → title (B) wins.
            let a = make_result_with_excerpt_and_locations(
                "/a",
                "Other Page",
                "apple content match",
                1.0,
                None,
            );
            let b = make_result_with_excerpt_and_locations(
                "/b",
                "Apple Product",
                "no match here",
                1.0,
                None,
            );

            let mut low = vec![a.clone(), b.clone()];
            let low_cfg = ScoringConfig {
                title_match_boost: 0.1,
                ..Default::default()
            };
            score_results(&mut low, "apple", &low_cfg);
            assert_eq!(
                low[0].url, "/a",
                "low title_match_boost: content result should rank first"
            );

            let mut high = vec![a.clone(), b.clone()];
            let high_cfg = ScoringConfig {
                title_match_boost: 5.0,
                ..Default::default()
            };
            score_results(&mut high, "apple", &high_cfg);
            assert_eq!(
                high[0].url, "/b",
                "high title_match_boost: title result should rank first"
            );
        }

        #[test]
        fn recency_boost_max_changes_ranking() {
            // A: title match, 730 days old. B: content match, 1 day old.
            // Low boost_max → A's relevance wins; high → B's recency wins.
            let mut a = make_result_with_excerpt_and_locations(
                "/a",
                "Apple Product",
                "no content match",
                1.0,
                None,
            );
            a.date = days_ago(730);
            let mut b = make_result_with_excerpt_and_locations(
                "/b",
                "Other Page",
                "apple content here",
                1.0,
                None,
            );
            b.date = days_ago(1);

            let mut low = vec![a.clone(), b.clone()];
            let low_cfg = ScoringConfig {
                recency_boost_max: 0.1,
                ..Default::default()
            };
            score_results(&mut low, "apple", &low_cfg);
            assert_eq!(
                low[0].url, "/a",
                "low recency_boost_max: relevant old result should rank first"
            );

            let mut high = vec![a.clone(), b.clone()];
            let high_cfg = ScoringConfig {
                recency_boost_max: 2.0,
                ..Default::default()
            };
            score_results(&mut high, "apple", &high_cfg);
            assert_eq!(
                high[0].url, "/b",
                "high recency_boost_max: recent result should rank first"
            );
        }

        #[test]
        fn recency_strategy_none_eliminates_recency_effect() {
            // Same relevance (identical content match), different dates.
            // With "exponential": recent (B) wins. With "none": equal scores.
            let mut a =
                make_result_with_excerpt_and_locations("/a", "Other", "apple content", 1.0, None);
            a.date = days_ago(730);
            let mut b =
                make_result_with_excerpt_and_locations("/b", "Other", "apple content", 1.0, None);
            b.date = days_ago(1);

            let exp_cfg = ScoringConfig {
                recency_strategy: "exponential".to_string(),
                ..Default::default()
            };
            let mut exp_results = vec![a.clone(), b.clone()];
            score_results(&mut exp_results, "apple", &exp_cfg);
            assert_eq!(
                exp_results[0].url, "/b",
                "exponential: recent result should rank first"
            );

            let none_cfg = ScoringConfig {
                recency_strategy: "none".to_string(),
                ..Default::default()
            };
            let mut none_results = vec![a.clone(), b.clone()];
            score_results(&mut none_results, "apple", &none_cfg);
            let diff = (none_results[0].score - none_results[1].score).abs();
            assert!(
                diff < 0.001,
                "none strategy: equal-relevance results must have equal scores (diff={diff})"
            );
        }

        #[test]
        fn content_match_boost_changes_ranking() {
            // A: title match only. B: content match only.
            // Low boost → title (A) wins; high boost → content (B) wins.
            let a = make_result_with_excerpt_and_locations(
                "/a",
                "Apple Product",
                "no content match",
                1.0,
                None,
            );
            let b = make_result_with_excerpt_and_locations(
                "/b",
                "Other Page",
                "apple content here",
                1.0,
                None,
            );

            let mut low = vec![a.clone(), b.clone()];
            let low_cfg = ScoringConfig {
                content_match_boost: 0.1,
                ..Default::default()
            };
            score_results(&mut low, "apple", &low_cfg);
            assert_eq!(
                low[0].url, "/a",
                "low content_match_boost: title result should rank first"
            );

            let mut high = vec![a.clone(), b.clone()];
            let high_cfg = ScoringConfig {
                content_match_boost: 3.0,
                ..Default::default()
            };
            score_results(&mut high, "apple", &high_cfg);
            assert_eq!(
                high[0].url, "/b",
                "high content_match_boost: content result should rank first"
            );
        }

        #[test]
        fn content_all_terms_multiplier_changes_ranking() {
            // 2-term query. A: all terms in content (2/2). B: all terms in title (2/2).
            // Low multiplier → B's title-all-terms boost dominates; high → A's content wins.
            let a = make_result_with_excerpt_and_locations(
                "/a",
                "Other",
                "apple orange in content",
                1.0,
                None,
            );
            let b = make_result_with_excerpt_and_locations(
                "/b",
                "Apple Orange Product",
                "no content match",
                1.0,
                None,
            );

            let mut low = vec![a.clone(), b.clone()];
            let low_cfg = ScoringConfig {
                content_all_terms_multiplier: 0.1,
                ..Default::default()
            };
            score_results(&mut low, "apple orange", &low_cfg);
            assert_eq!(
                low[0].url, "/b",
                "low content_all_terms_multiplier: title result should rank first"
            );

            let mut high = vec![a.clone(), b.clone()];
            let high_cfg = ScoringConfig {
                content_all_terms_multiplier: 3.0,
                ..Default::default()
            };
            score_results(&mut high, "apple orange", &high_cfg);
            assert_eq!(
                high[0].url, "/a",
                "high content_all_terms_multiplier: all-content result should rank first"
            );
        }

        #[test]
        fn phrase_adjacent_multiplier_changes_ranking() {
            // 2-term query. A: all-terms title, no locations. B: all-terms content, adjacent positions.
            // Low multiplier → title (A) wins; high → adjacent phrase (B) wins.
            let a = make_result_with_excerpt_and_locations(
                "/a",
                "Apple Orange Guide",
                "no content match",
                1.0,
                None,
            );
            let b = make_result_with_excerpt_and_locations(
                "/b",
                "Other",
                "apple orange close together",
                1.0,
                Some(vec![0, 1]),
            );

            let mut low = vec![a.clone(), b.clone()];
            let low_cfg = ScoringConfig {
                phrase_adjacent_multiplier: 1.0,
                ..Default::default()
            };
            score_results(&mut low, "apple orange", &low_cfg);
            assert_eq!(
                low[0].url, "/a",
                "low phrase_adjacent_multiplier: title result should rank first"
            );

            let mut high = vec![a.clone(), b.clone()];
            let high_cfg = ScoringConfig {
                phrase_adjacent_multiplier: 5.0,
                ..Default::default()
            };
            score_results(&mut high, "apple orange", &high_cfg);
            assert_eq!(
                high[0].url, "/b",
                "high phrase_adjacent_multiplier: adjacent phrase result should rank first"
            );
        }

        #[test]
        fn custom_recency_curve_honored() {
            // Standard curve: newer → higher boost. Reversed curve: older → higher boost.
            // Same relevance on both results (identical content match); only date differs.
            let mut a =
                make_result_with_excerpt_and_locations("/a", "Other", "apple content", 1.0, None);
            a.date = days_ago(1);
            let mut b =
                make_result_with_excerpt_and_locations("/b", "Other", "apple content", 1.0, None);
            b.date = days_ago(600);

            let standard = ScoringConfig {
                recency_strategy: "custom".to_string(),
                recency_curve: vec![[0.0, 1.0], [365.0, 0.5], [730.0, 0.0]],
                ..Default::default()
            };
            let mut std_results = vec![a.clone(), b.clone()];
            score_results(&mut std_results, "apple", &standard);
            assert_eq!(
                std_results[0].url, "/a",
                "standard curve (newer=higher): recent result should rank first"
            );

            let reversed = ScoringConfig {
                recency_strategy: "custom".to_string(),
                recency_curve: vec![[0.0, 0.0], [365.0, 0.5], [730.0, 1.0]],
                ..Default::default()
            };
            let mut rev_results = vec![a.clone(), b.clone()];
            score_results(&mut rev_results, "apple", &reversed);
            assert_eq!(
                rev_results[0].url, "/b",
                "reversed curve (older=higher): older result should rank first"
            );
        }

        #[test]
        fn title_all_terms_multiplier_changes_ranking() {
            // 2-term query. A: partial title (1/2 terms) + all content (2/2).
            //               B: all title (2/2 terms), no content match.
            // Low multiplier → A's partial-title+content wins; high → B's all-title wins.
            let a = make_result_with_excerpt_and_locations(
                "/a",
                "Apple Guide",
                "apple orange content",
                1.0,
                None,
            );
            let b = make_result_with_excerpt_and_locations(
                "/b",
                "Apple Orange Product",
                "no content match",
                1.0,
                None,
            );

            let mut low = vec![a.clone(), b.clone()];
            let low_cfg = ScoringConfig {
                title_all_terms_multiplier: 0.1,
                ..Default::default()
            };
            score_results(&mut low, "apple orange", &low_cfg);
            assert_eq!(
                low[0].url, "/a",
                "low title_all_terms_multiplier: partial-title+content result should rank first"
            );

            let mut high = vec![a.clone(), b.clone()];
            let high_cfg = ScoringConfig {
                title_all_terms_multiplier: 5.0,
                ..Default::default()
            };
            score_results(&mut high, "apple orange", &high_cfg);
            assert_eq!(
                high[0].url, "/b",
                "high title_all_terms_multiplier: all-title result should rank first"
            );
        }
    }

    // -------------------------------------------------------------------
    // Recency strategy values — verify exact formula outputs
    // -------------------------------------------------------------------

    mod recency_values {
        use super::*;

        // --- Exponential strategy (default) ---

        #[test]
        fn exponential_day_0_is_boost_max() {
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(0), &config);
            assert!(
                (boost - 0.5).abs() < 0.005,
                "day 0 should be ~0.5, got {boost}"
            );
        }

        #[test]
        fn exponential_at_half_life_is_half_boost_max() {
            // At half_life_days (365): boost = boost_max × exp(-ln2) = 0.5 × 0.5 = 0.25
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(365), &config);
            assert!(
                (boost - 0.25).abs() < 0.01,
                "day 365 should be ~0.25, got {boost}"
            );
        }

        #[test]
        fn exponential_at_two_half_lives_is_quarter_boost_max() {
            // At 2×half_life_days (730): boost = boost_max × exp(-2×ln2) = 0.5 × 0.25 = 0.125
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(730), &config);
            assert!(
                (boost - 0.125).abs() < 0.01,
                "day 730 should be ~0.125, got {boost}"
            );
        }

        #[test]
        fn exponential_at_penalty_threshold_is_zero() {
            // At penalty_after_days (1825): years_over=0, penalty=0 → result=0.0
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(1825), &config);
            assert!(
                boost.abs() < 0.01,
                "day 1825 (penalty threshold) should be ~0.0, got {boost}"
            );
        }

        #[test]
        fn exponential_past_threshold_applies_penalty() {
            // 5 years past threshold (1825+5×365=3650): years_over=5, penalty=0.25
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(3650), &config);
            assert!(
                (boost - (-0.25)).abs() < 0.01,
                "day 3650 should be ~-0.25, got {boost}"
            );
        }

        #[test]
        fn exponential_penalty_capped_at_max() {
            // 6 years past threshold (1825+6×365=4015): years_over=6, uncapped=0.3, capped=0.3
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(4015), &config);
            assert!(
                (boost - (-0.3)).abs() < 0.01,
                "day 4015 should be ~-0.3 (capped), got {boost}"
            );
        }

        #[test]
        fn exponential_custom_half_life_and_boost_max() {
            // half_life=30, boost_max=1.0: day 30 → 0.5, day 60 → 0.25
            let config = ScoringConfig {
                recency_boost_max: 1.0,
                recency_half_life_days: 30,
                ..Default::default()
            };
            let boost_30 = recency_boost(&days_ago(30), &config);
            let boost_60 = recency_boost(&days_ago(60), &config);
            assert!(
                (boost_30 - 0.5).abs() < 0.01,
                "day 30 (half_life=30, max=1.0) should be ~0.5, got {boost_30}"
            );
            assert!(
                (boost_60 - 0.25).abs() < 0.01,
                "day 60 (half_life=30, max=1.0) should be ~0.25, got {boost_60}"
            );
        }

        #[test]
        fn exponential_zero_half_life_returns_zero() {
            let config = ScoringConfig {
                recency_half_life_days: 0,
                ..Default::default()
            };
            assert_eq!(
                recency_boost(&days_ago(30), &config),
                0.0,
                "half_life_days=0 should return 0.0"
            );
        }

        #[test]
        fn future_date_returns_boost_max() {
            let config = ScoringConfig::default();
            let boost = recency_boost("2999-12-31", &config);
            assert_eq!(
                boost, config.recency_boost_max,
                "future date should return boost_max"
            );
        }

        // --- Linear strategy ---

        #[test]
        fn linear_day_0_is_boost_max() {
            let config = ScoringConfig {
                recency_strategy: "linear".to_string(),
                ..Default::default()
            };
            let boost = recency_boost(&days_ago(0), &config);
            assert!(
                (boost - 0.5).abs() < 0.005,
                "linear day 0 should be boost_max (0.5), got {boost}"
            );
        }

        #[test]
        fn linear_at_half_threshold_is_quarter_boost_max() {
            // At threshold/2 (~912 days): fraction = 1 - 912/1825 ≈ 0.5, boost ≈ 0.25
            let config = ScoringConfig {
                recency_strategy: "linear".to_string(),
                ..Default::default()
            };
            let boost = recency_boost(&days_ago(913), &config);
            assert!(
                (boost - 0.25).abs() < 0.01,
                "linear at ~half threshold should be ~0.25, got {boost}"
            );
        }

        #[test]
        fn linear_at_threshold_is_zero() {
            // At penalty_after_days (1825): fraction = 1 - 1825/1825 = 0 → boost = 0
            let config = ScoringConfig {
                recency_strategy: "linear".to_string(),
                ..Default::default()
            };
            let boost = recency_boost(&days_ago(1825), &config);
            assert!(
                boost.abs() < 0.01,
                "linear at penalty threshold should be ~0.0, got {boost}"
            );
        }

        #[test]
        fn linear_past_threshold_is_negative() {
            let config = ScoringConfig {
                recency_strategy: "linear".to_string(),
                ..Default::default()
            };
            let boost = recency_boost(&days_ago(2200), &config);
            assert!(
                boost < 0.0,
                "linear past threshold should be negative, got {boost}"
            );
        }

        // --- Step strategy ---

        #[test]
        fn step_within_half_life_is_boost_max() {
            // Below half_life_days (365): full boost
            let config = ScoringConfig {
                recency_strategy: "step".to_string(),
                ..Default::default()
            };
            assert_eq!(
                recency_boost(&days_ago(1), &config),
                0.5,
                "step day 1 should be boost_max"
            );
            assert_eq!(
                recency_boost(&days_ago(364), &config),
                0.5,
                "step day 364 should be boost_max"
            );
        }

        #[test]
        fn step_at_and_beyond_half_life_is_zero() {
            // At and past half_life_days (365): `days_old < half_life` is false → 0.0
            let config = ScoringConfig {
                recency_strategy: "step".to_string(),
                ..Default::default()
            };
            assert_eq!(
                recency_boost(&days_ago(365), &config),
                0.0,
                "step at half_life should be 0.0"
            );
            assert_eq!(
                recency_boost(&days_ago(366), &config),
                0.0,
                "step past half_life should be 0.0"
            );
        }

        #[test]
        fn step_past_threshold_is_negative() {
            let config = ScoringConfig {
                recency_strategy: "step".to_string(),
                ..Default::default()
            };
            let boost = recency_boost(&days_ago(2200), &config);
            assert!(
                boost < 0.0,
                "step past penalty threshold should be negative, got {boost}"
            );
        }

        // --- Custom curve strategy ---

        #[test]
        fn custom_curve_interpolates_between_points() {
            let config = ScoringConfig {
                recency_strategy: "custom".to_string(),
                recency_curve: vec![[0.0, 1.0], [365.0, 0.5], [730.0, 0.0]],
                ..Default::default()
            };
            assert!(
                (recency_custom(0.0, &config) - 1.0).abs() < 0.001,
                "day 0 should be 1.0"
            );
            assert!(
                (recency_custom(182.0, &config) - 0.75).abs() < 0.01,
                "day 182 should be ~0.75"
            );
            assert!(
                (recency_custom(365.0, &config) - 0.5).abs() < 0.001,
                "day 365 should be 0.5"
            );
            assert!(
                (recency_custom(547.0, &config) - 0.25).abs() < 0.01,
                "day 547 should be ~0.25"
            );
            assert!(
                (recency_custom(730.0, &config) - 0.0).abs() < 0.001,
                "day 730 should be 0.0"
            );
        }

        #[test]
        fn custom_curve_clamps_beyond_last_point() {
            // Past the last point (730), result clamps to the last value (0.0)
            let config = ScoringConfig {
                recency_strategy: "custom".to_string(),
                recency_curve: vec![[0.0, 1.0], [365.0, 0.5], [730.0, 0.0]],
                ..Default::default()
            };
            assert!(
                recency_custom(1000.0, &config).abs() < 0.001,
                "past last curve point should clamp to 0.0"
            );
        }

        #[test]
        fn custom_curve_single_point_returns_that_value() {
            let config = ScoringConfig {
                recency_strategy: "custom".to_string(),
                recency_curve: vec![[100.0, 0.5]],
                ..Default::default()
            };
            assert!(
                (recency_custom(0.0, &config) - 0.5).abs() < 0.001,
                "before single point: should return its value"
            );
            assert!(
                (recency_custom(100.0, &config) - 0.5).abs() < 0.001,
                "at single point: should return its value"
            );
            assert!(
                (recency_custom(500.0, &config) - 0.5).abs() < 0.001,
                "after single point: should return its value"
            );
        }

        #[test]
        fn custom_curve_empty_returns_zero() {
            let config = ScoringConfig {
                recency_strategy: "custom".to_string(),
                recency_curve: vec![],
                ..Default::default()
            };
            assert_eq!(recency_custom(0.0, &config), 0.0, "empty curve: day 0");
            assert_eq!(recency_custom(365.0, &config), 0.0, "empty curve: day 365");
        }

        // --- None strategy ---

        #[test]
        fn none_strategy_always_returns_zero() {
            let config = ScoringConfig {
                recency_strategy: "none".to_string(),
                ..Default::default()
            };
            assert_eq!(recency_boost(&days_ago(0), &config), 0.0, "none: day 0");
            assert_eq!(recency_boost(&days_ago(365), &config), 0.0, "none: day 365");
            assert_eq!(
                recency_boost(&days_ago(3650), &config),
                0.0,
                "none: day 3650"
            );
        }

        // --- Penalty boundary tests ---

        #[test]
        fn penalty_one_year_past_threshold() {
            // 1825 + 365 = 2190 days: years_over=1, penalty=min(0.05, 0.3)=0.05
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(2190), &config);
            assert!(
                (boost - (-0.05)).abs() < 0.01,
                "1 year past threshold should be ~-0.05, got {boost}"
            );
        }

        #[test]
        fn penalty_six_years_past_threshold_is_capped() {
            // 1825 + 6×365 = 4015 days: years_over=6, uncapped=0.30, capped=0.3
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(4015), &config);
            assert!(
                (boost - (-0.3)).abs() < 0.01,
                "6 years past threshold should be ~-0.3 (capped), got {boost}"
            );
        }

        #[test]
        fn penalty_twenty_years_past_threshold_is_still_capped() {
            // 1825 + 20×365 = 9125 days: years_over=20, uncapped=1.0, capped at max_penalty=0.3
            let config = ScoringConfig::default();
            let boost = recency_boost(&days_ago(9125), &config);
            assert!(
                (boost - (-0.3)).abs() < 0.001,
                "20 years past threshold should be capped at -0.3, got {boost}"
            );
        }
    }

    // --- Phrase-proximity value tests ---
    //
    // These tests call `phrase_proximity_multiplier` directly and assert exact
    // multiplier values. They verify boundary conditions that the ranking
    // regression tests above cannot: the adjacent/near/none thresholds use
    // strict-vs-inclusive comparisons (`<` vs `<=`), and off-by-one errors in
    // those operators would silently pass the higher-level ranking tests.
    mod phrase_proximity_values {
        use super::*;

        fn cfg() -> ScoringConfig {
            ScoringConfig::default()
            // defaults: phrase_adjacent_multiplier=2.5, phrase_near_multiplier=1.5,
            //           phrase_near_window=5
        }

        fn terms(strs: &[&str]) -> Vec<String> {
            strs.iter().map(|s| s.to_string()).collect()
        }

        #[test]
        fn empty_locations_returns_no_bonus() {
            assert_eq!(
                phrase_proximity_multiplier(&terms(&["hello", "world"]), &[], &cfg()),
                1.0
            );
        }

        #[test]
        fn fewer_locations_than_terms_returns_no_bonus() {
            // 3 terms, 1 location → locations.len() < n guard → 1.0
            assert_eq!(
                phrase_proximity_multiplier(&terms(&["a", "b", "c"]), &[5], &cfg()),
                1.0
            );
        }

        #[test]
        fn single_term_returns_no_bonus() {
            // n < 2 guard fires regardless of locations
            assert_eq!(
                phrase_proximity_multiplier(&terms(&["hello"]), &[0, 1, 2], &cfg()),
                1.0
            );
        }

        #[test]
        fn duplicate_positions_are_adjacent() {
            // span = max − min = 5 − 5 = 0; 0 < n (2) → phrase_adjacent_multiplier (2.5)
            let mult =
                phrase_proximity_multiplier(&terms(&["hello", "world"]), &[5, 5, 5, 5], &cfg());
            assert!((mult - 2.5).abs() < 0.001, "expected 2.5, got {mult}");
        }

        #[test]
        fn span_n_minus_1_is_adjacent_two_terms() {
            // [10, 11]: span 1 < n (2) → adjacent (2.5)
            let mult = phrase_proximity_multiplier(&terms(&["hello", "world"]), &[10, 11], &cfg());
            assert!(
                (mult - 2.5).abs() < 0.001,
                "expected adjacent (2.5), got {mult}"
            );
        }

        #[test]
        fn span_n_minus_1_is_adjacent_three_terms() {
            // [10, 11, 12]: span 2 < n (3) → adjacent (2.5)
            let mult = phrase_proximity_multiplier(&terms(&["a", "b", "c"]), &[10, 11, 12], &cfg());
            assert!(
                (mult - 2.5).abs() < 0.001,
                "expected adjacent (2.5), got {mult}"
            );
        }

        #[test]
        fn span_eq_n_is_near_not_adjacent() {
            // [10, 12]: span 2 = n (2); NOT < 2 (not adjacent); 2 ≤ 5 (near window) → near (1.5)
            let mult = phrase_proximity_multiplier(&terms(&["hello", "world"]), &[10, 12], &cfg());
            assert!(
                (mult - 1.5).abs() < 0.001,
                "expected near (1.5), got {mult}"
            );
        }

        #[test]
        fn span_eq_phrase_near_window_is_near() {
            // [10, 15]: span 5 ≤ phrase_near_window (5) → near (1.5) — ≤ is inclusive
            let mult = phrase_proximity_multiplier(&terms(&["hello", "world"]), &[10, 15], &cfg());
            assert!(
                (mult - 1.5).abs() < 0.001,
                "expected near (1.5) at boundary, got {mult}"
            );
        }

        #[test]
        fn span_exceeds_phrase_near_window_is_no_bonus() {
            // [10, 16]: span 6 > phrase_near_window (5) → 1.0
            let mult = phrase_proximity_multiplier(&terms(&["hello", "world"]), &[10, 16], &cfg());
            assert!(
                (mult - 1.0).abs() < 0.001,
                "expected no bonus (1.0), got {mult}"
            );
        }

        #[test]
        fn unsorted_locations_finds_adjacent_window() {
            // [50, 10, 30, 11] → sorted [10, 11, 30, 50]; window [10, 11]: span 1 < 2 → adjacent
            let mult =
                phrase_proximity_multiplier(&terms(&["hello", "world"]), &[50, 10, 30, 11], &cfg());
            assert!(
                (mult - 2.5).abs() < 0.001,
                "expected adjacent (2.5) from unsorted input, got {mult}"
            );
        }

        #[test]
        fn large_position_values_no_overflow() {
            // [u32::MAX - 1, u32::MAX]: span 1 < 2 → adjacent; subtraction must not overflow
            let mult = phrase_proximity_multiplier(
                &terms(&["hello", "world"]),
                &[u32::MAX - 1, u32::MAX],
                &cfg(),
            );
            assert!(
                (mult - 2.5).abs() < 0.001,
                "expected adjacent (2.5) at u32 boundary, got {mult}"
            );
        }
    }
}
