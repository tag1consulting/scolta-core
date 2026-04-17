# Changelog

All notable changes to scolta-core will be documented in this file.

This project uses [Semantic Versioning](https://semver.org/). Major versions are synchronized across all Scolta packages.

## [0.2.3] - Unreleased

### Added

- **`match_priority_pages`:** New WASM export and `inner::` function. Takes `{ query, priority_pages }` and returns pages whose `url_pattern` or `keywords` match the query. Enables pre-call priority page injection into `score_results`.
- **Priority page boosting in `score_results`:** `ScoringConfig` gains `priority_pages` (array of `{ url_pattern, keywords, boost, custom_excerpt, page_id }` objects). Matching pages receive a `priority_boost` addend to their score; `custom_excerpt` overrides the excerpt in the returned result.
- **Source weighting:** `SearchResult` gains `source_weight: Option<f64>`. When set, the base score is multiplied by this factor before all other boosts are applied.
- **N-set `merge_results`:** Rewrote `merge_results` to accept `{ sets: [{ results, weight }], deduplicate_by, case_sensitive, exclude_urls, normalize_urls }`. Applies per-set weight multipliers, then deduplicates (by URL or title), then excludes listed URL patterns. Old `{ original, expanded }` format returns an error.
- **`parse_expansion` generic-term filtering:** `parse_expansion` object form now accepts `generic_terms` (site-specific words to filter) and `existing_terms` (terms to merge into the output). Acronym exception: ≤3-char terms pass regardless of generic status. Proper noun exception: terms with an uppercase letter pass.
- **`extract_context` / `batch_extract_context`:** New WASM exports. Extract the most relevant portion of article content for LLM context using intro + keyword-anchored snippets + sentence-boundary truncation.
- **`sanitize_query`:** New WASM export. Redacts PII (email, phone, SSN, credit card, IPv4) from queries before analytics logging. Supports custom regex patterns via `custom_patterns`.
- **`truncate_conversation`:** New WASM export. Trims a conversation message array to a character limit, always preserving the first N messages and removing oldest pairs.
- **`WASM_INTERFACE_VERSION` bumped to 4** — reflects new exports and removal of `to_js_scoring_config`.
- `regex = "1"` added as a dependency (for `sanitize_query` custom patterns).

### Removed

- **`to_js_scoring_config`:** Removed WASM export and `inner::` function. Callers should use the JSON config fields directly.
- `UnknownFunction` and `ConfigWarning` error variants removed from `ScoltaError`.

## [0.2.2] - 2026-04-16

### Added

- **Language-aware stop words:** `ScoringConfig` now has `language` (ISO 639-1, default `"en"`) and `custom_stop_words` fields. Stop word filtering in `score_results`, term extraction, and expansion parsing all respect the configured language. Static word lists cover 30 languages: ar, ca, da, de, el, en, es, et, eu, fi, fr, ga, hi, hu, hy, id, it, lt, ne, nl, no, pl, pt, ro, ru, sr, sv, ta, tr, yi. CJK and unknown language codes return empty lists (no filtering).
- **`parse_expansion_with_language()`:** New `inner::` function and `browser.rs` object-form dispatch. `parse_expansion` now also accepts `{ "text": "...", "language": "fr" }` as input for language-aware expansion filtering.
- **Pluggable recency functions:** `ScoringConfig` gains `recency_strategy` (default `"exponential"`) and `recency_curve`. Supported strategies: `"exponential"` (unchanged default), `"linear"`, `"step"`, `"none"`, `"custom"` (piecewise-linear control points). Unknown strategies fall back to `"exponential"`. Config validation warns on unknown strategy, empty curve with `"custom"`, and unsorted curve points.
- **Batch scoring API (`batch_score_results`):** New `#[wasm_bindgen]` export and `inner::batch_score_results`. Accepts `{ "queries": [{ "query", "results", "config"? }], "default_config"? }` and returns an array of scored result arrays. Per-query config overrides the default config.
- **`WASM_INTERFACE_VERSION` bumped to 3** — reflects new `batch_score_results` export.
- New `src/stop_words.rs` module (`pub mod stop_words`) with `get_stop_words(language)`.

### Changed

- `common::is_stop_word`, `is_valid_term`, `extract_terms` now require a `language: &str` parameter. New `_with_custom` variants accept an additional `custom: &[String]` stop word list.
- `RECENCY_STRATEGY`, `RECENCY_CURVE`, `LANGUAGE`, `CUSTOM_STOP_WORDS` added to the `to_js_scoring_config` / `TO_JS_SCORING_CONFIG` output for JavaScript frontend integration.

## [0.2.1] - 2026-04-15

### Fixed

- **Performance:** `score_results` now calls `extract_terms()` once per query instead of once per result, eliminating redundant work on large result sets
- **Correctness:** Replace approximate `date_to_days()` with exact Howard Hinnant civil-day algorithm — eliminates cumulative off-by-days error on dates far from epoch

### Changed

- `wasm-opt` disabled in release profile (`wasm-opt = false`) — bundled wasm-opt binary lacks feature flags required by the output WASM; size optimization is still applied via `opt-level = "s"` and LTO

### Documentation

- Rewrote `API.md` from scratch to describe the wasm-bindgen architecture (8 browser exports, correct build instructions, actual data schemas); removed all Extism/PDK/wasm32-wasip1 references
- Updated `IMPLEMENTATION.md`, `TESTING.md`, `VERSIONING.md`, `CLAUDE.md` to replace Extism references with wasm-bindgen equivalents

## [0.2.0] - 2026-04-13

### Changed

- **BREAKING:** Removed server-side Extism/WASI target entirely — scolta-core is now browser-only WASM
- Removed `clean_html` and `build_pagefind_html` (ported to pure PHP in scolta-php)
- Removed `debug_call` (server-side profiling tool)
- Removed feature flags (`extism`/`browser`) — single target, no flags needed
- Removed `extism-pdk` and `regex` dependencies
- WASM interface version bumped to 2

### Added

- 8 wasm-bindgen exports: `score_results`, `merge_results`, `parse_expansion`, `resolve_prompt`, `get_prompt`, `to_js_scoring_config`, `version`, `describe`
- `to_js_scoring_config` passes through `AI_LANGUAGES` array for frontend multilingual support
- Search scoring algorithm with recency decay (exponential half-life), title/content match boosting, and expanded-term weight decay
- Result merging with Jaccard deduplication and configurable primary/expanded weight split
- HTML cleaner that strips page chrome and extracts main content
- Pagefind-compatible HTML builder with data attributes
- Prompt template system with `expand_query`, `summarize`, and `follow_up` templates and variable resolution
- LLM expansion response parser supporting JSON, markdown, and plain-text fallback formats
- Self-documenting `describe()` function catalog for runtime discovery
- `debug_call` profiling wrapper with timing and size metrics
- OnceLock-cached regex compilation for HTML processing
- Typed error enum with function-name attribution
- Shared stop words and term extraction utilities
