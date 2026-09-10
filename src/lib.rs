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

/// Forward config-clamp warnings to the host: `console.warn` in the browser,
/// stderr on native targets (tests, tooling).
pub(crate) fn emit_config_warnings(function: &str, warnings: &[scoring::ConfigWarning]) {
    for w in warnings {
        warn_host(&format!(
            "scolta-core {}: config field '{}': {}",
            function, w.field, w.message
        ));
    }
}

#[cfg(target_arch = "wasm32")]
fn warn_host(msg: &str) {
    browser::console_warn(msg);
}

#[cfg(not(target_arch = "wasm32"))]
fn warn_host(msg: &str) {
    eprintln!("{}", msg);
}

// ---------------------------------------------------------------------------
// Inner functions: plain Rust, callable from tests and browser exports.
// ---------------------------------------------------------------------------

pub mod inner {
    use super::*;

    /// Fetch a required string field, distinguishing an absent field
    /// (`MissingField`) from a present-but-wrong-typed one (`InvalidFieldType`).
    fn require_str<'a>(
        obj: &'a serde_json::Map<String, serde_json::Value>,
        function: &'static str,
        field: &'static str,
    ) -> Result<&'a str, ScoltaError> {
        match obj.get(field) {
            None => Err(ScoltaError::missing_field(function, field)),
            Some(v) => v
                .as_str()
                .ok_or_else(|| ScoltaError::invalid_field_type(function, field, "a string")),
        }
    }

    // -----------------------------------------------------------------------
    // Prompt functions
    // -----------------------------------------------------------------------

    /// Resolve a prompt template with site-specific variable substitution.
    ///
    /// Input: `{ "prompt_name": "...", "site_name": "...", "site_description": "...",
    /// "dynamic_anchors": [...] }` (all but `prompt_name` optional).
    ///
    /// # Errors
    /// `InvalidJson` for a non-object input, `MissingField`/`InvalidFieldType`
    /// for a bad `prompt_name`, `UnknownPrompt` for an unrecognized template.
    pub fn resolve_prompt(input: &serde_json::Value) -> Result<String, ScoltaError> {
        let obj = input
            .as_object()
            .ok_or_else(|| ScoltaError::invalid_json("resolve_prompt", "expected JSON object"))?;

        let prompt_name = require_str(obj, "resolve_prompt", "prompt_name")?;

        let site_name = obj.get("site_name").and_then(|v| v.as_str()).unwrap_or("");
        let site_description = obj
            .get("site_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let anchors: Option<Vec<String>> = obj
            .get("dynamic_anchors")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        prompts::resolve_template(prompt_name, site_name, site_description, anchors.as_deref())
            .ok_or_else(|| ScoltaError::UnknownPrompt {
                name: prompt_name.to_string(),
            })
    }

    /// Get a raw prompt template by (trimmed) name, without substitution.
    ///
    /// # Errors
    /// `UnknownPrompt` if the name does not match a template.
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
    ///
    /// # Errors
    /// `InvalidJson` for non-object input, `MissingField`/`InvalidFieldType`
    /// for a bad `query`, `MissingField`/`ParseError` for bad `results`.
    pub fn score_results(input: &serde_json::Value) -> Result<serde_json::Value, ScoltaError> {
        let obj = input
            .as_object()
            .ok_or_else(|| ScoltaError::invalid_json("score_results", "expected JSON object"))?;

        let query = require_str(obj, "score_results", "query")?;

        let results_json = obj
            .get("results")
            .ok_or_else(|| ScoltaError::missing_field("score_results", "results"))?;

        let mut results: Vec<scoring::SearchResult> = serde_json::from_value(results_json.clone())
            .map_err(|e| {
                ScoltaError::parse_error("score_results", format!("failed to parse results: {}", e))
            })?;

        let empty_config = json!({});
        let config_json = obj.get("config").unwrap_or(&empty_config);
        let (cfg, warnings) = config::from_json_validated(config_json);
        emit_config_warnings("score_results", &warnings);

        let primary_terms: Option<Vec<String>> = obj
            .get("primary_query")
            .and_then(|v| v.as_str())
            .map(|pq| common::extract_query(pq, &cfg.language).terms);

        let sort_override: Option<scoring::SortOverride> = obj
            .get("sort_override")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        // Quoted forced-phrase queries return only results that can contain
        // the exact phrase — see retain_forced_phrase_matches. A no-op for
        // every unquoted query.
        scoring::retain_forced_phrase_matches(&mut results, query, &cfg);

        scoring::score_results_with_primary(&mut results, query, primary_terms.as_deref(), &cfg);

        if let Some(ref sort) = sort_override {
            scoring::apply_sort_override(&mut results, sort);
        }

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
    ///
    /// # Errors
    /// `InvalidJson` for non-object input, `MissingField`/`ParseError` for a
    /// missing or unparseable `sets`.
    pub fn merge_results(input: &serde_json::Value) -> Result<serde_json::Value, ScoltaError> {
        let obj = input
            .as_object()
            .ok_or_else(|| ScoltaError::invalid_json("merge_results", "expected JSON object"))?;

        let sets_json = obj
            .get("sets")
            .ok_or_else(|| ScoltaError::missing_field("merge_results", "sets"))?;

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

        let debug_mode = obj.get("debug").and_then(|v| v.as_bool()).unwrap_or(false);

        let options = scoring::MergeOptions {
            sets,
            deduplicate_by,
            case_sensitive,
            exclude_urls,
            normalize_urls,
        };

        if debug_mode {
            let (merged, debug_info) = scoring::merge_results_with_debug(options);
            let results_val = serde_json::to_value(&merged)
                .map_err(|e| ScoltaError::parse_error("merge_results", e))?;
            let debug_val = serde_json::to_value(&debug_info)
                .map_err(|e| ScoltaError::parse_error("merge_results", e))?;
            Ok(serde_json::json!({ "results": results_val, "debug": debug_val }))
        } else {
            let merged = scoring::merge_results(options);
            serde_json::to_value(&merged).map_err(|e| ScoltaError::parse_error("merge_results", e))
        }
    }

    /// Return the priority pages from `priority_pages` whose keywords match `query`.
    ///
    /// Input: `{ "query": "...", "priority_pages": [...] }`
    /// Output: JSON array of matching PriorityPage entries.
    ///
    /// # Errors
    /// `InvalidJson` for non-object input, `MissingField`/`InvalidFieldType`
    /// for a bad `query`, `MissingField`/`ParseError` for bad `priority_pages`.
    pub fn match_priority_pages(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or_else(|| {
            ScoltaError::invalid_json("match_priority_pages", "expected JSON object")
        })?;

        let query = require_str(obj, "match_priority_pages", "query")?;

        let pages_json = obj
            .get("priority_pages")
            .ok_or_else(|| ScoltaError::missing_field("match_priority_pages", "priority_pages"))?;

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
    ///
    /// # Errors
    /// `InvalidJson` for non-object input, `MissingField`/`InvalidFieldType`
    /// for a bad `queries`, `ParseError` for an entry lacking `query`/`results`.
    pub fn batch_score_results(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or_else(|| {
            ScoltaError::invalid_json("batch_score_results", "expected JSON object")
        })?;

        let queries = match obj.get("queries") {
            None => {
                return Err(ScoltaError::missing_field("batch_score_results", "queries"));
            }
            Some(v) => v.as_array().ok_or_else(|| {
                ScoltaError::invalid_field_type("batch_score_results", "queries", "an array")
            })?,
        };

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
            let (cfg, warnings) = config::from_json_validated(config_json);
            emit_config_warnings("batch_score_results", &warnings);

            // Same forced-phrase exclusion as score_results, so a quoted
            // query scored through the batch path behaves identically.
            scoring::retain_forced_phrase_matches(&mut results, query, &cfg);

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
                        .map_or(2, config::saturate_u32);

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
    ///
    /// # Errors
    /// `InvalidJson` for non-object input, `MissingField`/`InvalidFieldType`
    /// for a bad `content` or `query`.
    pub fn extract_context(input: &serde_json::Value) -> Result<String, ScoltaError> {
        let obj = input
            .as_object()
            .ok_or_else(|| ScoltaError::invalid_json("extract_context", "expected JSON object"))?;

        let content = require_str(obj, "extract_context", "content")?;

        let query = require_str(obj, "extract_context", "query")?;

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
    ///
    /// # Errors
    /// `InvalidJson` for non-object input, `MissingField`/`InvalidFieldType`
    /// for a bad `query`, `MissingField`/`ParseError` for bad `items`.
    pub fn batch_extract_context(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or_else(|| {
            ScoltaError::invalid_json("batch_extract_context", "expected JSON object")
        })?;

        let query = require_str(obj, "batch_extract_context", "query")?;

        let items_json = obj
            .get("items")
            .ok_or_else(|| ScoltaError::missing_field("batch_extract_context", "items"))?;

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
                cfg.max_length = config::saturate_u32(v);
            }
            if let Some(v) = obj.get("intro_length").and_then(|v| v.as_u64()) {
                cfg.intro_length = config::saturate_u32(v);
            }
            if let Some(v) = obj.get("snippet_radius").and_then(|v| v.as_u64()) {
                cfg.snippet_radius = config::saturate_u32(v);
            }
            if let Some(v) = obj.get("separator").and_then(|v| v.as_str()) {
                cfg.separator = v.to_string();
            }
            if let Some(v) = obj.get("language").and_then(|v| v.as_str()) {
                cfg.language = v.to_string();
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
    ///
    /// Custom patterns are validated up front: a malformed entry or an invalid
    /// regex is an error, never a silently skipped redaction.
    ///
    /// # Errors
    /// `InvalidJson` for non-object input, `MissingField`/`InvalidFieldType`
    /// for a bad `query` or `custom_patterns`, `ParseError` for a malformed
    /// pattern entry or invalid regex.
    pub fn sanitize_query(input: &serde_json::Value) -> Result<String, ScoltaError> {
        let obj = input
            .as_object()
            .ok_or_else(|| ScoltaError::invalid_json("sanitize_query", "expected JSON object"))?;

        let query = require_str(obj, "sanitize_query", "query")?;

        let cfg = parse_sanitization_config(obj.get("config"))?;
        Ok(sanitize::sanitize_query(query, &cfg))
    }

    fn parse_sanitization_config(
        cfg_json: Option<&serde_json::Value>,
    ) -> Result<sanitize::SanitizationConfig, ScoltaError> {
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
            if let Some(patterns_json) = obj.get("custom_patterns") {
                let arr = patterns_json.as_array().ok_or_else(|| {
                    ScoltaError::invalid_field_type("sanitize_query", "custom_patterns", "an array")
                })?;
                let mut patterns = Vec::with_capacity(arr.len());
                for (i, p) in arr.iter().enumerate() {
                    let regex = p.get("regex").and_then(|v| v.as_str()).ok_or_else(|| {
                        ScoltaError::parse_error(
                            "sanitize_query",
                            format!("custom_patterns[{}]: missing or non-string 'regex'", i),
                        )
                    })?;
                    let replacement =
                        p.get("replacement")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                ScoltaError::parse_error(
                                    "sanitize_query",
                                    format!(
                                        "custom_patterns[{}]: missing or non-string 'replacement'",
                                        i
                                    ),
                                )
                            })?;
                    let compiled = sanitize::SanitizationPattern {
                        regex: regex.to_string(),
                        replacement: replacement.to_string(),
                    }
                    .compile()
                    .map_err(|e| {
                        ScoltaError::parse_error(
                            "sanitize_query",
                            format!("custom_patterns[{}]: invalid regex: {}", i, e),
                        )
                    })?;
                    patterns.push(compiled);
                }
                cfg.custom_patterns = patterns;
            }
        }
        Ok(cfg)
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
    ///
    /// # Errors
    /// `InvalidJson` for non-object input, `MissingField`/`ParseError` for a
    /// missing or unparseable `messages`.
    pub fn truncate_conversation(
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ScoltaError> {
        let obj = input.as_object().ok_or_else(|| {
            ScoltaError::invalid_json("truncate_conversation", "expected JSON object")
        })?;

        let messages_json = obj
            .get("messages")
            .ok_or_else(|| ScoltaError::missing_field("truncate_conversation", "messages"))?;

        let messages: Vec<conversation::Message> = serde_json::from_value(messages_json.clone())
            .map_err(|e| {
                ScoltaError::parse_error("truncate_conversation", format!("messages: {}", e))
            })?;

        let mut cfg = conversation::ConversationConfig::default();
        if let Some(cfg_obj) = obj.get("config").and_then(|v| v.as_object()) {
            if let Some(v) = cfg_obj.get("max_length").and_then(|v| v.as_u64()) {
                cfg.max_length = config::saturate_u32(v);
            }
            if let Some(v) = cfg_obj.get("preserve_first_n").and_then(|v| v.as_u64()) {
                cfg.preserve_first_n = config::saturate_u32(v);
            }
            if let Some(v) = cfg_obj.get("removal_unit").and_then(|v| v.as_u64()) {
                cfg.removal_unit = config::saturate_u32(v);
            }
        }

        let trimmed = conversation::truncate_conversation(messages, &cfg);
        serde_json::to_value(&trimmed)
            .map_err(|e| ScoltaError::parse_error("truncate_conversation", e))
    }

    // -----------------------------------------------------------------------
    // Utility
    // -----------------------------------------------------------------------

    /// Return the crate version string (`CARGO_PKG_VERSION`).
    pub fn version() -> String {
        VERSION.to_string()
    }

    /// Build the runtime function manifest: name, version, WASM interface
    /// version, and per-function `since`/`stability`/IO metadata. This is the
    /// single source of truth host adapters read at startup.
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
                    "stability": "stable",
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
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "string"
                },
                "batch_extract_context": {
                    "description": "Extract context from multiple documents in one call",
                    "since": "0.2.3",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "json"
                },
                "sanitize_query": {
                    "description": "Redact PII (email, phone, SSN, CC, IP) from a query before analytics logging",
                    "since": "0.2.3",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "string"
                },
                "truncate_conversation": {
                    "description": "Trim conversation history by removing oldest pairs to fit a character limit",
                    "since": "0.2.3",
                    "stability": "stable",
                    "input_type": "json",
                    "output_type": "json"
                },
                "batch_score_results": {
                    "description": "Score multiple queries in a single call; returns array of scored result arrays",
                    "since": "0.2.2",
                    "stability": "stable",
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
        }))
        .unwrap();
        let with_none = inner::resolve_prompt(&json!({
            "prompt_name": "expand_query",
            "site_name": "Site",
            "site_description": "desc",
            "dynamic_anchors": null
        }))
        .unwrap();
        assert_eq!(without, with_none);
    }

    #[test]
    fn test_resolve_prompt_placeholder_no_anchors_erased() {
        // summarize has {DYNAMIC_ANCHORS}; no anchors supplied → placeholder removed.
        let result = inner::resolve_prompt(&json!({
            "prompt_name": "summarize",
            "site_name": "Site",
            "site_description": "desc"
        }))
        .unwrap();
        assert!(!result.contains("{DYNAMIC_ANCHORS}"));
    }

    #[test]
    fn test_resolve_prompt_placeholder_with_anchors_substituted() {
        // summarize template contains {DYNAMIC_ANCHORS}; anchors must appear in output.
        let result = inner::resolve_prompt(&json!({
            "prompt_name": "summarize",
            "site_name": "Site",
            "site_description": "desc",
            "dynamic_anchors": ["Focus on pricing.", "Do not mention competitors."]
        }))
        .unwrap();
        assert!(!result.contains("{DYNAMIC_ANCHORS}"));
        assert!(result.contains("Focus on pricing."));
        assert!(result.contains("Do not mention competitors."));
    }

    #[test]
    fn test_resolve_prompt_no_placeholder_anchors_ignored() {
        // expand_query has no {DYNAMIC_ANCHORS}; anchors supplied → silently ignored.
        let without_anchors = inner::resolve_prompt(&json!({
            "prompt_name": "expand_query",
            "site_name": "Site",
            "site_description": "desc"
        }))
        .unwrap();
        let with_anchors = inner::resolve_prompt(&json!({
            "prompt_name": "expand_query",
            "site_name": "Site",
            "site_description": "desc",
            "dynamic_anchors": ["Some anchor."]
        }))
        .unwrap();
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

    // ---- expansion-diagnostic debug field ----

    #[test]
    fn test_merge_results_no_debug_returns_array() {
        let input = json!({
            "sets": [
                { "results": [{"url": "https://a.com", "title": "A", "excerpt": "", "date": "", "score": 1.0}], "weight": 1.0 }
            ]
        });
        let result = inner::merge_results(&input).unwrap();
        assert!(
            result.is_array(),
            "without debug flag, output must be a plain array"
        );
    }

    #[test]
    fn test_merge_results_debug_true_returns_object() {
        let input = json!({
            "sets": [
                { "results": [{"url": "https://a.com", "title": "A", "excerpt": "", "date": "", "score": 1.0}], "weight": 1.0 }
            ],
            "debug": true
        });
        let result = inner::merge_results(&input).unwrap();
        assert!(
            result.is_object(),
            "with debug:true, output must be an object"
        );
        assert!(
            result.get("results").is_some(),
            "debug output must contain 'results' key"
        );
        assert!(
            result.get("debug").is_some(),
            "debug output must contain 'debug' key"
        );
    }

    #[test]
    fn test_merge_results_debug_tracks_input_counts() {
        let input = json!({
            "sets": [
                { "results": [
                    {"url": "https://a.com", "title": "A", "excerpt": "", "date": "", "score": 2.0},
                    {"url": "https://b.com", "title": "B", "excerpt": "", "date": "", "score": 1.0}
                ], "weight": 1.0 },
                { "results": [
                    {"url": "https://c.com", "title": "C", "excerpt": "", "date": "", "score": 1.0}
                ], "weight": 0.5 }
            ],
            "debug": true
        });
        let result = inner::merge_results(&input).unwrap();
        let debug = &result["debug"];
        let sets = debug["sets"].as_array().unwrap();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0]["input_count"], 2);
        assert_eq!(sets[1]["input_count"], 1);
    }

    #[test]
    fn test_merge_results_debug_tracks_dedup_reduction() {
        let input = json!({
            "sets": [
                { "results": [
                    {"url": "https://a.com", "title": "A", "excerpt": "", "date": "", "score": 2.0},
                    {"url": "https://a.com", "title": "A dup", "excerpt": "", "date": "", "score": 1.0},
                    {"url": "https://b.com", "title": "B", "excerpt": "", "date": "", "score": 0.5}
                ], "weight": 1.0 }
            ],
            "deduplicate_by": "url",
            "debug": true
        });
        let result = inner::merge_results(&input).unwrap();
        let debug = &result["debug"];
        assert_eq!(debug["total_before_dedup"], 3);
        assert_eq!(debug["total_after_dedup"], 2);
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
    fn test_extract_context_language_config_plumbed() {
        // "der" is a German stop word: under language "de" the query has no
        // meaningful terms, so the deep occurrence must not be snippet-anchored.
        let content = format!("{}xx der yy {}", "A ".repeat(2500), "B ".repeat(2000));
        let de = inner::extract_context(&json!({
            "content": content,
            "query": "der",
            "config": {"max_length": 3000, "intro_length": 500, "snippet_radius": 30, "language": "de"}
        }))
        .unwrap();
        assert!(!de.contains("xx der"));

        let en = inner::extract_context(&json!({
            "content": content,
            "query": "der",
            "config": {"max_length": 3000, "intro_length": 500, "snippet_radius": 30, "language": "en"}
        }))
        .unwrap();
        assert!(en.contains("xx der yy"));
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
    fn test_sanitize_query_custom_pattern_redacts() {
        let input = json!({
            "query": "patient MRN-12345 admitted",
            "config": {
                "custom_patterns": [
                    {"regex": r"\bMRN-\d{5}\b", "replacement": "[PATIENT_ID]"}
                ]
            }
        });
        let result = inner::sanitize_query(&input).unwrap();
        assert!(result.contains("[PATIENT_ID]"));
        assert!(!result.contains("MRN-12345"));
    }

    #[test]
    fn test_sanitize_query_invalid_custom_pattern_is_err() {
        // A typo'd regex must be a hard error, not a silently skipped redaction.
        let input = json!({
            "query": "patient MRN-12345 admitted",
            "config": {
                "custom_patterns": [
                    {"regex": r"\b(MRN-\d{5}\b", "replacement": "[PATIENT_ID]"}
                ]
            }
        });
        let err = inner::sanitize_query(&input).unwrap_err();
        assert!(err.to_string().contains("invalid regex"));
    }

    #[test]
    fn test_sanitize_query_malformed_pattern_entry_is_err() {
        // Entry without a replacement must error instead of being dropped.
        let input = json!({
            "query": "patient MRN-12345 admitted",
            "config": {"custom_patterns": [{"regex": r"\bMRN-\d{5}\b"}]}
        });
        let err = inner::sanitize_query(&input).unwrap_err();
        assert!(err.to_string().contains("replacement"));
    }

    #[test]
    fn test_sanitize_query_custom_patterns_wrong_type_is_err() {
        let input = json!({
            "query": "hello",
            "config": {"custom_patterns": "not an array"}
        });
        let err = inner::sanitize_query(&input).unwrap_err();
        assert!(err
            .to_string()
            .contains("'custom_patterns' must be an array"));
    }

    #[test]
    fn test_sanitize_query_custom_pattern_not_recompiled_per_call() {
        let input = json!({
            "query": "id COMPILE-ONCE-9999 here",
            "config": {
                "custom_patterns": [
                    {"regex": r"\bCOMPILE-ONCE-\d{4}\b", "replacement": "[ID]"}
                ]
            }
        });
        inner::sanitize_query(&input).unwrap();
        let count_after_first = sanitize::custom_regex_compile_count();
        inner::sanitize_query(&input).unwrap();
        inner::sanitize_query(&input).unwrap();
        assert_eq!(
            sanitize::custom_regex_compile_count(),
            count_after_first,
            "same custom pattern must not recompile on every sanitize_query call"
        );
    }

    #[test]
    fn test_score_results_out_of_range_config_clamped() {
        // recency_boost_max far above its 2.0 ceiling must behave exactly like 2.0.
        let results = json!([
            {"url": "https://a.com", "title": "Fresh", "excerpt": "x", "date": "2026-06-01", "score": 1.0}
        ]);
        let absurd = inner::score_results(&json!({
            "query": "fresh",
            "results": results,
            "config": {"recency_boost_max": 100.0}
        }))
        .unwrap();
        let clamped = inner::score_results(&json!({
            "query": "fresh",
            "results": results,
            "config": {"recency_boost_max": 2.0}
        }))
        .unwrap();
        assert_eq!(
            absurd[0]["score"], clamped[0]["score"],
            "out-of-range recency_boost_max must be clamped to 2.0 in the export path"
        );
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

    // -------------------------------------------------------------------
    // Malformed input handling
    //
    // Verifies that every inner:: entry point returns Err (never panics)
    // when given structurally or semantically invalid JSON. Also verifies
    // that edge-case valid inputs (empty arrays, missing optional fields)
    // succeed rather than error.
    // -------------------------------------------------------------------
    mod malformed_input {
        use super::*;

        // ---- score_results ----

        #[test]
        fn score_results_array_input_is_err() {
            assert!(inner::score_results(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn score_results_missing_query_is_err() {
            assert!(inner::score_results(&json!({"results": []})).is_err());
        }

        #[test]
        fn score_results_missing_results_is_err() {
            assert!(inner::score_results(&json!({"query": "test"})).is_err());
        }

        #[test]
        fn score_results_query_is_number_is_err() {
            // query field exists but is not a string → InvalidFieldType, not MissingField
            let err = inner::score_results(&json!({"query": 42, "results": []})).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("'query' must be a string"),
                "wrong-typed field must report a type error, got: {}",
                msg
            );
            assert!(
                !msg.contains("missing required field"),
                "wrong-typed field must not be reported as missing, got: {}",
                msg
            );
        }

        #[test]
        fn missing_query_still_reported_as_missing() {
            let err = inner::score_results(&json!({"results": []})).unwrap_err();
            assert!(err.to_string().contains("missing required field 'query'"));
        }

        #[test]
        fn batch_score_results_queries_is_string_is_type_err() {
            let err = inner::batch_score_results(&json!({"queries": "not an array"})).unwrap_err();
            assert!(err.to_string().contains("'queries' must be an array"));
        }

        #[test]
        fn extract_context_content_is_number_is_type_err() {
            let err = inner::extract_context(&json!({"content": 7, "query": "test"})).unwrap_err();
            assert!(err.to_string().contains("'content' must be a string"));
        }

        #[test]
        fn score_results_results_is_string_is_err() {
            assert!(
                inner::score_results(&json!({"query": "test", "results": "not an array"})).is_err()
            );
        }

        #[test]
        fn score_results_empty_results_array_is_ok() {
            let result = inner::score_results(&json!({"query": "test", "results": []}));
            assert!(result.is_ok());
            assert_eq!(result.unwrap().as_array().unwrap().len(), 0);
        }

        #[test]
        fn score_results_result_without_date_is_ok() {
            // date is optional — missing date defaults to ""
            let result = inner::score_results(&json!({
                "query": "test",
                "results": [{"url": "/a", "title": "A", "excerpt": "content"}]
            }));
            assert!(result.is_ok());
        }

        #[test]
        fn score_results_empty_query_is_ok() {
            // empty query → terms extraction returns empty → results returned as-is
            let result = inner::score_results(&json!({
                "query": "",
                "results": [{"url": "/a", "title": "A", "excerpt": "a", "date": "2026-01-01"}]
            }));
            assert!(result.is_ok());
        }

        // ---- merge_results ----

        #[test]
        fn merge_results_array_input_is_err() {
            assert!(inner::merge_results(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn merge_results_missing_sets_is_err() {
            assert!(inner::merge_results(&json!({"deduplicate_by": "url"})).is_err());
        }

        #[test]
        fn merge_results_sets_is_string_is_err() {
            assert!(inner::merge_results(&json!({"sets": "not an array"})).is_err());
        }

        #[test]
        fn merge_results_empty_sets_is_ok() {
            let result = inner::merge_results(&json!({"sets": []}));
            assert!(result.is_ok());
            assert_eq!(result.unwrap().as_array().unwrap().len(), 0);
        }

        // ---- match_priority_pages ----

        #[test]
        fn match_priority_pages_array_input_is_err() {
            assert!(inner::match_priority_pages(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn match_priority_pages_missing_query_is_err() {
            assert!(inner::match_priority_pages(&json!({"priority_pages": []})).is_err());
        }

        #[test]
        fn match_priority_pages_missing_priority_pages_is_err() {
            assert!(inner::match_priority_pages(&json!({"query": "test"})).is_err());
        }

        #[test]
        fn match_priority_pages_empty_pages_is_ok() {
            let result =
                inner::match_priority_pages(&json!({"query": "test", "priority_pages": []}));
            assert!(result.is_ok());
            assert_eq!(result.unwrap().as_array().unwrap().len(), 0);
        }

        // ---- batch_score_results ----

        #[test]
        fn batch_score_results_array_input_is_err() {
            assert!(inner::batch_score_results(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn batch_score_results_missing_queries_is_err() {
            assert!(inner::batch_score_results(&json!({"default_config": {}})).is_err());
        }

        #[test]
        fn batch_score_results_query_entry_missing_results_is_err() {
            assert!(inner::batch_score_results(&json!({
                "queries": [{"query": "test"}]
            }))
            .is_err());
        }

        #[test]
        fn batch_score_results_empty_queries_is_ok() {
            let result = inner::batch_score_results(&json!({"queries": []}));
            assert!(result.is_ok());
            assert_eq!(result.unwrap().as_array().unwrap().len(), 0);
        }

        // ---- extract_context ----

        #[test]
        fn extract_context_array_input_is_err() {
            assert!(inner::extract_context(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn extract_context_missing_content_is_err() {
            assert!(inner::extract_context(&json!({"query": "test"})).is_err());
        }

        #[test]
        fn extract_context_missing_query_is_err() {
            assert!(inner::extract_context(&json!({"content": "hello world"})).is_err());
        }

        #[test]
        fn extract_context_empty_content_is_ok() {
            let result = inner::extract_context(&json!({"content": "", "query": "test"}));
            assert!(result.is_ok());
        }

        // ---- batch_extract_context ----

        #[test]
        fn batch_extract_context_array_input_is_err() {
            assert!(inner::batch_extract_context(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn batch_extract_context_missing_query_is_err() {
            assert!(inner::batch_extract_context(&json!({"items": []})).is_err());
        }

        #[test]
        fn batch_extract_context_missing_items_is_err() {
            assert!(inner::batch_extract_context(&json!({"query": "test"})).is_err());
        }

        #[test]
        fn batch_extract_context_empty_items_is_ok() {
            let result = inner::batch_extract_context(&json!({"query": "test", "items": []}));
            assert!(result.is_ok());
            assert_eq!(result.unwrap().as_array().unwrap().len(), 0);
        }

        // ---- sanitize_query ----

        #[test]
        fn sanitize_query_array_input_is_err() {
            assert!(inner::sanitize_query(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn sanitize_query_missing_query_is_err() {
            assert!(inner::sanitize_query(&json!({})).is_err());
        }

        #[test]
        fn sanitize_query_empty_query_is_ok() {
            let result = inner::sanitize_query(&json!({"query": ""}));
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "");
        }

        // ---- truncate_conversation ----

        #[test]
        fn truncate_conversation_array_input_is_err() {
            assert!(inner::truncate_conversation(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn truncate_conversation_missing_messages_is_err() {
            assert!(inner::truncate_conversation(&json!({})).is_err());
        }

        #[test]
        fn truncate_conversation_messages_is_string_is_err() {
            assert!(inner::truncate_conversation(&json!({"messages": "not an array"})).is_err());
        }

        #[test]
        fn truncate_conversation_empty_messages_is_ok() {
            let result = inner::truncate_conversation(&json!({"messages": []}));
            assert!(result.is_ok());
            assert_eq!(result.unwrap().as_array().unwrap().len(), 0);
        }

        // ---- resolve_prompt ----

        #[test]
        fn resolve_prompt_array_input_is_err() {
            assert!(inner::resolve_prompt(&json!([1, 2, 3])).is_err());
        }

        #[test]
        fn resolve_prompt_missing_prompt_name_is_err() {
            assert!(inner::resolve_prompt(&json!({"site_name": "Test"})).is_err());
        }

        #[test]
        fn resolve_prompt_unknown_prompt_name_is_err() {
            assert!(inner::resolve_prompt(&json!({"prompt_name": "nonexistent_xyz"})).is_err());
        }

        // ---- parse_expansion ----

        #[test]
        fn parse_expansion_complete_garbage_returns_empty() {
            // Garbage that isn't JSON and doesn't contain useful terms
            let terms = inner::parse_expansion("!!@#$%^&*()");
            // Must not panic; may return empty or filtered terms
            let _ = terms; // just verifying no panic
        }

        #[test]
        fn parse_expansion_partial_json_does_not_panic() {
            // Unclosed bracket — serde will fail to parse; falls back to expansion::parse_expansion
            let terms = inner::parse_expansion(r#"["term1", "term2"#);
            let _ = terms;
        }

        #[test]
        fn parse_expansion_object_without_text_field_does_not_panic() {
            // Valid JSON object but no "text" field → falls back to expansion::parse_expansion on the whole string
            let terms = inner::parse_expansion(r#"{"key": "value"}"#);
            let _ = terms;
        }

        #[test]
        fn parse_expansion_empty_string_returns_empty() {
            let terms = inner::parse_expansion("");
            assert!(terms.is_empty());
        }
    }
}
