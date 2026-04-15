//! # Scolta Core WASM
//!
//! Browser WebAssembly module for the Scolta search engine. This crate is
//! the **source of truth** for search scoring, prompt management, and
//! query expansion. The browser loads this module via wasm-bindgen to
//! perform all scoring client-side — no server round-trips.
//!
//! ## Architecture
//!
//! - [`browser`] — wasm-bindgen exports (the public API)
//! - [`inner`] — Plain Rust implementations used by browser exports and tests
//! - [`common`] — Stop words, term extraction
//! - [`config`] — Scoring configuration parsing
//! - [`error`] — Typed error handling
//! - [`expansion`] — LLM response parsing
//! - [`prompts`] — Prompt templates
//! - [`scoring`] — Result scoring and ranking

pub mod browser;
pub mod common;
pub mod config;
pub mod error;
pub mod expansion;
pub mod prompts;
pub mod scoring;
pub mod stop_words;

use error::ScoltaError;
use serde_json::json;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// WASM interface version — tracks binary compatibility.
pub const WASM_INTERFACE_VERSION: u32 = 3;

// ---------------------------------------------------------------------------
// Inner functions: plain Rust, callable from tests and browser exports.
// ---------------------------------------------------------------------------

/// Plain-Rust implementations that browser exports delegate to.
/// Also used directly by unit and integration tests.
pub mod inner {
    use super::*;

    /// Resolve a prompt template with site-specific variable substitution.
    pub fn resolve_prompt(input: &serde_json::Value) -> Result<String, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "resolve_prompt",
            "expected JSON object",
        ))?;

        let prompt_name = obj
            .get("prompt_name")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("resolve_prompt", "prompt_name"))?;

        let site_name = obj.get("site_name").and_then(|v| v.as_str()).unwrap_or("");

        let site_description = obj
            .get("site_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        prompts::resolve_template(prompt_name, site_name, site_description).ok_or_else(|| {
            ScoltaError::UnknownPrompt {
                name: prompt_name.to_string(),
            }
        })
    }

    /// Get a raw prompt template by name (without variable substitution).
    pub fn get_prompt(name: &str) -> Result<String, ScoltaError> {
        let name = name.trim();
        prompts::get_template(name)
            .map(|s| s.to_string())
            .ok_or_else(|| ScoltaError::UnknownPrompt {
                name: name.to_string(),
            })
    }

    /// Export scoring configuration for JavaScript frontend integration.
    pub fn to_js_scoring_config(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let cfg = config::from_json(input);
        Ok(config::to_js_scoring_config(&cfg, input))
    }

    /// Score and re-rank search results by relevance.
    pub fn score_results(input: &serde_json::Value) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "score_results",
            "expected JSON object",
        ))?;

        let query = obj
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("score_results", "query"))?;

        let results_json = obj
            .get("results")
            .ok_or(ScoltaError::missing_field("score_results", "results"))?;

        let mut results: Vec<scoring::SearchResult> = serde_json::from_value(results_json.clone())
            .map_err(|e| {
                ScoltaError::parse_error("score_results", format!("failed to parse results: {}", e))
            })?;

        let empty_config = json!({});
        let config_json = obj.get("config").unwrap_or(&empty_config);
        let cfg = config::from_json(config_json);

        scoring::score_results(&mut results, query, &cfg);

        serde_json::to_value(&results).map_err(|e| ScoltaError::parse_error("score_results", e))
    }

    /// Merge original and expanded search results with deduplication.
    pub fn merge_results(input: &serde_json::Value) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "merge_results",
            "expected JSON object",
        ))?;

        let original_json = obj
            .get("original")
            .ok_or(ScoltaError::missing_field("merge_results", "original"))?;

        let expanded_json = obj
            .get("expanded")
            .ok_or(ScoltaError::missing_field("merge_results", "expanded"))?;

        let original: Vec<scoring::SearchResult> = serde_json::from_value(original_json.clone())
            .map_err(|e| {
                ScoltaError::parse_error(
                    "merge_results",
                    format!("failed to parse original results: {}", e),
                )
            })?;

        let expanded: Vec<scoring::SearchResult> = serde_json::from_value(expanded_json.clone())
            .map_err(|e| {
                ScoltaError::parse_error(
                    "merge_results",
                    format!("failed to parse expanded results: {}", e),
                )
            })?;

        let empty_config = json!({});
        let config_json = obj.get("config").unwrap_or(&empty_config);
        let cfg = config::from_json(config_json);

        let merged = scoring::merge_results(original, expanded, &cfg);
        serde_json::to_value(&merged).map_err(|e| ScoltaError::parse_error("merge_results", e))
    }

    /// Parse an LLM expansion response into a term array (defaults to English).
    ///
    /// Also accepts a JSON object `{ "text": "...", "language": "fr" }` to
    /// specify a language for stop word filtering.
    pub fn parse_expansion(input: &str) -> Vec<String> {
        // Support JSON object form: { "text": "...", "language": "fr" }
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(input) {
            if let Some(map) = obj.as_object() {
                if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                    let language = map.get("language").and_then(|v| v.as_str()).unwrap_or("en");
                    return expansion::parse_expansion_with_language(text, language);
                }
            }
        }
        // Bare string form — default to English
        expansion::parse_expansion(input)
    }

    /// Parse an LLM expansion response with an explicit language for stop word
    /// filtering.
    pub fn parse_expansion_with_language(input: &str, language: &str) -> Vec<String> {
        expansion::parse_expansion_with_language(input, language)
    }

    /// Score multiple queries against their respective result sets.
    ///
    /// Input shape:
    /// ```json
    /// {
    ///   "queries": [
    ///     { "query": "...", "results": [...], "config": {...} }
    ///   ],
    ///   "default_config": { ... }
    /// }
    /// ```
    ///
    /// Per-query `"config"` is merged on top of `"default_config"`. Returns
    /// an array of arrays — one scored result array per input query, in the
    /// same order as the input.
    pub fn batch_score_results(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "batch_score_results",
            "expected JSON object",
        ))?;

        let queries = obj
            .get("queries")
            .and_then(|v| v.as_array())
            .ok_or(ScoltaError::missing_field("batch_score_results", "queries"))?;

        let empty_obj = serde_json::json!({});
        let default_config_json = obj.get("default_config").unwrap_or(&empty_obj);

        let mut batch_results: Vec<serde_json::Value> = Vec::with_capacity(queries.len());

        for (i, query_entry) in queries.iter().enumerate() {
            let qobj = query_entry.as_object().ok_or_else(|| {
                ScoltaError::parse_error(
                    "batch_score_results",
                    format!("queries[{}] expected object", i),
                )
            })?;

            let query = qobj.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                ScoltaError::parse_error(
                    "batch_score_results",
                    format!("queries[{}].query is required", i),
                )
            })?;

            let results_json = qobj.get("results").ok_or_else(|| {
                ScoltaError::parse_error(
                    "batch_score_results",
                    format!("queries[{}].results is required", i),
                )
            })?;

            let mut results: Vec<scoring::SearchResult> =
                serde_json::from_value(results_json.clone()).map_err(|e| {
                    ScoltaError::parse_error(
                        "batch_score_results",
                        format!("queries[{}].results: {}", i, e),
                    )
                })?;

            // Per-query config overrides default_config
            let config_json = qobj.get("config").unwrap_or(default_config_json);
            let cfg = config::from_json(config_json);

            scoring::score_results(&mut results, query, &cfg);

            let scored = serde_json::to_value(&results)
                .map_err(|e| ScoltaError::parse_error("batch_score_results", e))?;
            batch_results.push(scored);
        }

        Ok(serde_json::Value::Array(batch_results))
    }

    /// Get the crate version string.
    pub fn version() -> String {
        VERSION.to_string()
    }

    /// Describe the module's exported functions.
    pub fn describe() -> serde_json::Value {
        json!({
            "name": "scolta-core",
            "version": VERSION,
            "wasm_interface_version": WASM_INTERFACE_VERSION,
            "description": "Scolta browser WASM — client-side search scoring, prompt management, and query expansion",
            "functions": {
                "score_results": {
                    "description": "Score and re-rank search results by relevance",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "json"
                },
                "merge_results": {
                    "description": "Merge original and expanded search results with deduplication",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "json"
                },
                "parse_expansion": {
                    "description": "Parse an LLM expansion response into a term array",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "string",
                    "output_type": "json"
                },
                "resolve_prompt": {
                    "description": "Resolve a prompt template with site-specific variable substitution",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "string"
                },
                "get_prompt": {
                    "description": "Get a raw prompt template by name without substitution",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "string",
                    "output_type": "string"
                },
                "to_js_scoring_config": {
                    "description": "Export scoring config as SCREAMING_SNAKE_CASE JSON for JavaScript",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "json"
                },
                "version": {
                    "description": "Get the crate version",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "none",
                    "output_type": "string"
                },
                "batch_score_results": {
                    "description": "Score multiple queries in a single call; returns array of scored result arrays",
                    "since": "0.2.2",
                    "stability": "experimental",
                    "input_type": "json",
                    "output_type": "json"
                },
                "describe": {
                    "description": "Describe all exported functions",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "none",
                    "output_type": "json"
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(inner::version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_resolve_prompt() {
        let input = json!({
            "prompt_name": "expand_query",
            "site_name": "Test Site",
            "site_description": "a test site"
        });
        let result = inner::resolve_prompt(&input);
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Test Site"));
        assert!(text.contains("test site"));
    }

    #[test]
    fn test_resolve_prompt_unknown() {
        let input = json!({
            "prompt_name": "nonexistent",
            "site_name": "Test"
        });
        let result = inner::resolve_prompt(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_prompt_missing_field() {
        let input = json!({"site_name": "Test"});
        let result = inner::resolve_prompt(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_prompt() {
        let result = inner::get_prompt("expand_query");
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("alternative search terms"));
        assert!(text.contains("{SITE_NAME}"));
    }

    #[test]
    fn test_get_prompt_unknown() {
        let result = inner::get_prompt("fake_prompt");
        assert!(result.is_err());
    }

    #[test]
    fn test_to_js_scoring_config() {
        let input = json!({
            "recency_boost_max": 0.8,
            "ai_expand_query": false
        });
        let result = inner::to_js_scoring_config(&input).unwrap();
        assert_eq!(result["RECENCY_BOOST_MAX"], 0.8);
        assert_eq!(result["AI_EXPAND_QUERY"], false);
        assert_eq!(result["RECENCY_HALF_LIFE_DAYS"], 365);
        assert_eq!(result["LANGUAGE"], "en");
        assert_eq!(result["RECENCY_STRATEGY"], "exponential");
    }

    #[test]
    fn test_score_results() {
        let input = json!({
            "query": "drupal",
            "results": [
                {"url": "https://a.com", "title": "About Us", "excerpt": "Company info", "date": "2020-01-01"},
                {"url": "https://b.com", "title": "Drupal Guide", "excerpt": "All about Drupal", "date": "2026-03-01"}
            ],
            "config": {}
        });
        let result = inner::score_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["url"], "https://b.com");
    }

    #[test]
    fn test_merge_results() {
        let input = json!({
            "original": [
                {"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 10.0}
            ],
            "expanded": [
                {"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 5.0},
                {"url": "https://b.com", "title": "B", "excerpt": "b", "date": "2025-06-01", "score": 3.0}
            ],
            "config": {"expand_primary_weight": 0.7}
        });
        let result = inner::merge_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_parse_expansion() {
        let terms = inner::parse_expansion(r#"["term1", "term2", "term3"]"#);
        assert_eq!(terms.len(), 3);
        assert_eq!(terms[0], "term1");
    }

    #[test]
    fn test_parse_expansion_markdown() {
        let terms = inner::parse_expansion("```json\n[\"search term\", \"other\"]\n```");
        assert_eq!(terms.len(), 2);
    }

    #[test]
    fn test_parse_expansion_filters() {
        let terms = inner::parse_expansion(r#"["the", "a", "real term"]"#);
        assert!(!terms.contains(&"the".to_string()));
        assert!(terms.contains(&"real term".to_string()));
    }

    #[test]
    fn test_parse_expansion_object_form() {
        // JSON object form dispatches to parse_expansion_with_language
        let terms =
            inner::parse_expansion(r#"{"text": "[\"und\", \"drupal\"]", "language": "de"}"#);
        assert!(!terms.contains(&"und".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
    }

    #[test]
    fn test_batch_score_results_basic() {
        let input = json!({
            "queries": [
                {
                    "query": "drupal",
                    "results": [
                        {"url": "https://a.com", "title": "Drupal Guide", "excerpt": "About Drupal", "date": "2026-01-01"},
                        {"url": "https://b.com", "title": "Other Page", "excerpt": "Unrelated", "date": "2026-01-01"}
                    ]
                }
            ]
        });
        let result = inner::batch_score_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let first_batch = arr[0].as_array().unwrap();
        assert_eq!(first_batch.len(), 2);
        assert_eq!(first_batch[0]["url"], "https://a.com");
    }

    #[test]
    fn test_batch_score_results_multiple_queries() {
        let input = json!({
            "queries": [
                {
                    "query": "rust",
                    "results": [{"url": "https://a.com", "title": "Rust Guide", "excerpt": "Rust language", "date": "2026-01-01"}]
                },
                {
                    "query": "python",
                    "results": [{"url": "https://b.com", "title": "Python Docs", "excerpt": "Python language", "date": "2026-01-01"}]
                }
            ]
        });
        let result = inner::batch_score_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_batch_score_results_per_query_config() {
        let input = json!({
            "queries": [{
                "query": "test",
                "results": [{"url": "https://a.com", "title": "Test", "excerpt": "testing", "date": "2026-01-01"}],
                "config": {"language": "de"}
            }],
            "default_config": {"language": "en"}
        });
        let result = inner::batch_score_results(&input).unwrap();
        assert_eq!(result.as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_batch_score_results_empty_queries() {
        let input = json!({"queries": []});
        let result = inner::batch_score_results(&input).unwrap();
        assert_eq!(result.as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_batch_score_results_missing_queries() {
        let input = json!({});
        assert!(inner::batch_score_results(&input).is_err());
    }

    #[test]
    fn test_describe() {
        let desc = inner::describe();
        assert_eq!(desc["name"], "scolta-core");
        assert_eq!(desc["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(desc["wasm_interface_version"], 3);
        let functions = desc["functions"].as_object().unwrap();
        assert!(functions.contains_key("score_results"));
        assert!(functions.contains_key("batch_score_results"));
        assert!(functions.contains_key("describe"));
        assert!(!functions.contains_key("clean_html"));
        assert!(!functions.contains_key("debug_call"));
        for (name, info) in functions {
            assert!(info.get("since").is_some(), "{} missing 'since'", name);
            assert!(
                info.get("stability").is_some(),
                "{} missing 'stability'",
                name
            );
        }
    }
}
