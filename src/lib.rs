//! # Scolta Core WASM
//!
//! Browser WebAssembly module for the Scolta search engine. This crate is
//! the **source of truth** for search scoring, prompt management, query
//! expansion, context extraction, PII sanitization, and conversation trimming.
//!
//! ## Architecture
//!
//! - [`browser`] — wasm-bindgen exports (the public API)
//! - [`inner`] — Plain Rust implementations used by browser exports and tests
//! - [`common`] — Stop words, term extraction
//! - [`config`] — Scoring configuration parsing
//! - [`context`] — LLM context extraction
//! - [`conversation`] — Conversation history trimming
//! - [`error`] — Typed error handling
//! - [`expansion`] — LLM response parsing and term filtering
//! - [`prompts`] — Prompt templates
//! - [`sanitize`] — PII redaction
//! - [`scoring`] — Result scoring and ranking

pub mod browser;
pub mod common;
pub mod config;
pub mod context;
pub mod conversation;
pub mod error;
pub mod expansion;
pub mod prompts;
pub mod sanitize;
pub mod scoring;
pub mod stop_words;

use error::ScoltaError;
use serde_json::json;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// WASM interface version — tracks binary compatibility.
///
/// Increment when function signatures or calling conventions change in a way
/// that breaks binary compatibility with host wrappers (scolta-php, scolta.js).
pub const WASM_INTERFACE_VERSION: u32 = 4;

// ---------------------------------------------------------------------------
// Inner functions: plain Rust, callable from tests and browser exports.
// ---------------------------------------------------------------------------

pub mod inner {
    use super::*;

    // -----------------------------------------------------------------------
    // Prompt functions
    // -----------------------------------------------------------------------

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

        let anchors: Option<Vec<String>> = obj
            .get("dynamic_anchors")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        prompts::resolve_template(
            prompt_name,
            site_name,
            site_description,
            anchors.as_deref(),
        )
        .ok_or_else(|| ScoltaError::UnknownPrompt {
            name: prompt_name.to_string(),
        })
    }

    pub fn get_prompt(name: &str) -> Result<String, ScoltaError> {
        let name = name.trim();
        prompts::get_template(name)
            .map(|s| s.to_string())
            .ok_or_else(|| ScoltaError::UnknownPrompt {
                name: name.to_string(),
            })
    }

    // -----------------------------------------------------------------------
    // Scoring
    // -----------------------------------------------------------------------

    /// Score and re-rank search results by relevance.
    ///
    /// Input: `{ "query": "...", "results": [...], "config": {...} }`
    ///
    /// Each result may include `"source_weight"` (f64, default 1.0) to dampen
    /// secondary-source results. The config may include `"priority_pages"` to
    /// boost specific results when query keywords match.
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

    /// Merge N result sets with per-set weights, deduplication, and URL filtering.
    ///
    /// Input:
    /// ```json
    /// {
    ///   "sets": [
    ///     { "results": [...], "weight": 0.7 },
    ///     { "results": [...], "weight": 0.3 }
    ///   ],
    ///   "deduplicate_by": "url",
    ///   "case_sensitive": false,
    ///   "exclude_urls": ["https://..."],
    ///   "normalize_urls": true
    /// }
    /// ```
    pub fn merge_results(input: &serde_json::Value) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "merge_results",
            "expected JSON object",
        ))?;

        let sets_json = obj
            .get("sets")
            .ok_or(ScoltaError::missing_field("merge_results", "sets"))?;

        let sets: Vec<scoring::MergeSet> =
            serde_json::from_value(sets_json.clone()).map_err(|e| {
                ScoltaError::parse_error("merge_results", format!("failed to parse sets: {}", e))
            })?;

        let deduplicate_by = obj
            .get("deduplicate_by")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let case_sensitive = obj
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let exclude_urls: Vec<String> = obj
            .get("exclude_urls")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let normalize_urls = obj
            .get("normalize_urls")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let options = scoring::MergeOptions {
            sets,
            deduplicate_by,
            case_sensitive,
            exclude_urls,
            normalize_urls,
        };

        let merged = scoring::merge_results(options);
        serde_json::to_value(&merged).map_err(|e| ScoltaError::parse_error("merge_results", e))
    }

    /// Return the priority pages from `priority_pages` whose keywords match `query`.
    ///
    /// Input: `{ "query": "...", "priority_pages": [...] }`
    /// Output: JSON array of matching PriorityPage entries.
    pub fn match_priority_pages(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "match_priority_pages",
            "expected JSON object",
        ))?;

        let query = obj
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("match_priority_pages", "query"))?;

        let pages_json = obj.get("priority_pages").ok_or(ScoltaError::missing_field(
            "match_priority_pages",
            "priority_pages",
        ))?;

        let pages: Vec<scoring::PriorityPage> = serde_json::from_value(pages_json.clone())
            .map_err(|e| {
                ScoltaError::parse_error(
                    "match_priority_pages",
                    format!("failed to parse priority_pages: {}", e),
                )
            })?;

        let matched = scoring::match_priority_pages(query, &pages);
        serde_json::to_value(&matched)
            .map_err(|e| ScoltaError::parse_error("match_priority_pages", e))
    }

    /// Score multiple queries against their respective result sets.
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

            let config_json = qobj.get("config").unwrap_or(default_config_json);
            let cfg = config::from_json(config_json);

            scoring::score_results(&mut results, query, &cfg);

            let scored = serde_json::to_value(&results)
                .map_err(|e| ScoltaError::parse_error("batch_score_results", e))?;
            batch_results.push(scored);
        }

        Ok(serde_json::Value::Array(batch_results))
    }

    // -----------------------------------------------------------------------
    // Expansion
    // -----------------------------------------------------------------------

    /// Parse an LLM expansion response.
    ///
    /// Two forms:
    /// 1. Bare string (LLM output) — language defaults to `"en"`.
    /// 2. JSON object: `{ "text": "...", "language": "de", "generic_terms": [...],
    ///    "existing_terms": [...], ... }` — all extra fields optional.
    pub fn parse_expansion(input: &str) -> Vec<String> {
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(input) {
            if let Some(map) = obj.as_object() {
                if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                    let language = map.get("language").and_then(|v| v.as_str()).unwrap_or("en");

                    let generic_terms: Vec<String> = map
                        .get("generic_terms")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();

                    let filter_single_word_generic = map
                        .get("filter_single_word_generic")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);

                    let keep_acronyms = map
                        .get("keep_acronyms")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);

                    let keep_proper_nouns = map
                        .get("keep_proper_nouns")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);

                    let min_term_length = map
                        .get("min_term_length")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(2) as u32;

                    let existing_terms: Vec<String> = map
                        .get("existing_terms")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();

                    let cfg = expansion::ExpansionConfig {
                        language: language.to_string(),
                        generic_terms,
                        filter_single_word_generic,
                        keep_acronyms,
                        keep_proper_nouns,
                        min_term_length,
                        existing_terms,
                    };

                    return expansion::parse_expansion_with_config(text, &cfg);
                }
            }
        }

        expansion::parse_expansion(input)
    }

    pub fn parse_expansion_with_language(input: &str, language: &str) -> Vec<String> {
        expansion::parse_expansion_with_language(input, language)
    }

    // -----------------------------------------------------------------------
    // Context extraction
    // -----------------------------------------------------------------------

    /// Extract relevant context from a document for LLM summarization.
    ///
    /// Input: `{ "content": "...", "query": "...", "config": { ... } }`
    /// Output: extracted context string.
    pub fn extract_context(input: &serde_json::Value) -> Result<String, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "extract_context",
            "expected JSON object",
        ))?;

        let content = obj
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("extract_context", "content"))?;

        let query = obj
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("extract_context", "query"))?;

        let cfg = parse_context_config(obj.get("config"));
        Ok(context::extract_context(content, query, &cfg))
    }

    /// Extract context from multiple documents in one call.
    ///
    /// Input:
    /// ```json
    /// {
    ///   "items": [{ "content": "...", "url": "...", "title": "..." }],
    ///   "query": "...",
    ///   "config": { ... }
    /// }
    /// ```
    /// Output: JSON array of `{ "url": "...", "title": "...", "context": "..." }`.
    pub fn batch_extract_context(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "batch_extract_context",
            "expected JSON object",
        ))?;

        let query = obj
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("batch_extract_context", "query"))?;

        let items_json = obj
            .get("items")
            .ok_or(ScoltaError::missing_field("batch_extract_context", "items"))?;

        let items: Vec<serde_json::Value> =
            serde_json::from_value(items_json.clone()).map_err(|e| {
                ScoltaError::parse_error("batch_extract_context", format!("items: {}", e))
            })?;

        let cfg = parse_context_config(obj.get("config"));

        let context_items: Vec<context::ContextItem> = items
            .into_iter()
            .map(|item| context::ContextItem {
                content: item
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                url: item
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                title: item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
            .collect();

        let results = context::batch_extract_context(context_items, query, &cfg);

        let output: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| {
                json!({
                    "url": r.url,
                    "title": r.title,
                    "context": r.context
                })
            })
            .collect();

        serde_json::to_value(output)
            .map_err(|e| ScoltaError::parse_error("batch_extract_context", e))
    }

    fn parse_context_config(cfg_json: Option<&serde_json::Value>) -> context::ContextConfig {
        let mut cfg = context::ContextConfig::default();
        if let Some(obj) = cfg_json.and_then(|v| v.as_object()) {
            if let Some(v) = obj.get("max_length").and_then(|v| v.as_u64()) {
                cfg.max_length = v as u32;
            }
            if let Some(v) = obj.get("intro_length").and_then(|v| v.as_u64()) {
                cfg.intro_length = v as u32;
            }
            if let Some(v) = obj.get("snippet_radius").and_then(|v| v.as_u64()) {
                cfg.snippet_radius = v as u32;
            }
            if let Some(v) = obj.get("separator").and_then(|v| v.as_str()) {
                cfg.separator = v.to_string();
            }
        }
        cfg
    }

    // -----------------------------------------------------------------------
    // Sanitization
    // -----------------------------------------------------------------------

    /// Redact PII from a query before analytics logging.
    ///
    /// Input: `{ "query": "...", "config": { ... } }`
    /// Output: sanitized query string.
    pub fn sanitize_query(input: &serde_json::Value) -> Result<String, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "sanitize_query",
            "expected JSON object",
        ))?;

        let query = obj
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or(ScoltaError::missing_field("sanitize_query", "query"))?;

        let cfg = parse_sanitization_config(obj.get("config"));
        Ok(sanitize::sanitize_query(query, &cfg))
    }

    fn parse_sanitization_config(
        cfg_json: Option<&serde_json::Value>,
    ) -> sanitize::SanitizationConfig {
        let mut cfg = sanitize::SanitizationConfig::default();
        if let Some(obj) = cfg_json.and_then(|v| v.as_object()) {
            if let Some(v) = obj.get("redact_email").and_then(|v| v.as_bool()) {
                cfg.redact_email = v;
            }
            if let Some(v) = obj.get("redact_phone").and_then(|v| v.as_bool()) {
                cfg.redact_phone = v;
            }
            if let Some(v) = obj.get("redact_ssn").and_then(|v| v.as_bool()) {
                cfg.redact_ssn = v;
            }
            if let Some(v) = obj.get("redact_credit_card").and_then(|v| v.as_bool()) {
                cfg.redact_credit_card = v;
            }
            if let Some(v) = obj.get("redact_ip").and_then(|v| v.as_bool()) {
                cfg.redact_ip = v;
            }
            if let Some(arr) = obj.get("custom_patterns").and_then(|v| v.as_array()) {
                cfg.custom_patterns = arr
                    .iter()
                    .filter_map(|p| {
                        let regex = p.get("regex").and_then(|v| v.as_str())?;
                        let replacement = p.get("replacement").and_then(|v| v.as_str())?;
                        Some(sanitize::SanitizationPattern {
                            regex: regex.to_string(),
                            replacement: replacement.to_string(),
                        })
                    })
                    .collect();
            }
        }
        cfg
    }

    // -----------------------------------------------------------------------
    // Conversation truncation
    // -----------------------------------------------------------------------

    /// Trim a conversation to fit within a character limit.
    ///
    /// Input:
    /// ```json
    /// {
    ///   "messages": [{"role": "user", "content": "..."}],
    ///   "config": { "max_length": 12000, "preserve_first_n": 2, "removal_unit": 2 }
    /// }
    /// ```
    /// Output: JSON array of trimmed messages.
    pub fn truncate_conversation(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or(ScoltaError::invalid_json(
            "truncate_conversation",
            "expected JSON object",
        ))?;

        let messages_json = obj.get("messages").ok_or(ScoltaError::missing_field(
            "truncate_conversation",
            "messages",
        ))?;

        let messages: Vec<conversation::Message> = serde_json::from_value(messages_json.clone())
            .map_err(|e| {
                ScoltaError::parse_error("truncate_conversation", format!("messages: {}", e))
            })?;

        let mut cfg = conversation::ConversationConfig::default();
        if let Some(cfg_obj) = obj.get("config").and_then(|v| v.as_object()) {
            if let Some(v) = cfg_obj.get("max_length").and_then(|v| v.as_u64()) {
                cfg.max_length = v as u32;
            }
            if let Some(v) = cfg_obj.get("preserve_first_n").and_then(|v| v.as_u64()) {
                cfg.preserve_first_n = v as u32;
            }
            if let Some(v) = cfg_obj.get("removal_unit").and_then(|v| v.as_u64()) {
                cfg.removal_unit = v as u32;
            }
        }

        let trimmed = conversation::truncate_conversation(messages, &cfg);
        serde_json::to_value(&trimmed)
            .map_err(|e| ScoltaError::parse_error("truncate_conversation", e))
    }

    // -----------------------------------------------------------------------
    // Utility
    // -----------------------------------------------------------------------

    pub fn version() -> String {
        VERSION.to_string()
    }

    pub fn describe() -> serde_json::Value {
        json!({
            "name": "scolta-core",
            "version": VERSION,
            "wasm_interface_version": WASM_INTERFACE_VERSION,
            "description": "Scolta browser WASM — client-side search scoring, prompt management, query expansion, context extraction, PII sanitization, and conversation trimming",
            "functions": {
                "score_results": {
                    "description": "Score and re-rank search results; supports source_weight and priority_pages",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "json"
                },
                "merge_results": {
                    "description": "Merge N weighted result sets with deduplication and URL exclusion",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "json"
                },
                "match_priority_pages": {
                    "description": "Return priority pages whose keywords match a query",
                    "since": "0.2.3",
                    "stability": "experimental",
                    "input_type": "json",
                    "output_type": "json"
                },
                "parse_expansion": {
                    "description": "Parse an LLM expansion response with generic-term filtering and term merging",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "string",
                    "output_type": "json"
                },
                "extract_context": {
                    "description": "Extract relevant context from a document for LLM summarization",
                    "since": "0.2.3",
                    "stability": "experimental",
                    "input_type": "json",
                    "output_type": "string"
                },
                "batch_extract_context": {
                    "description": "Extract context from multiple documents in one call",
                    "since": "0.2.3",
                    "stability": "experimental",
                    "input_type": "json",
                    "output_type": "json"
                },
                "sanitize_query": {
                    "description": "Redact PII (email, phone, SSN, CC, IP) from a query before analytics logging",
                    "since": "0.2.3",
                    "stability": "experimental",
                    "input_type": "json",
                    "output_type": "string"
                },
                "truncate_conversation": {
                    "description": "Trim conversation history by removing oldest pairs to fit a character limit",
                    "since": "0.2.3",
                    "stability": "experimental",
                    "input_type": "json",
                    "output_type": "json"
                },
                "batch_score_results": {
                    "description": "Score multiple queries in a single call; returns array of scored result arrays",
                    "since": "0.2.2",
                    "stability": "experimental",
                    "input_type": "json",
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
                "version": {
                    "description": "Get the crate version",
                    "since": "0.1.0",
                    "stability": "stable",
                    "input_type": "none",
                    "output_type": "string"
                },
                "describe": {
                    "description": "Describe all exported functions (version, stability, input/output types)",
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
    fn test_wasm_interface_version_incremented() {
        assert_eq!(WASM_INTERFACE_VERSION, 4);
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
        assert!(result.unwrap().contains("Test Site"));
    }

    #[test]
    fn test_resolve_prompt_unknown() {
        let input = json!({"prompt_name": "nonexistent", "site_name": "Test"});
        assert!(inner::resolve_prompt(&input).is_err());
    }

    #[test]
    fn test_resolve_prompt_no_placeholder_no_anchors_unchanged() {
        // expand_query has no {DYNAMIC_ANCHORS}; omitting dynamic_anchors → no change.
        let without = inner::resolve_prompt(&json!({
            "prompt_name": "expand_query",
            "site_name": "Site",
            "site_description": "desc"
        })).unwrap();
        let with_none = inner::resolve_prompt(&json!({
            "prompt_name": "expand_query",
            "site_name": "Site",
            "site_description": "desc",
            "dynamic_anchors": null
        })).unwrap();
        assert_eq!(without, with_none);
    }

    #[test]
    fn test_resolve_prompt_placeholder_no_anchors_erased() {
        // summarize has {DYNAMIC_ANCHORS}; no anchors supplied → placeholder removed.
        let result = inner::resolve_prompt(&json!({
            "prompt_name": "summarize",
            "site_name": "Site",
            "site_description": "desc"
        })).unwrap();
        assert!(!result.contains("{DYNAMIC_ANCHORS}"));
    }

    #[test]
    fn test_resolve_prompt_placeholder_with_anchors_substituted() {
        // When the summarize template contains {DYNAMIC_ANCHORS}, anchors appear in output.
        // Regardless, the placeholder string must not remain in the output.
        let result = inner::resolve_prompt(&json!({
            "prompt_name": "summarize",
            "site_name": "Site",
            "site_description": "desc",
            "dynamic_anchors": ["Focus on pricing.", "Do not mention competitors."]
        })).unwrap();
        assert!(!result.contains("{DYNAMIC_ANCHORS}"));
        // When the template has the placeholder (added in commit 2.2), anchors appear.
        if prompts::SUMMARIZE.contains("{DYNAMIC_ANCHORS}") {
            assert!(result.contains("Focus on pricing."));
            assert!(result.contains("Do not mention competitors."));
        }
    }

    #[test]
    fn test_resolve_prompt_no_placeholder_anchors_ignored() {
        // expand_query has no {DYNAMIC_ANCHORS}; anchors supplied → silently ignored.
        let without_anchors = inner::resolve_prompt(&json!({
            "prompt_name": "expand_query",
            "site_name": "Site",
            "site_description": "desc"
        })).unwrap();
        let with_anchors = inner::resolve_prompt(&json!({
            "prompt_name": "expand_query",
            "site_name": "Site",
            "site_description": "desc",
            "dynamic_anchors": ["Some anchor."]
        })).unwrap();
        // Output is identical: anchors are silently dropped when no placeholder exists.
        assert_eq!(without_anchors, with_anchors);
    }

    #[test]
    fn test_get_prompt() {
        let result = inner::get_prompt("expand_query");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("alternative search terms"));
    }

    #[test]
    fn test_score_results_basic() {
        let input = json!({
            "query": "drupal",
            "results": [
                {"url": "https://a.com", "title": "About Us", "excerpt": "Company info", "date": "2020-01-01"},
                {"url": "https://b.com", "title": "Drupal Guide", "excerpt": "All about Drupal", "date": "2026-03-01"}
            ]
        });
        let result = inner::score_results(&input).unwrap();
        assert_eq!(result[0]["url"], "https://b.com");
    }

    #[test]
    fn test_score_results_source_weight() {
        let input = json!({
            "query": "drupal",
            "results": [
                {"url": "https://a.com", "title": "Drupal A", "excerpt": "Drupal content", "date": "2026-01-01", "score": 1.0, "source_weight": 1.0},
                {"url": "https://b.com", "title": "Drupal B", "excerpt": "Drupal content", "date": "2026-01-01", "score": 1.0, "source_weight": 0.3}
            ]
        });
        let result = inner::score_results(&input).unwrap();
        let score_a = result[0]["score"].as_f64().unwrap();
        let score_b = result[1]["score"].as_f64().unwrap();
        assert!(score_a > score_b);
    }

    #[test]
    fn test_score_results_priority_pages() {
        let input = json!({
            "query": "meet the team",
            "results": [
                {"url": "https://example.com/team/", "title": "Team", "excerpt": "Our team", "date": "2026-01-01", "score": 1.0},
                {"url": "https://example.com/blog/", "title": "Blog", "excerpt": "Articles", "date": "2026-01-01", "score": 1.0}
            ],
            "config": {
                "priority_pages": [{
                    "url_pattern": "/team/",
                    "keywords": ["team", "leadership"],
                    "boost": 100.0
                }]
            }
        });
        let result = inner::score_results(&input).unwrap();
        assert_eq!(result[0]["url"], "https://example.com/team/");
    }

    #[test]
    fn test_merge_results_two_sets() {
        let input = json!({
            "sets": [
                { "results": [{"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 10.0}], "weight": 0.7 },
                { "results": [
                    {"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 5.0},
                    {"url": "https://b.com", "title": "B", "excerpt": "b", "date": "2025-06-01", "score": 3.0}
                ], "weight": 0.3 }
            ],
            "deduplicate_by": "url"
        });
        let result = inner::merge_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // a.com wins dedup (0.7*10=7.0 > 0.3*5=1.5)
        assert_eq!(arr[0]["url"], "https://a.com");
    }

    #[test]
    fn test_merge_results_exclude_urls() {
        let input = json!({
            "sets": [
                { "results": [
                    {"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 5.0},
                    {"url": "https://b.com", "title": "B", "excerpt": "b", "date": "2026-01-01", "score": 3.0}
                ], "weight": 1.0 }
            ],
            "exclude_urls": ["https://a.com"]
        });
        let result = inner::merge_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["url"], "https://b.com");
    }

    #[test]
    fn test_merge_results_missing_sets() {
        let input = json!({"original": [], "expanded": []});
        assert!(inner::merge_results(&input).is_err());
    }

    #[test]
    fn test_match_priority_pages() {
        let input = json!({
            "query": "who is on the team",
            "priority_pages": [
                {"url_pattern": "/team/", "keywords": ["team", "leadership"], "boost": 100.0},
                {"url_pattern": "/contact/", "keywords": ["contact"], "boost": 50.0}
            ]
        });
        let result = inner::match_priority_pages(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["url_pattern"], "/team/");
    }

    #[test]
    fn test_parse_expansion_bare_string() {
        let terms = inner::parse_expansion(r#"["term1", "term2", "term3"]"#);
        assert_eq!(terms.len(), 3);
    }

    #[test]
    fn test_parse_expansion_object_form_with_language() {
        let terms =
            inner::parse_expansion(r#"{"text": "[\"und\", \"drupal\"]", "language": "de"}"#);
        assert!(!terms.contains(&"und".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
    }

    #[test]
    fn test_parse_expansion_object_form_generic_terms() {
        let terms = inner::parse_expansion(
            r#"{"text": "[\"team\", \"drupal\", \"platform\"]", "language": "en", "generic_terms": ["team", "platform"]}"#,
        );
        assert!(!terms.contains(&"team".to_string()));
        assert!(!terms.contains(&"platform".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
    }

    #[test]
    fn test_parse_expansion_object_form_existing_terms() {
        let terms = inner::parse_expansion(
            r#"{"text": "[\"performance\"]", "language": "en", "existing_terms": ["migration", "drupal"]}"#,
        );
        assert!(terms.contains(&"performance".to_string()));
        assert!(terms.contains(&"migration".to_string()));
        assert!(terms.contains(&"drupal".to_string()));
    }

    #[test]
    fn test_extract_context_short_unchanged() {
        let input = json!({
            "content": "Short content.",
            "query": "drupal"
        });
        let result = inner::extract_context(&input).unwrap();
        assert_eq!(result, "Short content.");
    }

    #[test]
    fn test_batch_extract_context() {
        let input = json!({
            "items": [
                {"content": "Short.", "url": "https://a.com", "title": "A"},
                {"content": "Brief.", "url": "https://b.com", "title": "B"}
            ],
            "query": "drupal"
        });
        let result = inner::batch_extract_context(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["url"], "https://a.com");
    }

    #[test]
    fn test_sanitize_query_email() {
        let input = json!({"query": "contact user@example.com please"});
        let result = inner::sanitize_query(&input).unwrap();
        assert!(result.contains("[EMAIL]"));
        assert!(!result.contains('@'));
    }

    #[test]
    fn test_sanitize_query_clean() {
        let input = json!({"query": "drupal performance tips"});
        let result = inner::sanitize_query(&input).unwrap();
        assert_eq!(result, "drupal performance tips");
    }

    #[test]
    fn test_sanitize_query_custom_config() {
        let input = json!({
            "query": "call 555-867-5309 today",
            "config": {"redact_phone": false}
        });
        let result = inner::sanitize_query(&input).unwrap();
        assert!(result.contains("555-867-5309")); // phone not redacted
    }

    #[test]
    fn test_truncate_conversation_basic() {
        let input = json!({
            "messages": [
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "hi"}
            ],
            "config": {"max_length": 1000}
        });
        let result = inner::truncate_conversation(&input).unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_truncate_conversation_removes_pairs() {
        let input = json!({
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": "initial"},
                {"role": "user", "content": "q1"},
                {"role": "assistant", "content": "a1"},
                {"role": "user", "content": "q2"},
                {"role": "assistant", "content": "a2"}
            ],
            // "sys"(3)+"initial"(7)+"q1"(2)+"a1"(2)+"q2"(2)+"a2"(2)=18; 15 forces one removal
            "config": {"max_length": 15, "preserve_first_n": 2, "removal_unit": 2}
        });
        let result = inner::truncate_conversation(&input).unwrap();
        let arr = result.as_array().unwrap();
        // system and initial preserved; oldest pair (q1+a1) removed
        assert_eq!(arr[0]["content"], "sys");
        let has_q1 = arr.iter().any(|m| m["content"] == "q1");
        assert!(!has_q1);
    }

    #[test]
    fn test_batch_score_results_basic() {
        let input = json!({
            "queries": [{
                "query": "drupal",
                "results": [
                    {"url": "https://a.com", "title": "Drupal Guide", "excerpt": "About Drupal", "date": "2026-01-01"},
                    {"url": "https://b.com", "title": "Other Page", "excerpt": "Unrelated", "date": "2026-01-01"}
                ]
            }]
        });
        let result = inner::batch_score_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0][0]["url"], "https://a.com");
    }

    #[test]
    fn test_describe() {
        let desc = inner::describe();
        assert_eq!(desc["name"], "scolta-core");
        assert_eq!(desc["wasm_interface_version"], 4);
        let functions = desc["functions"].as_object().unwrap();
        // New functions
        assert!(functions.contains_key("match_priority_pages"));
        assert!(functions.contains_key("extract_context"));
        assert!(functions.contains_key("batch_extract_context"));
        assert!(functions.contains_key("sanitize_query"));
        assert!(functions.contains_key("truncate_conversation"));
        // Stable functions still present
        assert!(functions.contains_key("score_results"));
        assert!(functions.contains_key("merge_results"));
        assert!(functions.contains_key("parse_expansion"));
        // Removed
        assert!(!functions.contains_key("to_js_scoring_config"));
        // All functions have required metadata
        for (name, info) in functions {
            assert!(info.get("since").is_some(), "{} missing 'since'", name);
            assert!(
                info.get("stability").is_some(),
                "{} missing 'stability'",
                name
            );
        }
    }

    #[test]
    fn readme_does_not_reference_old_build_target() {
        let readme = std::fs::read_to_string("README.md").expect("README.md should exist");
        assert!(
            !readme.contains("wasm32-wasip1"),
            "README.md references old build target wasm32-wasip1"
        );
        assert!(
            !readme.contains("extism"),
            "README.md references removed Extism dependency."
        );
    }
}
