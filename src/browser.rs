//! wasm-bindgen exports — the public API of scolta-core.
//!
//! These functions wrap `inner::` functions with wasm-bindgen serialization.
//! The business logic lives in `inner::` — this module is the boundary layer.
//!
//! # Exported functions
//!
//! - [`score_results`] — Score and rank search results
//! - [`merge_results`] — Merge original + expanded results
//! - [`parse_expansion`] — Parse LLM expansion response (string or JSON object)
//! - [`batch_score_results`] — Score multiple queries in one call
//! - [`resolve_prompt`] — Resolve prompt template
//! - [`get_prompt`] — Get raw prompt template
//! - [`to_js_scoring_config`] — Export scoring config for JS
//! - [`version`] — Get crate version
//! - [`describe`] — Self-describing function manifest
//!
//! # Not exported
//!
//! - `clean_html`, `build_pagefind_html` — build-time only (server WASM)
//! - `debug_call` — server-side profiling tool

use wasm_bindgen::prelude::*;

use crate::inner;

/// Score search results against a query.
///
/// Input: JSON string with shape:
///   `{ "query": "search terms", "results": [...], "config": {...} }`
///
/// Output: JSON string — array of scored results, sorted descending.
#[wasm_bindgen]
pub fn score_results(input: &str) -> Result<String, JsError> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| JsError::new(&format!("Invalid JSON: {}", e)))?;
    let result = inner::score_results(&value).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&result)
        .map_err(|e| JsError::new(&format!("JSON serialization failed: {}", e)))
}

/// Merge original and expanded search results.
///
/// Input: JSON string with shape:
///   `{ "original": [...], "expanded": [...], "config": {...} }`
///
/// Output: JSON string — merged and deduplicated results.
#[wasm_bindgen]
pub fn merge_results(input: &str) -> Result<String, JsError> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| JsError::new(&format!("Invalid JSON: {}", e)))?;
    let result = inner::merge_results(&value).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&result)
        .map_err(|e| JsError::new(&format!("JSON serialization failed: {}", e)))
}

/// Parse an LLM expansion response into individual search terms.
///
/// Accepts two input forms:
///
/// 1. **Bare string** — treated as the raw LLM response; language defaults to `"en"`.
///    ```text
///    ["term1", "term2"]
///    ```
///
/// 2. **JSON object** — allows specifying a language for stop word filtering.
///    ```json
///    { "text": "[\"term1\", \"term2\"]", "language": "de" }
///    ```
///
/// Output: JSON string — array of extracted, filtered terms.
#[wasm_bindgen]
pub fn parse_expansion(input: &str) -> Result<String, JsError> {
    let terms = inner::parse_expansion(input);
    serde_json::to_string(&terms)
        .map_err(|e| JsError::new(&format!("JSON serialization failed: {}", e)))
}

/// Score multiple queries against their respective result sets in a single call.
///
/// Input: JSON string with shape:
/// ```json
/// {
///   "queries": [
///     { "query": "search terms", "results": [...], "config": {...} },
///     { "query": "other query",  "results": [...] }
///   ],
///   "default_config": { "language": "en" }
/// }
/// ```
///
/// Per-query `"config"` overrides `"default_config"` for that entry.
///
/// Output: JSON string — array of arrays of scored results, one inner array
/// per input query, in the same order.
#[wasm_bindgen]
pub fn batch_score_results(input: &str) -> Result<String, JsError> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| JsError::new(&format!("Invalid JSON: {}", e)))?;
    let result = inner::batch_score_results(&value).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&result)
        .map_err(|e| JsError::new(&format!("JSON serialization failed: {}", e)))
}

/// Resolve a prompt template with variable substitution.
///
/// Input: JSON string with shape:
///   `{ "prompt_name": "expand_query", "site_name": "...", "site_description": "..." }`
///
/// Output: The resolved prompt string.
#[wasm_bindgen]
pub fn resolve_prompt(input: &str) -> Result<String, JsError> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| JsError::new(&format!("Invalid JSON: {}", e)))?;
    inner::resolve_prompt(&value).map_err(|e| JsError::new(&e.to_string()))
}

/// Get a raw prompt template by name.
///
/// Input: Prompt name string ("expand_query", "summarize", "follow_up").
/// Output: Raw template string with {SITE_NAME} and {SITE_DESCRIPTION} placeholders.
#[wasm_bindgen]
pub fn get_prompt(name: &str) -> Result<String, JsError> {
    inner::get_prompt(name).map_err(|e| JsError::new(&e.to_string()))
}

/// Convert scoring config to JavaScript-friendly format.
///
/// Input: JSON string of scoring config.
/// Output: JSON string with JS-style keys (UPPER_SNAKE_CASE).
#[wasm_bindgen]
pub fn to_js_scoring_config(input: &str) -> Result<String, JsError> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| JsError::new(&format!("Invalid JSON: {}", e)))?;
    let result = inner::to_js_scoring_config(&value).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&result)
        .map_err(|e| JsError::new(&format!("JSON serialization failed: {}", e)))
}

/// Return the scolta-core version string.
#[wasm_bindgen]
pub fn version() -> String {
    inner::version()
}

/// Return a JSON description of all available functions.
#[wasm_bindgen]
pub fn describe() -> String {
    serde_json::to_string(&inner::describe()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::inner;

    #[test]
    fn score_results_json_roundtrip() {
        let input = serde_json::json!({
            "query": "rust programming",
            "results": [
                {"title": "Learn Rust", "url": "/rust", "excerpt": "Rust programming language", "date": "2025-01-01"},
                {"title": "Go Tutorial", "url": "/go", "excerpt": "Go programming", "date": "2025-01-01"}
            ]
        });
        let result = inner::score_results(&input).unwrap();
        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        let score0 = arr[0]["score"].as_f64().unwrap();
        let score1 = arr[1]["score"].as_f64().unwrap();
        assert!(
            score0 >= score1,
            "Results should be sorted by score descending"
        );
    }

    #[test]
    fn parse_expansion_json_output() {
        let terms = inner::parse_expansion("[\"cost\", \"pricing\", \"rates\"]");
        assert_eq!(terms, vec!["cost", "pricing", "rates"]);
    }

    #[test]
    fn merge_results_json_roundtrip() {
        let input = serde_json::json!({
            "original": [
                {"title": "Page A", "url": "/a", "score": 0.9, "excerpt": "a", "date": "2025-01-01"}
            ],
            "expanded": [
                {"title": "Page B", "url": "/b", "score": 0.8, "excerpt": "b", "date": "2025-01-01"}
            ],
            "config": {"expand_primary_weight": 0.7}
        });
        let result = inner::merge_results(&input).unwrap();
        assert!(result.is_array());
    }

    #[test]
    fn resolve_prompt_roundtrip() {
        let input = serde_json::json!({
            "prompt_name": "expand_query",
            "site_name": "TestSite",
            "site_description": "a test site"
        });
        let result = inner::resolve_prompt(&input).unwrap();
        assert!(result.contains("TestSite"));
    }

    #[test]
    fn to_js_scoring_config_roundtrip() {
        let input = serde_json::json!({"recency_boost_max": 0.8});
        let result = inner::to_js_scoring_config(&input).unwrap();
        assert_eq!(result["RECENCY_BOOST_MAX"], 0.8);
    }

    #[test]
    fn parse_expansion_object_form_with_language() {
        // JSON object form: { "text": "...", "language": "de" }
        let input = serde_json::json!({
            "text": r#"["und", "drupal", "suche"]"#,
            "language": "de"
        });
        let terms = inner::parse_expansion_with_language(
            input["text"].as_str().unwrap(),
            input["language"].as_str().unwrap_or("en"),
        );
        assert!(!terms.contains(&"und".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
    }

    #[test]
    fn batch_score_results_basic() {
        let input = serde_json::json!({
            "queries": [
                {
                    "query": "drupal performance",
                    "results": [
                        {"url": "https://a.com", "title": "Drupal Speed Guide", "excerpt": "Improve Drupal performance", "date": "2026-01-01"},
                        {"url": "https://b.com", "title": "About Us", "excerpt": "Company info", "date": "2026-01-01"}
                    ]
                },
                {
                    "query": "rust programming",
                    "results": [
                        {"url": "https://c.com", "title": "Learn Rust", "excerpt": "Rust language guide", "date": "2026-01-01"}
                    ]
                }
            ]
        });
        let result = inner::batch_score_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // First batch: 2 results
        assert_eq!(arr[0].as_array().unwrap().len(), 2);
        // First result of first batch should be Drupal-related
        assert_eq!(arr[0][0]["url"], "https://a.com");
        // Second batch: 1 result
        assert_eq!(arr[1].as_array().unwrap().len(), 1);
    }

    #[test]
    fn batch_score_results_default_config() {
        let input = serde_json::json!({
            "queries": [
                {
                    "query": "search",
                    "results": [
                        {"url": "https://a.com", "title": "Search Results", "excerpt": "find things", "date": "2026-01-01"}
                    ]
                }
            ],
            "default_config": { "language": "en", "recency_boost_max": 0.0 }
        });
        let result = inner::batch_score_results(&input).unwrap();
        assert_eq!(result.as_array().unwrap().len(), 1);
    }

    #[test]
    fn batch_score_results_per_query_config_overrides_default() {
        let input = serde_json::json!({
            "queries": [
                {
                    "query": "test",
                    "results": [
                        {"url": "https://a.com", "title": "Test Page", "excerpt": "testing", "date": "2026-01-01"}
                    ],
                    "config": { "recency_boost_max": 0.1 }
                }
            ],
            "default_config": { "recency_boost_max": 0.9 }
        });
        let result = inner::batch_score_results(&input).unwrap();
        assert_eq!(result.as_array().unwrap().len(), 1);
    }
}
