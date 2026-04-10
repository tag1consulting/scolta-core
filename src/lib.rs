//! # Scolta Core WASM
//!
//! Canonical WebAssembly module for the Scolta search engine. This crate is
//! the **source of truth** for search scoring, HTML processing, prompt
//! management, and query expansion. All platform adapters (PHP, Python, JS,
//! Go) call into this WASM module to get identical behavior.
//!
//! ## Architecture
//!
//! Each `#[plugin_fn]` is a thin wrapper that deserializes Extism host input,
//! calls a plain Rust function in [`inner`], and serializes the output.
//!
//! Tests and `debug_call` use the `inner::` functions directly because
//! `#[plugin_fn]` replaces the function signature with
//! `extern "C" fn() -> i32` (not callable from Rust).
//!
//! ## Features
//!
//! - `extism` (default) — Server-side Extism PDK exports for `wasm32-wasip1`.
//!   Includes HTML processing (`clean_html`, `build_pagefind_html`) which
//!   requires the `regex` crate.
//! - `browser` — Client-side wasm-bindgen exports for `wasm32-unknown-unknown`.
//!   Exposes scoring, merging, expansion parsing, prompts, and config to JS.
//!
//! These features are mutually exclusive.
//!
//! ## Modules
//!
//! - [`common`] — Shared constants (stop words, term extraction)
//! - [`error`] — Typed error handling ([`ScoltaError`](error::ScoltaError))
//! - [`prompts`] — Prompt template management
//! - [`html`] — HTML cleaning and Pagefind integration (server-only)
//! - [`scoring`] — Search result scoring and ranking
//! - [`config`] — Configuration parsing and export
//! - [`expansion`] — LLM response parsing
//! - [`debug`] — Performance monitoring (server-only)
//! - [`browser`] — Browser wasm-bindgen exports (browser-only)

// Mutual exclusion guard: extism and browser features cannot be combined.
#[cfg(all(feature = "extism", feature = "browser"))]
compile_error!(
    "Features 'extism' and 'browser' are mutually exclusive. \
     Build with --features extism (server) OR --no-default-features --features browser (client)."
);

pub mod common;
pub mod config;
pub mod error;
pub mod expansion;
pub mod prompts;
pub mod scoring;

#[cfg(feature = "extism")]
pub mod debug;
#[cfg(feature = "extism")]
pub mod html;

#[cfg(feature = "browser")]
pub mod browser;

use error::ScoltaError;
#[cfg(feature = "extism")]
use extism_pdk::*;
use serde_json::json;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// WASM interface version — tracks binary compatibility between scolta-core
/// and its host wrappers (scolta-php, scolta-python, scolta.js).
///
/// Increment this when function signatures or calling conventions change
/// in a way that requires wrapper updates. See VERSIONING.md.
pub const WASM_INTERFACE_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Inner functions: plain Rust, callable from tests and debug_call.
// ---------------------------------------------------------------------------

/// Plain-Rust implementations that the `#[plugin_fn]` wrappers delegate to.
/// Also used directly by `debug_call` and unit/integration tests.
pub mod inner {
    use super::*;

    /// Resolve a prompt template with site-specific variable substitution.
    ///
    /// Input fields:
    /// - `prompt_name` (required): One of "expand_query", "summarize", "follow_up"
    /// - `site_name` (optional): Site name to substitute
    /// - `site_description` (optional): Site description to substitute
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
    ///
    /// Input: plain string — one of "expand_query", "summarize", "follow_up"
    pub fn get_prompt(name: &str) -> Result<String, ScoltaError> {
        let name = name.trim();
        prompts::get_template(name)
            .map(|s| s.to_string())
            .ok_or_else(|| ScoltaError::UnknownPrompt {
                name: name.to_string(),
            })
    }

    /// Clean HTML by removing chrome and extracting main content.
    #[cfg(feature = "extism")]
    pub fn clean_html(input: &serde_json::Value) -> Result<String, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "clean_html",
            "expected JSON object",
        ))?;

        let raw_html = obj
            .get("html")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("clean_html", "html"))?;

        let title = obj.get("title").and_then(|v| v.as_str()).unwrap_or("");

        Ok(html::clean_html(raw_html, title))
    }

    /// Build a Pagefind-compatible HTML document.
    #[cfg(feature = "extism")]
    pub fn build_pagefind_html(input: &serde_json::Value) -> Result<String, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "build_pagefind_html",
            "expected JSON object",
        ))?;

        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("build_pagefind_html", "id"))?;

        let title = obj
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("build_pagefind_html", "title"))?;

        let body = obj
            .get("body")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("build_pagefind_html", "body"))?;

        let url = obj
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("build_pagefind_html", "url"))?;

        let date = obj.get("date").and_then(|v| v.as_str()).unwrap_or("");

        let site_name = obj.get("site_name").and_then(|v| v.as_str()).unwrap_or("");

        Ok(html::build_pagefind_html(
            id, title, body, url, date, site_name,
        ))
    }

    /// Export scoring configuration for JavaScript frontend integration.
    ///
    /// This is a convenience function for JS consumers. Other language
    /// adapters should use the config struct directly.
    pub fn to_js_scoring_config(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let cfg = config::from_json(input);
        Ok(config::to_js_scoring_config(&cfg, input))
    }

    /// Score and re-rank search results by relevance.
    ///
    /// Input fields:
    /// - `query` (required): Search query string
    /// - `results` (required): Array of SearchResult objects
    /// - `config` (optional): ScoringConfig overrides
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
    ///
    /// Input fields:
    /// - `original` (required): Results from primary search
    /// - `expanded` (required): Results from expanded query
    /// - `config` (optional): ScoringConfig overrides (includes `expand_primary_weight`)
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

    /// Parse an LLM expansion response into a term array.
    pub fn parse_expansion(input: &str) -> Vec<String> {
        expansion::parse_expansion(input)
    }

    /// Get the crate version string.
    pub fn version() -> String {
        VERSION.to_string()
    }

    /// Describe the module's exported functions.
    ///
    /// Returns a JSON object describing every WASM export: name, description,
    /// input format, and output format. This makes the module self-documenting
    /// for platform adapter developers.
    pub fn describe() -> serde_json::Value {
        json!({
            "name": "scolta-core",
            "version": VERSION,
            "wasm_interface_version": WASM_INTERFACE_VERSION,
            "description": "Scolta search engine core — scoring, HTML processing, prompt management, and query expansion",
            "functions": {
                "resolve_prompt": {
                    "description": "Resolve a prompt template with site-specific variable substitution",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "input_fields": {
                        "prompt_name": {"type": "string", "required": true, "values": ["expand_query", "summarize", "follow_up"]},
                        "site_name": {"type": "string", "required": false, "default": ""},
                        "site_description": {"type": "string", "required": false, "default": ""}
                    },
                    "output_type": "string",
                    "output_description": "Resolved prompt text with placeholders replaced"
                },
                "get_prompt": {
                    "description": "Get a raw prompt template by name without substitution",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "string",
                    "input_description": "Prompt name: expand_query, summarize, or follow_up",
                    "output_type": "string",
                    "output_description": "Raw template with {SITE_NAME} and {SITE_DESCRIPTION} placeholders"
                },
                "clean_html": {
                    "description": "Strip page chrome and extract main content as plain text",
                    "since": "0.1.0",
                    "stability": "stable",
                    "target": "server",
                    "input_type": "json",
                    "input_fields": {
                        "html": {"type": "string", "required": true},
                        "title": {"type": "string", "required": false, "default": ""}
                    },
                    "output_type": "string",
                    "output_description": "Cleaned plain text suitable for search indexing"
                },
                "build_pagefind_html": {
                    "description": "Generate Pagefind-compatible HTML for search indexing",
                    "since": "0.1.0",
                    "stability": "stable",
                    "target": "server",
                    "input_type": "json",
                    "input_fields": {
                        "id": {"type": "string", "required": true},
                        "title": {"type": "string", "required": true},
                        "body": {"type": "string", "required": true},
                        "url": {"type": "string", "required": true},
                        "date": {"type": "string", "required": false, "default": ""},
                        "site_name": {"type": "string", "required": false, "default": ""}
                    },
                    "output_type": "string",
                    "output_description": "Complete HTML document with data-pagefind-* attributes"
                },
                "score_results": {
                    "description": "Score and re-rank search results by relevance",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "input_fields": {
                        "query": {"type": "string", "required": true},
                        "results": {"type": "array<SearchResult>", "required": true},
                        "config": {"type": "ScoringConfig", "required": false}
                    },
                    "output_type": "json",
                    "output_description": "Array of SearchResult objects sorted by score (descending)"
                },
                "merge_results": {
                    "description": "Merge original and expanded search results with deduplication",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "input_fields": {
                        "original": {"type": "array<SearchResult>", "required": true},
                        "expanded": {"type": "array<SearchResult>", "required": true},
                        "config": {"type": "ScoringConfig", "required": false}
                    },
                    "output_type": "json",
                    "output_description": "Merged, deduplicated array sorted by combined score"
                },
                "to_js_scoring_config": {
                    "description": "Export scoring config as SCREAMING_SNAKE_CASE JSON for JavaScript frontends",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "input_description": "ScoringConfig fields plus optional AI toggle flags",
                    "output_type": "json",
                    "output_description": "Config object with uppercase keys for window.scolta"
                },
                "parse_expansion": {
                    "description": "Parse an LLM expansion response into a term array",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "string",
                    "input_description": "Raw LLM response (JSON array, markdown-wrapped, or newline/comma separated)",
                    "output_type": "json",
                    "output_description": "Array of cleaned search terms"
                },
                "version": {
                    "description": "Get the crate version",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "none",
                    "output_type": "string"
                },
                "describe": {
                    "description": "Describe all exported functions (this output)",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "none",
                    "output_type": "json"
                },
                "debug_call": {
                    "description": "Profile any function with timing and size metrics",
                    "since": "0.1.0",
                    "stability": "stable",
                    "target": "server",
                    "input_type": "json",
                    "input_fields": {
                        "function": {"type": "string", "required": true},
                        "input": {"type": "string", "required": false, "default": ""}
                    },
                    "output_type": "json",
                    "output_description": "{output, error, time_us, input_size, output_size}"
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Extism plugin exports (server-side only)
// ---------------------------------------------------------------------------

#[cfg(feature = "extism")]
mod extism_exports {
    use super::*;

    /// Resolve a prompt template with site-specific details.
    ///
    /// Input JSON: `{"prompt_name": "expand_query", "site_name": "...", "site_description": "..."}`
    /// Output: Resolved prompt string
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn resolve_prompt(Json(input): Json<serde_json::Value>) -> FnResult<String> {
        Ok(inner::resolve_prompt(&input)?)
    }

    /// Get raw prompt template by name.
    ///
    /// Input: Plain string prompt name (e.g., "expand_query")
    /// Output: Raw template with {SITE_NAME} and {SITE_DESCRIPTION} placeholders
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn get_prompt(input: String) -> FnResult<String> {
        Ok(inner::get_prompt(&input)?)
    }

    /// Clean HTML by removing chrome and extracting main content.
    ///
    /// Input JSON: `{"html": "...", "title": "..."}`
    /// Output: Cleaned plain text
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn clean_html(Json(input): Json<serde_json::Value>) -> FnResult<String> {
        Ok(inner::clean_html(&input)?)
    }

    /// Build a Pagefind-compatible HTML document.
    ///
    /// Input JSON: `{"id": "...", "title": "...", "body": "...", "url": "...", "date": "...", "site_name": "..."}`
    /// Output: Complete HTML document with data-pagefind-* attributes
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn build_pagefind_html(Json(input): Json<serde_json::Value>) -> FnResult<String> {
        Ok(inner::build_pagefind_html(&input)?)
    }

    /// Export scoring configuration for JavaScript integration.
    ///
    /// Input JSON: scoring config fields + AI toggle fields
    /// Output: Configuration object with SCREAMING_SNAKE_CASE keys
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn to_js_scoring_config(
        Json(input): Json<serde_json::Value>,
    ) -> FnResult<Json<serde_json::Value>> {
        inner::to_js_scoring_config(&input)
            .map(Json)
            .map_err(|e| e.into())
    }

    /// Score and re-rank search results.
    ///
    /// Input JSON: `{"query": "...", "results": [...], "config": {...}}`
    /// Output: Scored and sorted results array
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn score_results(
        Json(input): Json<serde_json::Value>,
    ) -> FnResult<Json<serde_json::Value>> {
        inner::score_results(&input).map(Json).map_err(|e| e.into())
    }

    /// Merge original and expanded search results.
    ///
    /// Input JSON: `{"original": [...], "expanded": [...], "config": {...}}`
    /// Output: Merged and deduplicated results array
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn merge_results(
        Json(input): Json<serde_json::Value>,
    ) -> FnResult<Json<serde_json::Value>> {
        inner::merge_results(&input).map(Json).map_err(|e| e.into())
    }

    /// Parse LLM expansion response into term array.
    ///
    /// Input: Plain text (JSON array, markdown-wrapped, or newline-separated)
    /// Output: JSON array of cleaned search terms
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn parse_expansion(input: String) -> FnResult<Json<Vec<String>>> {
        Ok(Json(inner::parse_expansion(&input)))
    }

    /// Get current crate version.
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn version(_: ()) -> FnResult<String> {
        Ok(inner::version())
    }

    /// Describe all exported functions.
    ///
    /// Output: JSON object with function names, descriptions, input/output formats,
    /// lifecycle status (since, stability), and WASM interface version.
    /// Enables self-discovery for platform adapter developers.
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn describe(_: ()) -> FnResult<Json<serde_json::Value>> {
        Ok(Json(inner::describe()))
    }

    /// Debug call wrapper for performance profiling.
    ///
    /// Input JSON: `{"function": "clean_html", "input": "{...}"}`
    /// Output JSON: `{"output": "..." | null, "error": "..." | null, "time_us": N, "input_size": N, "output_size": N}`
    ///
    /// # Stability
    /// - **Status:** stable
    /// - **Since:** 0.1.0
    #[plugin_fn]
    pub fn debug_call(Json(input): Json<serde_json::Value>) -> FnResult<Json<serde_json::Value>> {
        let obj = input
            .as_object()
            .ok_or_else(|| ScoltaError::invalid_json("debug_call", "expected JSON object"))?;

        let function = obj
            .get("function")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ScoltaError::missing_field("debug_call", "function"))?;

        let call_input = obj.get("input").and_then(|v| v.as_str()).unwrap_or("");

        // IMPORTANT: This match must cover all #[plugin_fn] exports except debug_call itself.
        // When adding a new export, add its case here too.
        let result = match function {
            "resolve_prompt" => debug::measure_call(function, call_input, || {
                let parsed = serde_json::from_str(call_input).unwrap_or(json!({}));
                inner::resolve_prompt(&parsed).map_err(|e| e.to_string())
            }),
            "get_prompt" => debug::measure_call(function, call_input, || {
                inner::get_prompt(call_input).map_err(|e| e.to_string())
            }),
            "clean_html" => debug::measure_call(function, call_input, || {
                let parsed = serde_json::from_str(call_input).unwrap_or(json!({}));
                inner::clean_html(&parsed).map_err(|e| e.to_string())
            }),
            "build_pagefind_html" => debug::measure_call(function, call_input, || {
                let parsed = serde_json::from_str(call_input).unwrap_or(json!({}));
                inner::build_pagefind_html(&parsed).map_err(|e| e.to_string())
            }),
            "to_js_scoring_config" => debug::measure_call(function, call_input, || {
                let parsed = serde_json::from_str(call_input).unwrap_or(json!({}));
                inner::to_js_scoring_config(&parsed)
                    .map(|v| v.to_string())
                    .map_err(|e| e.to_string())
            }),
            "score_results" => debug::measure_call(function, call_input, || {
                let parsed = serde_json::from_str(call_input).unwrap_or(json!({}));
                inner::score_results(&parsed)
                    .map(|v| v.to_string())
                    .map_err(|e| e.to_string())
            }),
            "merge_results" => debug::measure_call(function, call_input, || {
                let parsed = serde_json::from_str(call_input).unwrap_or(json!({}));
                inner::merge_results(&parsed)
                    .map(|v| v.to_string())
                    .map_err(|e| e.to_string())
            }),
            "parse_expansion" => debug::measure_call(function, call_input, || {
                Ok(serde_json::to_string(&inner::parse_expansion(call_input)).unwrap_or_default())
            }),
            "version" => debug::measure_call(function, call_input, || Ok(inner::version())),
            "describe" => {
                debug::measure_call(function, call_input, || Ok(inner::describe().to_string()))
            }
            _ => {
                return Err(ScoltaError::UnknownFunction {
                    name: function.to_string(),
                }
                .into());
            }
        };

        Ok(Json(debug::debug_result_to_json(&result)))
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
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
        assert!(err.contains("resolve_prompt"));
    }

    #[test]
    fn test_resolve_prompt_missing_field() {
        let input = json!({"site_name": "Test"});
        let result = inner::resolve_prompt(&input);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("prompt_name"));
        assert!(err.contains("resolve_prompt"));
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

    #[cfg(feature = "extism")]
    #[test]
    fn test_clean_html() {
        let input = json!({
            "html": "<html><body><p>Hello World</p><script>evil()</script></body></html>",
            "title": ""
        });
        let result = inner::clean_html(&input).unwrap();
        assert!(result.contains("Hello World"));
        assert!(!result.contains("evil"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn test_build_pagefind_html() {
        let input = json!({
            "id": "doc-42",
            "title": "Test Page",
            "body": "Clean content",
            "url": "https://example.com/test",
            "date": "2026-04-01",
            "site_name": "Example"
        });
        let result = inner::build_pagefind_html(&input).unwrap();
        assert!(result.contains("data-pagefind-body"));
        assert!(result.contains("doc-42"));
        assert!(result.contains("data-pagefind-filter=\"site:Example\""));
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
    fn test_describe() {
        let desc = inner::describe();
        assert_eq!(desc["name"], "scolta-core");
        assert_eq!(desc["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(desc["wasm_interface_version"], 1);
        let functions = desc["functions"].as_object().unwrap();
        assert!(functions.contains_key("score_results"));
        assert!(functions.contains_key("describe"));
        assert!(functions.contains_key("debug_call"));
        // Every function must have lifecycle metadata per VERSIONING.md.
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
