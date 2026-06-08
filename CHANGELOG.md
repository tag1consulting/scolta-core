# Changelog

All notable changes to scolta-core will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/). Major versions are synchronized across all Scolta packages; minor and patch versions are released independently per package.

## [Unreleased]

### Added
- **No-fabrication guard for unrecognized named entities in the default `expand_query` prompt (rule 15).** A behavioral regression run of the merged decomposition rules (13/14) found the existing no-fabrication clause too narrow: rule 13 forbids inventing *members* to fill a category list, but nothing stopped the model from manufacturing authoritative-sounding domain detail for a *named entity it does not recognize*. Observed across demos, a fictional medical condition expanded to confident clinical terminology and a made-up product to confident attributes, while a fictional planet was handled correctly — inconsistent, and in a medical/legal/safety context actively harmful. New rule 15 (UNRECOGNIZED OR UNVERIFIABLE NAMED ENTITIES) generalizes the guard: when a query names a specific entity the model does not recognize as real and well-known, it must not manufacture members, terminology, treatments, or attributes for it, and must expand only with generic, neutral phrasings of the surrounding topic ("treatment for Glorptosis" → "medical treatment" / "therapy options" / "symptom management", not invented pathology). The rule is a guard, not a decomposition rule, so the existing 2-4/up-to-6 cap line is unchanged, and it does not affect cases where rule 13 already works (those name *known* categories). This text is byte-identical to the line added to scolta-php's `DefaultPrompts` `'expand_query'` template and scolta-python's `prompts.py` copy; the compiled WASM must be rebuilt downstream so the client-side AI path picks up the new text. Covered by `test_expand_query_forbids_fabricating_unverified_entities`. Additive: queries that name a recognized entity expand exactly as before.

## [1.0.1] - 2026-06-05

### Added
- **Regression snapshot test pinning the `summarize` CORPUS AWARENESS prompt bullet.** A new test (`test_summarize_corpus_awareness_matches_canonical_snapshot`) loads `tests/fixtures/corpus_awareness_bullet.txt` via `include_str!` and asserts the `SUMMARIZE` constant contains that exact bullet byte-for-byte, guarding against silent drift (follow-up to the [#33](https://github.com/tag1consulting/scolta-core/issues/33) corpus-statistic fix). The fixture is kept hand-identical to the matching bullet in scolta-php's `DefaultPrompts` `'summarize'` template. Test-only; no runtime or WASM change.
- **Category-member and context decomposition rules in the default `expand_query` prompt ([tag1consulting/scolta-core#36](https://github.com/tag1consulting/scolta-core/issues/36)).** Two new rules instruct the model to decompose groupings into concrete terms instead of restating them as abstract synonyms: rule 13 (CATEGORY → MEMBERS) expands a category/family/region into its well-known members ("version control systems" → Git/Mercurial/Subversion; "Southeast Asian food" → Thai/Vietnamese/Indonesian), and rule 14 (CONTEXT / USE-CASE → CONCRETE ITEMS) expands a context/occasion into the item types that serve it ("home office setup" → standing desk/ergonomic chair/monitor arm). Both lead with non-food examples so the behavior generalizes across domains, and rule 13 explicitly forbids fabricating members for categories the model does not know — unfamiliar categories fall back to normal alternate phrasings. The 2-4 term cap is reconciled to allow up to 6 concrete members when decomposing, and rule 7 is narrowed to taxonomy/filter-label matching so it no longer contradicts rule 13. Additive: queries that are not categories or contexts expand exactly as before. This text is byte-identical to the line added to scolta-php's `DefaultPrompts` `'expand_query'` template; the compiled WASM must be rebuilt so the client-side AI path picks up the new text.

### Fixed
- **Added an explicit output-length budget to the default `summarize` prompt ([tag1consulting/scolta-php#168](https://github.com/tag1consulting/scolta-php/issues/168)).** The prompt capped input excerpts but never bounded output length, so a multi-item summary could pad with ad-hoc sub-category headers and run long enough to be cut off at the `max_tokens` ceiling mid-sentence. The `SUMMARIZE` FORMAT RULES now state: keep the summary under ~150 words, a single flat bulleted list, no section or sub-category headers. The budget is expressed in words + structure (models approximate length and are unreliable at literal character counts). This line is byte-identical to the one added to scolta-php's `DefaultPrompts` `'summarize'` template; the compiled WASM must be rebuilt so the client-side AI path picks up the new text.
- **Removed the Wikipedia-specific corpus statistic from the default `summarize` prompt.** The `CORPUS AWARENESS` rule shipped a hard-coded "~6,900 Featured Articles" example that described only the Wikipedia demo, reached every site using the default prompt, and taught the model to fabricate corpus counts (observed confabulating stats on unrelated demos). The example is now count-free and frames gaps via the site description's scope, and the rule explicitly forbids inventing statistics (counts, totals, sizes). The compiled WASM must be rebuilt so the client-side AI path picks up the new text. ([tag1consulting/scolta-core#33](https://github.com/tag1consulting/scolta-core/issues/33))

### Removed
- **Reverted query-word-importance scoring weight (#31).** Removed the `incidental_match_weight` config and the per-query-word importance weighting from `score_results`/`batch_score_results`. Validation showed the weighting was inert — it changed result ordering on zero real queries — so the scorer returns to counting every matched query term equally, as it did before #31.

### Changed
- **Aligned scoring engine defaults with scolta-php's sweep-validated values.** `ScoringConfig` defaults now set `title_match_boost` 1.0 → 2.0 (better top-1 precision) and `recency_boost_max` 0.5 → 0.25 (recency adds noise on non-time-sensitive content), matching the defaults a full-matrix scoring sweep already shipped in `scolta-php`. No adapter or demo behavior change: adapters export every scoring field into `window.scolta.scoring`, so the WASM scorer always receives explicit config and never falls back to these `Default` values — the alignment only affects standalone `scolta-core` crate/WASM users who omit a field, and keeps the docs consistent with the code. No function signature changed, so `WASM_INTERFACE_VERSION` is unchanged. Updated `API.md`, `IMPLEMENTATION.md`, `README.md`, the `src/scoring.rs` doc-comment defaults table, and the `from_json` JSON-parse fallbacks accordingly. Added a doc-sync guard test (`api_md_documents_actual_scoring_defaults`) that parses the `ScoringConfig` defaults table in `API.md` and asserts every documented default equals `ScoringConfig::default()`, failing loudly on an unparseable row so the docs can't silently drift from the code again. WASM rebuilt.
- **Added release tarball member-list assertion to CI.** New `tarball-allowlist` job builds the WASM tarball and asserts it contains exactly the 4 expected files (`scolta_core_bg.wasm`, `scolta_core.js`, `scolta_core.d.ts`, `scolta_core_bg.wasm.d.ts`). Catches accidental inclusions on every PR.

### Removed
- **Deleted `.gitattributes`** — inert for this repo (cargo ignores it; nothing runs `git archive`). `Cargo.toml` `exclude` already handles crates.io packaging.

### Documentation
- **Clarified independent versioning model.** VERSIONING.md and CLAUDE.md now state that minor and patch versions are released independently per package, with adapters pinning scolta-php via `composer.lock` within their `^1.x` constraint. Refreshed stale `1.0.0-rc4` example version strings. Added Drupal's `scolta.info.yml` and WordPress's `readme.txt Stable Tag` to the version-location table. Removed stale `scolta-python` reference from CLAUDE.md.

## [1.0.0] - 2026-05-27

### Changed
- **Pre-1.0 cleanup.** Promoted all experimental functions to stable for 1.0 GA: `match_priority_pages`, `extract_context`, `batch_extract_context`, `sanitize_query`, `truncate_conversation`, `batch_score_results`. Removed stale references to deleted functions (`to_js_scoring_config`, `clean_html`, `build_pagefind_html`, `debug_call`) from documentation, tests, and doc comments. Rewrote API.md and IMPLEMENTATION.md from scratch. Fixed CHANGELOG section headers to use keepachangelog standard names. Added keepachangelog link references. CI now runs integration tests (`cargo test` instead of `cargo test --lib`). Added missing integration tests for `batch_score_results`, `resolve_prompt`, `get_prompt`, and `version`. Added `.gitattributes` and Cargo.toml packaging metadata. Fixed `repository` URL in Cargo.toml.
- **Prompt templates synced with PHP canonical.** `expand_query`: JSON object response format (was JSON array), 12 rules (was 11) — added site-topic disambiguation (rule 9) and constraint queries (rule 12), plus 2 new examples. `summarize`: added CURATION RULES (filter, dig, scan, focus, variety, category, breadth), LANGUAGE RULES, METADATA RULES, CORPUS AWARENESS in grounding check; updated tone to "Direct, expert, helpful." `follow_up`: added NUMBERED RESULT REFERENCES, CURATION RULES, CORPUS AWARENESS in grounding check; updated tone to match summarize.

## [1.0.0-rc4] - 2026-05-18

### Added
- `sort_override` field in `score_results` input: `{ "field": "...", "direction": "asc"|"desc" }`. When present, results are filtered to those carrying the named metadata field and sorted by its value (numeric strings compared numerically, others lexicographically) with relevance score as a tiebreaker. Omitting `sort_override` preserves the current relevance-ranked behavior. ([#21](https://github.com/tag1consulting/scolta-core/issues/21))

## [1.0.0-rc3] - 2026-05-13

### Changed
- Version synchronized with coordinated 1.0.0-rc3 release across all Scolta packages.

## [1.0.0-rc2] - 2026-05-12

### Changed
- Version synchronized with coordinated 1.0.0-rc2 release across all Scolta packages.

## [1.0.0-rc1] - 2026-05-11

First stable release — all features from 0.3.x promoted to 1.0 API surface.

## [0.3.10] - 2026-05-05

### Changed
- Version synchronized with coordinated 0.3.10 release across all Scolta packages. No Rust code changes since 0.3.8; binary is unchanged.

## [0.3.9] - 2026-05-02

### Changed
- Version synchronized with scolta-php 0.3.9 (scoring preset UI for adapter packages). No Rust code changes since 0.3.7; binary is unchanged.

## [0.3.8] - 2026-05-01

### Changed
- Version synchronized with scolta-php 0.3.8. No Rust code changes since 0.3.7; binary is unchanged.

## [0.3.7] - 2026-04-30

### Changed
- Documentation: clearer project positioning, cross-platform search consistency messaging.

### Changed
- Version synchronized with coordinated 0.3.7 release across all Scolta packages. No Rust code changes since 0.3.6.

## [0.3.6] - 2026-04-29

### Changed
- Version synchronized with coordinated 0.3.6 release across all Scolta packages. No Rust code changes since 0.3.4.

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

[Unreleased]: https://github.com/tag1consulting/scolta-core/compare/v1.0.0-rc4...HEAD
[1.0.0-rc4]: https://github.com/tag1consulting/scolta-core/compare/v1.0.0-rc3...v1.0.0-rc4
[1.0.0-rc3]: https://github.com/tag1consulting/scolta-core/compare/v1.0.0-rc2...v1.0.0-rc3
[1.0.0-rc2]: https://github.com/tag1consulting/scolta-core/compare/v1.0.0-rc1...v1.0.0-rc2
[1.0.0-rc1]: https://github.com/tag1consulting/scolta-core/compare/v0.3.10...v1.0.0-rc1
[0.3.10]: https://github.com/tag1consulting/scolta-core/compare/v0.3.9...v0.3.10
[0.3.9]: https://github.com/tag1consulting/scolta-core/compare/v0.3.8...v0.3.9
[0.3.8]: https://github.com/tag1consulting/scolta-core/compare/v0.3.7...v0.3.8
[0.3.7]: https://github.com/tag1consulting/scolta-core/compare/v0.3.6...v0.3.7
[0.3.6]: https://github.com/tag1consulting/scolta-core/compare/v0.3.5...v0.3.6
[0.3.5]: https://github.com/tag1consulting/scolta-core/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/tag1consulting/scolta-core/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/tag1consulting/scolta-core/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/tag1consulting/scolta-core/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/tag1consulting/scolta-core/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/tag1consulting/scolta-core/compare/v0.2.4...v0.3.0
[0.2.4]: https://github.com/tag1consulting/scolta-core/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/tag1consulting/scolta-core/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/tag1consulting/scolta-core/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/tag1consulting/scolta-core/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/tag1consulting/scolta-core/releases/tag/v0.2.0
