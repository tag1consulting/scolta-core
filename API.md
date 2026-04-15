# Scolta Core API Reference

Version: 0.2.2-dev · Target: wasm32-unknown-unknown · Framework: wasm-bindgen

## 1. Overview

`scolta-core` is the Rust/WebAssembly module that powers Scolta's client-side search experience. It provides search result scoring, prompt template management, and LLM expansion response parsing. By running in the browser via wasm-bindgen, it guarantees identical scoring behavior across all platform adapters (PHP, WordPress, Drupal, Laravel) without server round-trips.

**Architecture:** `browser.rs` contains 9 `#[wasm_bindgen]` exports that form the public API. Each export is a thin serialization wrapper delegating to a corresponding function in the `inner::` module (`lib.rs`). The `inner::` functions are plain Rust — testable with `cargo test` without a WASM runtime — and are the sole implementation of all business logic. Modules `scoring`, `config`, `prompts`, `expansion`, `error`, and `common` contain the algorithmic details.

---

## 2. Browser WASM Exports

All 9 exports take and return JSON strings (or plain strings for `version` and `get_prompt`). On error they return a `JsError` that becomes a JavaScript exception.

---

### `score_results(input: &str) -> Result<String, JsError>`

Score and re-rank search results by relevance to a query.

**Input JSON:**
```json
{
  "query": "search terms",
  "results": [
    {
      "url": "https://example.com/page",
      "title": "Page Title",
      "excerpt": "Page excerpt text",
      "date": "2025-06-01",
      "score": 0.85
    }
  ],
  "config": {}
}
```

- `query` — required string. The user's search query.
- `results` — required array of `SearchResult` objects (see Section 4).
- `config` — optional `ScoringConfig` object (see Section 4). Missing fields use defaults.

**Output JSON:** Array of `SearchResult` objects sorted by computed score descending. Each result gains a `score` field with the computed relevance value.

```json
[
  {
    "url": "https://example.com/page",
    "title": "Page Title",
    "excerpt": "Page excerpt text",
    "date": "2025-06-01",
    "score": 2.35
  }
]
```

**Scoring formula:**
```
final_score = base_score + title_boost + content_boost + recency_boost
```

Where `base_score` is the upstream engine score (e.g., from Pagefind) if present, otherwise 1.0.

---

### `merge_results(input: &str) -> Result<String, JsError>`

Merge original and expanded search results with URL-based deduplication. Used when AI query expansion produces a secondary result set that should be blended with the primary results.

**Input JSON:**
```json
{
  "original": [ /* array of SearchResult */ ],
  "expanded": [ /* array of SearchResult */ ],
  "config": { "expand_primary_weight": 0.7 }
}
```

- `original` — required. Results from the primary search.
- `expanded` — required. Results from the expanded (AI-augmented) search.
- `config` — optional. `expand_primary_weight` (default 0.7) controls how much weight original results receive vs. expanded. Expanded results get `1.0 - expand_primary_weight`.

**Output JSON:** Merged, deduplicated array sorted by combined score descending.

```json
[
  { "url": "https://example.com/a", "title": "A", "score": 1.8, "excerpt": "...", "date": "2025-01-01" },
  { "url": "https://example.com/b", "title": "B", "score": 0.9, "excerpt": "...", "date": "2024-06-01" }
]
```

---

### `parse_expansion(input: &str) -> Result<String, JsError>`

Parse an LLM expansion response into an array of search terms. Accepts two input forms:

**1. Bare string** — raw LLM response; language defaults to `"en"`.

Handles three text formats with graceful fallback:

- **JSON array** (preferred): `["term1", "term2", "term3"]`
- **Markdown-wrapped JSON**: ` ```json ["term1", "term2"] ``` `
- **Fallback**: newline- or comma-separated plain text

**2. JSON object** — specify a language for stop word filtering:
```json
{ "text": "[\"term1\", \"term2\"]", "language": "de" }
```

All results are filtered through the stop word list for the given language (see `ScoringConfig.language`). Single-character strings and pure numbers are removed.

**Output JSON:** Array of cleaned expansion terms.

```json
["cardiac surgery", "heart procedure", "surgical intervention"]
```

---

### `batch_score_results(input: &str) -> Result<String, JsError>` *(since 0.2.2)*

Score multiple queries against their respective result sets in a single WASM call. Reduces JS↔WASM round-trip overhead when re-scoring several result batches at once (e.g., after AI query expansion produces multiple term sets).

**Input JSON:**
```json
{
  "queries": [
    {
      "query": "search terms",
      "results": [ /* array of SearchResult */ ],
      "config": { "language": "en" }
    },
    {
      "query": "other query",
      "results": [ /* array of SearchResult */ ]
    }
  ],
  "default_config": { "recency_boost_max": 0.3 }
}
```

- `queries` — required array. Each entry must have `query` and `results`; `config` is optional.
- `default_config` — optional. Applied to every query entry. Per-query `config` takes precedence.

**Output JSON:** Array of scored result arrays, one per query, in input order.

```json
[
  [ { "url": "https://a.com", "score": 2.1, ... } ],
  [ { "url": "https://b.com", "score": 1.4, ... } ]
]
```

**Error:** `JsError` if `queries` is missing, any entry lacks `query` or `results`, or any `results` array cannot be parsed.

---

### `resolve_prompt(input: &str) -> Result<String, JsError>`

Resolve a named prompt template with site-specific variable substitution.

**Input JSON:**
```json
{
  "prompt_name": "expand_query",
  "site_name": "Acme Corp",
  "site_description": "the premier widget supplier"
}
```

- `prompt_name` — required. One of: `"expand_query"`, `"summarize"`, `"follow_up"`.
- `site_name` — optional. Substituted for `{SITE_NAME}` in the template.
- `site_description` — optional. Substituted for `{SITE_DESCRIPTION}` in the template.

**Output:** The resolved prompt string (plain text, not JSON).

**Error:** `JsError` if `prompt_name` is missing or not recognized.

---

### `get_prompt(name: &str) -> Result<String, JsError>`

Get a raw prompt template by name without variable substitution. Useful for displaying templates to administrators or pre-processing before substitution.

**Input:** Prompt name string — one of `"expand_query"`, `"summarize"`, `"follow_up"`.

**Output:** Raw template string containing `{SITE_NAME}` and `{SITE_DESCRIPTION}` placeholders.

**Error:** `JsError` if the name is not recognized.

---

### `to_js_scoring_config(input: &str) -> Result<String, JsError>`

Convert a scoring config object to the SCREAMING_SNAKE_CASE format expected by the JavaScript frontend (`window.scolta`). Also passes through AI feature flags that the browser needs but the scoring algorithm does not.

**Input JSON:** Any subset of `ScoringConfig` fields plus AI toggle flags:
```json
{
  "recency_boost_max": 0.8,
  "ai_expand_query": true,
  "ai_summarize": false,
  "ai_languages": ["en", "es"]
}
```

**Output JSON:** Config with SCREAMING_SNAKE_CASE keys:
```json
{
  "RECENCY_BOOST_MAX": 0.8,
  "RECENCY_HALF_LIFE_DAYS": 365,
  "RECENCY_PENALTY_AFTER_DAYS": 1825,
  "RECENCY_MAX_PENALTY": 0.3,
  "RECENCY_STRATEGY": "exponential",
  "RECENCY_CURVE": [],
  "TITLE_MATCH_BOOST": 1.0,
  "TITLE_ALL_TERMS_MULTIPLIER": 1.5,
  "CONTENT_MATCH_BOOST": 0.4,
  "CONTENT_ALL_TERMS_MULTIPLIER": 0.48,
  "EXCERPT_LENGTH": 300,
  "RESULTS_PER_PAGE": 10,
  "MAX_PAGEFIND_RESULTS": 50,
  "EXPAND_PRIMARY_WEIGHT": 0.7,
  "LANGUAGE": "en",
  "CUSTOM_STOP_WORDS": [],
  "AI_EXPAND_QUERY": true,
  "AI_SUMMARIZE": false,
  "AI_SUMMARY_TOP_N": 5,
  "AI_SUMMARY_MAX_CHARS": 2000,
  "AI_MAX_FOLLOWUPS": 3,
  "AI_LANGUAGES": ["en", "es"]
}
```

---

### `version() -> String`

Return the scolta-core crate version string (e.g., `"0.2.1-dev"`). No input.

---

### `describe() -> String`

Return a JSON manifest of all exported functions with metadata. Used by platform adapters to verify compatibility at runtime.

**Output JSON:**
```json
{
  "name": "scolta-core",
  "version": "0.2.2-dev",
  "wasm_interface_version": 3,
  "description": "Scolta browser WASM — client-side search scoring, prompt management, and query expansion",
  "functions": {
    "score_results": { "since": "0.1.0", "stability": "stable", "input_type": "json", "output_type": "json" },
    "merge_results": { "since": "0.1.0", "stability": "stable", "input_type": "json", "output_type": "json" },
    "parse_expansion": { "since": "0.1.0", "stability": "stable", "input_type": "string", "output_type": "json" },
    "batch_score_results": { "since": "0.2.2", "stability": "experimental", "input_type": "json", "output_type": "json" },
    "resolve_prompt": { "since": "0.1.0", "stability": "stable", "input_type": "json", "output_type": "string" },
    "get_prompt": { "since": "0.1.0", "stability": "stable", "input_type": "string", "output_type": "string" },
    "to_js_scoring_config": { "since": "0.1.0", "stability": "stable", "input_type": "json", "output_type": "json" },
    "version": { "since": "0.1.0", "stability": "stable", "input_type": "none", "output_type": "string" },
    "describe": { "since": "0.1.0", "stability": "stable", "input_type": "none", "output_type": "json" }
  }
}
```

`wasm_interface_version` tracks binary compatibility. Platform adapters check this value at load time and throw if it doesn't match the expected version.

---

## 3. Server-Only Functions (Not Exported to WASM)

Three functions exist in the Rust codebase but are **not** exposed as wasm-bindgen exports:

| Function | Reason |
|---|---|
| `clean_html` | Build-time HTML processing. Used server-side when generating pagefind-compatible HTML documents. Not needed in the browser. |
| `build_pagefind_html` | Build-time document generation. Produces HTML with `data-pagefind-*` attributes for the indexer. Not needed in the browser. |
| `debug_call` | Server-side profiling wrapper. Wraps any inner function with microsecond timing and input/output size tracking. Not part of the browser WASM surface. |

These are available as Rust library functions when using scolta-core as an `rlib` dependency via the `inner::` module in `lib.rs`.

---

## 4. Data Formats

### ScoringConfig

All fields are optional in JSON input; missing fields use the listed defaults.

| Field | Type | Default | Description |
|---|---|---|---|
| `recency_boost_max` | f64 | 0.5 | Maximum additive recency boost for recent content |
| `recency_half_life_days` | u32 | 365 | Decay half-life in days; also step boundary for `"step"` strategy |
| `recency_penalty_after_days` | u32 | 1825 | Days (~5 years) after which an old-content penalty begins |
| `recency_max_penalty` | f64 | 0.3 | Maximum additive penalty for very old content |
| `recency_strategy` | string | `"exponential"` | Decay strategy: `"exponential"`, `"linear"`, `"step"`, `"none"`, `"custom"` |
| `recency_curve` | `[[f64,f64]]` | `[]` | Control points `[days_old, boost]` for `"custom"` strategy; must be sorted ascending |
| `title_match_boost` | f64 | 1.0 | Boost when any query term appears in the title |
| `title_all_terms_multiplier` | f64 | 1.5 | Multiplier applied when ALL terms appear in title |
| `content_match_boost` | f64 | 0.4 | Boost when any query term appears in content |
| `content_all_terms_multiplier` | f64 | 0.48 | Multiplier applied when ALL terms appear in content |
| `expand_primary_weight` | f64 | 0.7 | Weight for primary results vs. expanded in merge |
| `excerpt_length` | u32 | 300 | Max excerpt length in characters |
| `results_per_page` | u32 | 10 | Results per page for pagination |
| `max_pagefind_results` | u32 | 50 | Max results from Pagefind to consider |
| `language` | string | `"en"` | ISO 639-1 code for stop word filtering (30 languages supported; CJK/unknown → no filtering) |
| `custom_stop_words` | string[] | `[]` | Additional stop words layered on top of the language list |

Valid ranges are checked by `ScoringConfig::validate()`. Values outside reasonable ranges produce `ConfigWarning` entries but do not prevent scoring.

**Recency strategy details:**

| Strategy | Behaviour |
|---|---|
| `"exponential"` | `MAX × exp(-age/HALF_LIFE × ln2)` — matches Tag1 reference (default) |
| `"linear"` | Linear decay from max at day 0 to 0 at `recency_penalty_after_days` |
| `"step"` | Full max until `recency_half_life_days`, then 0 until penalty threshold |
| `"none"` | Always 0.0 — disables recency entirely |
| `"custom"` | Piecewise-linear interpolation over `recency_curve` control points |

All strategies (except `"none"` and `"custom"`) apply a shared linear old-content penalty beyond `recency_penalty_after_days`: 5% per year, capped at `recency_max_penalty`.

### SearchResult

| Field | Type | Required | Description |
|---|---|---|---|
| `url` | string | yes | Canonical URL of the result |
| `title` | string | yes | Page title |
| `excerpt` | string | yes | Text excerpt |
| `date` | string | yes | ISO 8601 date (YYYY-MM-DD) |
| `score` | f64 | no (default 0) | Upstream search score; used as base score |
| `content_type` | string | no | Content type (e.g., `"article"`, `"page"`) |
| `site_name` | string | no | Site name or source |
| *(any)* | any | no | Additional fields are preserved via `serde(flatten)` |

### Prompt Templates

Three templates are available via `get_prompt` / `resolve_prompt`:

| Name | Purpose | Placeholders |
|---|---|---|
| `expand_query` | Asks the LLM for 2-4 alternative search terms | `{SITE_NAME}`, `{SITE_DESCRIPTION}` |
| `summarize` | Asks the LLM to summarize search result excerpts | `{SITE_NAME}`, `{SITE_DESCRIPTION}` |
| `follow_up` | Handles follow-up questions in a search conversation | `{SITE_NAME}`, `{SITE_DESCRIPTION}` |

### Expansion Response Formats

`parse_expansion` accepts responses in priority order:

1. **Bare JSON array** — `["term1", "term2", "term3"]`
2. **Markdown-wrapped JSON** — ` ```json\n["term1"]\n``` ` or ` ```\n["term1"]\n``` `
3. **Fallback** — plain text split on newlines and commas; quotes and whitespace stripped

In all cases, stop words, single-character strings, and pure numbers are filtered out.

---

## 5. Error Handling

The `ScoltaError` enum covers all error conditions. In wasm-bindgen exports, errors are converted to `JsError` and surface as JavaScript exceptions.

| Variant | When |
|---|---|
| `InvalidJson { function, detail }` | Input could not be parsed as valid JSON |
| `MissingField { function, field }` | A required input field was absent |
| `InvalidFieldType { function, field, expected }` | A field was present but had the wrong type |
| `UnknownPrompt { name }` | `resolve_prompt` or `get_prompt` received an unrecognized prompt name |
| `UnknownFunction { name }` | (Server-only) `debug_call` received an unrecognized function name |
| `ParseError { function, detail }` | Failed to parse or process input data |
| `ConfigWarning { field, message }` | A config value is outside its reasonable range (non-fatal; scoring proceeds) |

Every error message includes the originating function name. Example:

```
score_results: missing required field 'query'
resolve_prompt: unknown prompt template 'nonexistent'
```

In JavaScript, catch the error as a standard `Error`:

```js
import init, { score_results } from './pkg/scolta_core.js';
await init();

try {
  const scored = score_results(JSON.stringify({ query: 'test', results: [] }));
} catch (e) {
  console.error('scolta-core error:', e.message);
}
```

---

## 6. Building

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install wasm-pack
cargo install wasm-pack
```

### Build for browser

```bash
cd packages/scolta-core

# Release build (optimized for size)
wasm-pack build --target web --release

# Output files:
pkg/scolta_core_bg.wasm   # The WASM binary
pkg/scolta_core.js        # ES module wrapper + wasm-bindgen glue
pkg/scolta_core.d.ts      # TypeScript definitions
```

### Run unit tests

```bash
# No wasm-pack needed for unit tests
cargo test

# Format check
cargo fmt --check

# Lint
cargo clippy -- -D warnings
```

### Verify WASM output

```bash
wasm-pack build --target web --release
test -f pkg/scolta_core_bg.wasm
test -f pkg/scolta_core.js
test -f pkg/scolta_core.d.ts
```

### WASM binary size

The release binary is optimized with `opt-level = "s"`, LTO, symbol stripping, and `codegen-units = 1`. Typical size is under 500 KB. The CI build job reports the binary size and warns if it exceeds 500 KB.

---

## 7. Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `wasm-bindgen` | 0.2 | WASM/JavaScript interop |
| `js-sys` | 0.3 | JavaScript type bindings |
| `serde` | 1 | Serialization framework (with derive feature) |
| `serde_json` | 1 | JSON parsing and serialization |
