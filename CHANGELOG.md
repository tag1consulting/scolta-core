# Changelog

All notable changes to scolta-core will be documented in this file.

This project uses [Semantic Versioning](https://semver.org/). Major versions are synchronized across all Scolta packages.

## [Unreleased]

_No changes yet._

## [0.3.5] - 2026-04-28

_No new entries — release synchronized with scolta-php/scolta-wp/scolta-drupal/scolta-laravel._

## [0.3.4] - 2026-04-27

### Fixed
- **Content all-terms multiplier is now multiplicative.** `content_match_score_with_terms` now applies `content_all_terms_multiplier` via `*=` (consistent with title scoring) rather than assignment. Default adjusted from `0.48` to `1.2` so that `content_match_boost (0.4) × 1.2 = 0.48` — identical output for default configurations, but users can now tune `content_match_boost` independently and have all-terms scoring scale proportionally.
- **`SearchResult.date` is now optional in JSON deserialization.** The `date` field lacked `#[serde(default)]`, so any caller omitting `date` from a result object received a deserialization error instead of scoring the result with a zero recency boost. Fixed by adding `#[serde(default)]`, making the field behave identically to `score`, `content_type`, and `site_name`. Revealed by the new malformed-input test suite.
- **Expanded query results now receive title boost from primary query terms.** `score_results` accepts an optional `primary_query` field; when present, the title boost for each result is the maximum of the expanded-query title boost and the primary-query title boost. This fixes ranking bias that favored literal title matches over semantically correct results found via query expansion.

### Added
- **`merge_results` optional `debug` field.** When `"debug": true` is included in the input,
  `merge_results` returns `{"results": [...], "debug": {...}}` instead of a plain array. The
  `debug` object contains per-set `input_count` and `weight`, plus `total_before_dedup`,
  `total_after_dedup`, and `excluded_count`. Omitting `debug` or setting it to `false` preserves
  the existing plain-array response (fully backward-compatible).
- **Prompt sync: all three prompt improvements from scolta-php.** `prompts.rs` updated to match
  `DefaultPrompts.php`: rule 4 strengthened + rule 11 (audience qualifiers) in `expand_query`;
  `GROUNDING CHECK` section added to both `summarize` and `follow_up`; per-excerpt scanning and
  minimum bullet count instruction added to `summarize` FORMAT RULES.
- **Context extraction UTF-8 safety tests.** New `mod utf8_safety` in `context::tests`: 6 tests covering multi-byte char handling in snippet extraction and sentence truncation — large 2-byte-char content with a keyword, `caffè` keyword where snippet radius lands on an odd byte offset inside `è`, flag emoji (🇮🇹, 8 bytes) adjacent to the keyword, `truncate_at_sentence` finding a period before a 2-byte char, CJK content with no ASCII sentence terminators, and range merging across 200 bytes of `è` filler.
- **Malformed input tests.** New `mod malformed_input` in `lib::tests`: 42 tests verifying that every `inner::` entry point returns `Err` (never panics) when given a JSON array instead of an object, a missing required field, or a wrong field type. Also verifies that valid edge-case inputs (empty results arrays, missing optional fields, empty queries) succeed. Covers: `score_results`, `merge_results`, `match_priority_pages`, `batch_score_results`, `extract_context`, `batch_extract_context`, `sanitize_query`, `truncate_conversation`, `resolve_prompt`, and `parse_expansion`.
- **Phrase-proximity value tests.** New `mod phrase_proximity_values` in `scoring::tests`: 11 tests verifying exact multiplier outputs for `phrase_proximity_multiplier`. Covers: empty locations (1.0), fewer locations than terms (1.0), single-term no-op (1.0), duplicate positions → adjacent (2.5), span = n−1 boundary for 2-term and 3-term queries (adjacent), span = n (near, not adjacent), span = `phrase_near_window` inclusive boundary (near), span = window+1 (no bonus), unsorted input sorting, and u32::MAX positions with no overflow.
- **Recency strategy value tests.** New `mod recency_values` in `scoring::tests`: 24 tests verifying exact formula outputs for all five recency strategies (`exponential`, `linear`, `step`, `custom`, `none`). Covers: day-0 boost, half-life decay, two-half-life decay, penalty threshold, penalty cap, custom half_life/boost_max, zero half_life, future dates, interpolation between curve points, single-point and empty curves, and the 1/6/20-year penalty boundary cases.
- **Ranking sensitivity tests.** New `mod ranking_sensitivity` in `scoring::tests`: 8 tests verifying that changing a scoring config parameter (`title_match_boost`, `recency_boost_max`, `recency_strategy`, `content_match_boost`, `content_all_terms_multiplier`, `phrase_adjacent_multiplier`, `recency_curve`, `title_all_terms_multiplier`) flips the ranking order as expected.

## [0.3.3] - 2026-04-26

### Changed
- **Config value clamping.** `ScoringConfig::clamp_and_validate()` added; `from_json_validated()` now uses it instead of the warn-only `validate()`. Out-of-range values (e.g. `recency_boost_max: 100.0`) are clamped to their documented boundaries — preventing misconfiguration from silently breaking search ranking. A warning is still emitted for each clamped field. Fields affected: `recency_boost_max` (0.0–2.0), `recency_half_life_days` (1–3650), `recency_max_penalty` (0.0–1.0), `results_per_page` (1–100), `max_pagefind_results` (1–500). String-enum and structural fields (`recency_strategy`, `recency_curve` sort order) remain warn-only. WASM rebuilt.

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
