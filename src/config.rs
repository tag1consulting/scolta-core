//! Configuration parsing utilities.
//!
//! [`from_json`] parses a JSON object into a [`ScoringConfig`], using
//! defaults for missing fields. Returns warnings for out-of-range values
//! via [`from_json_validated`].

use crate::scoring::{ConfigWarning, ScoringConfig};

/// Parse a JSON object into a ScoringConfig, returning warnings for
/// any values outside their reasonable ranges.
pub fn from_json_validated(json: &serde_json::Value) -> (ScoringConfig, Vec<ConfigWarning>) {
    let config = from_json(json);
    let warnings = config.validate();
    (config, warnings)
}

/// Parse a JSON object into a ScoringConfig.
///
/// Missing fields use defaults. Wrong types use defaults silently.
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
        phrase_adjacent_multiplier: obj
            .get("phrase_adjacent_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(2.5),
        phrase_near_multiplier: obj
            .get("phrase_near_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.5),
        phrase_near_window: obj
            .get("phrase_near_window")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32,
        phrase_window: obj
            .get("phrase_window")
            .and_then(|v| v.as_u64())
            .unwrap_or(15) as u32,
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
        language: obj
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("en")
            .to_string(),
        custom_stop_words: obj
            .get("custom_stop_words")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        recency_strategy: obj
            .get("recency_strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("exponential")
            .to_string(),
        recency_curve: obj
            .get("recency_curve")
            .and_then(|v| serde_json::from_value::<Vec<[f64; 2]>>(v.clone()).ok())
            .unwrap_or_default(),
        priority_pages: obj
            .get("priority_pages")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_from_json_defaults() {
        let json = json!({});
        let config = from_json(&json);
        assert_eq!(config.recency_boost_max, 0.5);
        assert_eq!(config.recency_half_life_days, 365);
        assert_eq!(config.content_all_terms_multiplier, 0.48);
        assert_eq!(config.language, "en");
        assert!(config.custom_stop_words.is_empty());
        assert_eq!(config.recency_strategy, "exponential");
        assert!(config.recency_curve.is_empty());
        assert!(config.priority_pages.is_empty());
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
        assert_eq!(config.content_match_boost, 0.4);
    }

    #[test]
    fn test_from_json_language_fields() {
        let json = json!({
            "language": "de",
            "custom_stop_words": ["scolta", "pagefind"],
        });
        let config = from_json(&json);
        assert_eq!(config.language, "de");
        assert_eq!(config.custom_stop_words, vec!["scolta", "pagefind"]);
    }

    #[test]
    fn test_from_json_priority_pages() {
        let json = json!({
            "priority_pages": [
                {
                    "url_pattern": "/team/",
                    "keywords": ["team", "leadership"],
                    "boost": 100.0
                }
            ]
        });
        let config = from_json(&json);
        assert_eq!(config.priority_pages.len(), 1);
        assert_eq!(config.priority_pages[0].url_pattern, "/team/");
        assert_eq!(config.priority_pages[0].boost, 100.0);
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
        assert_eq!(config.recency_boost_max, 10.0);
        assert!(warnings.len() >= 2);
    }

    #[test]
    fn test_from_json_validated_no_warnings() {
        let json = json!({});
        let (_, warnings) = from_json_validated(&json);
        assert!(warnings.is_empty());
    }
}
