//! Configuration parsing and export utilities.
//!
//! Two entry points:
//! - [`from_json`] parses a JSON object into a [`ScoringConfig`], using
//!   defaults for missing fields. Returns warnings for out-of-range values.
//! - [`to_js_scoring_config`] exports config as uppercase-key JSON for
//!   JavaScript frontend integration (`window.scolta`).
//!
//! # Note on `to_js_scoring_config`
//!
//! This is a convenience function for the JavaScript frontend, not part of
//! the core scoring API. It transforms config keys to SCREAMING_SNAKE_CASE
//! and passes through AI feature flags that the frontend needs but the
//! scoring engine does not. Language adapters other than JavaScript should
//! use `from_json` and the `ScoringConfig` struct directly.

use crate::scoring::{ConfigWarning, ScoringConfig};
use serde_json::json;

/// Parse a JSON object into a ScoringConfig, returning warnings for
/// any values outside their reasonable ranges.
///
/// Missing fields use defaults. Wrong types use defaults. This is
/// intentionally permissive — but unlike a silent default, the warnings
/// tell the caller what happened.
///
/// # Arguments
/// * `json` - JSON object with configuration fields
///
/// # Returns
/// Tuple of (config, warnings). Warnings are empty if all values are valid.
pub fn from_json_validated(json: &serde_json::Value) -> (ScoringConfig, Vec<ConfigWarning>) {
    let config = from_json(json);
    let warnings = config.validate();
    (config, warnings)
}

/// Parse a JSON object into a ScoringConfig.
///
/// Missing fields use defaults. Wrong types use defaults silently.
/// For diagnostics, use [`from_json_validated`] instead.
pub fn from_json(json: &serde_json::Value) -> ScoringConfig {
    let empty = serde_json::Map::new();
    let obj = json.as_object().unwrap_or(&empty);

    ScoringConfig {
        recency_boost_max: obj
            .get("recency_boost_max")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5),
        recency_half_life_days: obj
            .get("recency_half_life_days")
            .and_then(|v| v.as_u64())
            .unwrap_or(365) as u32,
        recency_penalty_after_days: obj
            .get("recency_penalty_after_days")
            .and_then(|v| v.as_u64())
            .unwrap_or(1825) as u32,
        recency_max_penalty: obj
            .get("recency_max_penalty")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.3),
        title_match_boost: obj
            .get("title_match_boost")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0),
        title_all_terms_multiplier: obj
            .get("title_all_terms_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.5),
        content_match_boost: obj
            .get("content_match_boost")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.4),
        content_all_terms_multiplier: obj
            .get("content_all_terms_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.48),
        expand_primary_weight: obj
            .get("expand_primary_weight")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7),
        excerpt_length: obj
            .get("excerpt_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(300) as u32,
        results_per_page: obj
            .get("results_per_page")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32,
        max_pagefind_results: obj
            .get("max_pagefind_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as u32,
    }
}

/// Export scoring configuration as JSON for JavaScript frontend integration.
///
/// Returns the shape expected by `window.scolta` in the JavaScript frontend,
/// with all configuration parameters as SCREAMING_SNAKE_CASE keys.
///
/// AI toggle fields (`ai_expand_query`, `ai_summarize`, etc.) are passed
/// through from the input JSON since they're frontend feature flags, not
/// part of the scoring algorithm.
///
/// # Note
///
/// This is a convenience function for JavaScript consumers. Other language
/// adapters should use `from_json` directly and map field names in their
/// own bridge code.
pub fn to_js_scoring_config(
    config: &ScoringConfig,
    input: &serde_json::Value,
) -> serde_json::Value {
    let obj = input.as_object();

    let ai_expand_query = obj
        .and_then(|o| o.get("ai_expand_query"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let ai_summarize = obj
        .and_then(|o| o.get("ai_summarize"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let ai_summary_top_n = obj
        .and_then(|o| o.get("ai_summary_top_n"))
        .and_then(|v| v.as_u64())
        .unwrap_or(5);
    let ai_summary_max_chars = obj
        .and_then(|o| o.get("ai_summary_max_chars"))
        .and_then(|v| v.as_u64())
        .unwrap_or(2000);
    let ai_max_followups = obj
        .and_then(|o| o.get("ai_max_followups"))
        .and_then(|v| v.as_u64())
        .unwrap_or(3);

    json!({
        "RECENCY_BOOST_MAX": config.recency_boost_max,
        "RECENCY_HALF_LIFE_DAYS": config.recency_half_life_days,
        "RECENCY_PENALTY_AFTER_DAYS": config.recency_penalty_after_days,
        "RECENCY_MAX_PENALTY": config.recency_max_penalty,
        "TITLE_MATCH_BOOST": config.title_match_boost,
        "TITLE_ALL_TERMS_MULTIPLIER": config.title_all_terms_multiplier,
        "CONTENT_MATCH_BOOST": config.content_match_boost,
        "CONTENT_ALL_TERMS_MULTIPLIER": config.content_all_terms_multiplier,
        "EXCERPT_LENGTH": config.excerpt_length,
        "RESULTS_PER_PAGE": config.results_per_page,
        "MAX_PAGEFIND_RESULTS": config.max_pagefind_results,
        "AI_EXPAND_QUERY": ai_expand_query,
        "AI_SUMMARIZE": ai_summarize,
        "AI_SUMMARY_TOP_N": ai_summary_top_n,
        "AI_SUMMARY_MAX_CHARS": ai_summary_max_chars,
        "EXPAND_PRIMARY_WEIGHT": config.expand_primary_weight,
        "AI_MAX_FOLLOWUPS": ai_max_followups
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_js_scoring_config() {
        let config = ScoringConfig::default();
        let input = json!({});
        let result = to_js_scoring_config(&config, &input);

        assert_eq!(result["RECENCY_BOOST_MAX"], 0.5);
        assert_eq!(result["RECENCY_HALF_LIFE_DAYS"], 365);
        assert_eq!(result["CONTENT_ALL_TERMS_MULTIPLIER"], 0.48);
        assert_eq!(result["AI_EXPAND_QUERY"], true);
        assert_eq!(result["AI_SUMMARIZE"], true);
        assert_eq!(result["AI_SUMMARY_TOP_N"], 5);
        assert_eq!(result["AI_MAX_FOLLOWUPS"], 3);
    }

    #[test]
    fn test_to_js_scoring_config_custom() {
        let config = ScoringConfig {
            recency_boost_max: 0.8,
            recency_half_life_days: 200,
            ..Default::default()
        };
        let input = json!({});
        let result = to_js_scoring_config(&config, &input);
        assert_eq!(result["RECENCY_BOOST_MAX"], 0.8);
        assert_eq!(result["RECENCY_HALF_LIFE_DAYS"], 200);
    }

    #[test]
    fn test_to_js_scoring_config_ai_toggles() {
        let config = ScoringConfig::default();
        let input = json!({
            "ai_expand_query": false,
            "ai_summarize": false,
            "ai_summary_top_n": 3,
            "ai_summary_max_chars": 1000,
            "ai_max_followups": 5,
        });
        let result = to_js_scoring_config(&config, &input);
        assert_eq!(result["AI_EXPAND_QUERY"], false);
        assert_eq!(result["AI_SUMMARIZE"], false);
        assert_eq!(result["AI_SUMMARY_TOP_N"], 3);
        assert_eq!(result["AI_SUMMARY_MAX_CHARS"], 1000);
        assert_eq!(result["AI_MAX_FOLLOWUPS"], 5);
    }

    #[test]
    fn test_from_json_defaults() {
        let json = json!({});
        let config = from_json(&json);
        assert_eq!(config.recency_boost_max, 0.5);
        assert_eq!(config.recency_half_life_days, 365);
        assert_eq!(config.content_all_terms_multiplier, 0.48);
    }

    #[test]
    fn test_from_json_custom() {
        let json = json!({
            "recency_boost_max": 0.8,
            "recency_half_life_days": 200,
            "content_all_terms_multiplier": 0.6,
        });
        let config = from_json(&json);
        assert_eq!(config.recency_boost_max, 0.8);
        assert_eq!(config.recency_half_life_days, 200);
        assert_eq!(config.content_all_terms_multiplier, 0.6);
        assert_eq!(config.content_match_boost, 0.4); // Default
    }

    #[test]
    fn test_from_json_non_object() {
        let json = json!("not an object");
        let config = from_json(&json);
        assert_eq!(config.recency_boost_max, 0.5);
    }

    #[test]
    fn test_from_json_validated_warns() {
        let json = json!({"recency_boost_max": 10.0, "results_per_page": 0});
        let (config, warnings) = from_json_validated(&json);
        assert_eq!(config.recency_boost_max, 10.0); // Still uses the value
        assert!(warnings.len() >= 2);
    }

    #[test]
    fn test_from_json_validated_no_warnings() {
        let json = json!({});
        let (_, warnings) = from_json_validated(&json);
        assert!(warnings.is_empty());
    }
}
