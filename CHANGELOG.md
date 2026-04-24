# Changelog

All notable changes to scolta-core will be documented in this file.

This project uses [Semantic Versioning](https://semver.org/). Major versions are synchronized across all Scolta packages.

## [0.3.2] - 2026-04-24

### Changed
- Version aligned with coordinated 0.3.2 release across Scolta packages. No Rust code changes since 0.3.1.

## [0.3.1] - 2026-04-23

### Fixed
- **Release workflow**: Trigger now accepts both `v0.x.x` and bare `0.x.x` tag formats. The 0.3.0 tag lacked the `v` prefix, so the workflow never fired and no WASM assets were attached to the release.

## [0.3.0] - 2026-04-23

### Added
- **`{DYNAMIC_ANCHORS}` placeholder in `resolve_prompt`**: Callers can now pass `dynamic_anchors: string[]` in the `resolve_prompt` JSON input. When the `summarize` or `follow_up` template is used, anchors are joined with newlines and injected before the FORMAT RULES block. When anchors are absent or the template has no placeholder, the call is a no-op — fully backward-compatible with all existing callers.
- **`resolve_template` `anchors` parameter**: `resolve_template` gains `anchors: Option<&[String]>`. Silent no-op when the template has no `{DYNAMIC_ANCHORS}` placeholder; erases the placeholder to an empty string when anchors are `None` or empty.

## [0.2.4] - 2026-04-21

### Fixed
- **Phrase-match ranking regression:** exact-phrase body matches (e.g. "hello world" appearing together) previously ranked below documents with a single query term in the title (e.g. "Hello Integrations"). Root cause: `score_results()` applied per-term title/content boosts with no phrase-proximity signal — Pagefind tokenizes queries into OR'd terms, and the scorer had no way to know "hello" and "world" were adjacent in a document vs. scattered. Fixed by consuming Pagefind's word-position `locations[]` data through a new `QueryInfo` + `phrase_proximity_multiplier` path; see `### Added` below.

### Added
- **Phrase-proximity scoring**: `score_results()` now applies a phrase-proximity multiplier to the content boost when Pagefind word positions (`locations`) are available. Adjacent phrase (span ≤ terms−1): ×2.5 multiplier; near phrase (span ≤ `phrase_near_window`): ×1.5. Fixes the root cause of exact-phrase matches ranking below scattered single-term title hits.
- **`extract_query()` / `extract_query_with_custom()`** in `common.rs`: Returns `QueryInfo { terms, is_phrase, forced_phrase }`. Detects double-quoted queries (`"hello world"`) and sets `forced_phrase = true` for downstream phrase scoring.
- **`score_result_with_query_info()`**: New internal scoring entry point that accepts `QueryInfo` and applies phrase-proximity multiplier when `is_phrase` is true and `locations` data is present.
- **`phrase_proximity_multiplier()`**: Internal function that converts Pagefind `locations` positions into an adjacent/near/scattered multiplier via a sliding-window minimum-span algorithm.
- **`ScoringConfig` phrase fields**: `phrase_adjacent_multiplier` (default 2.5), `phrase_near_multiplier` (default 1.5), `phrase_near_window` (default 5), `phrase_window` (default 15).
- **`SearchResult.locations`**: Optional `Vec<u32>` field (serde default = None) receiving Pagefind word-position data. Results without positions fall back to existing term-only scoring.
- **Five regression tests** in `scoring.rs`: adjacent phrase > title hit; near phrase > scattered; single-term query unchanged; no-locations fallback; forced-phrase quoted query.

## [0.2.3] - 2026-04-17

### Added
- `batch_extract_context()` — query-relevant snippet extraction for LLM summarization (intro paragraph + keyword-anchored snippets + sentence boundary truncation)
- `sanitize_query()` — PII redaction (email, phone, SSN, credit card, IP) with configurable patterns
- `match_priority_pages()` — URL pattern + keyword matching with configurable boost multipliers
- `truncate_conversation()` — conversation history trimming preserving system messages

### Changed
- `merge_results()` — new N-set format `{ sets: [{ results, weight }] }` with `deduplicate_by` and `normalize_urls` options (replaces deprecated `{ original, expanded }` format)
- `parse_expansion()` — added `generic_terms` filtering and `existing_terms` merging
- `score_results()` — added `priority_pages` config and per-result `source_weight`

### Removed
- `to_js_scoring_config` WASM export and `inner::` function removed; callers should use JSON config fields directly

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
