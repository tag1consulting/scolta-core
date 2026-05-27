# Scolta Core API Reference

Version: 1.0.0 · Target: wasm32-unknown-unknown · Framework: wasm-bindgen

## 1. Overview

`scolta-core` is the Rust/WebAssembly module that powers Scolta's client-side search experience. It provides search result scoring, prompt template management, LLM expansion response parsing, context extraction, PII sanitization, and conversation trimming. By running in the browser via wasm-bindgen, it guarantees identical scoring behavior across all platform adapters (PHP, WordPress, Drupal, Laravel) without server round-trips.

**Architecture:** `browser.rs` contains 13 `#[wasm_bindgen]` exports that form the public API. Each export is a thin serialization wrapper delegating to a corresponding function in the `inner::` module (`lib.rs`). The `inner::` functions are plain Rust — testable with `cargo test` without a WASM runtime — and are the sole implementation of all business logic. Modules `scoring`, `config`, `prompts`, `expansion`, `context`, `conversation`, `sanitize`, `error`, `common`, and `stop_words` contain the algorithmic details.

---

## 2. Browser WASM Exports

All 13 exports take and return JSON strings (or plain strings for `version`, `get_prompt`, and `describe`). On error they return a `JsError` that becomes a JavaScript exception.

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
- `sort_override` — optional object `{ "field": "...", "direction": "asc"|"desc" }`. When present, results that lack the named metadata field are excluded, and the remaining results are sorted by that field's value (numeric strings compared numerically, others lexicographically). Relevance score is used as a tiebreaker for equal field values. Omit to use the default relevance-ranked behavior.
- `primary_query` — optional string. When provided, the title boost for each result is the maximum of the title boost computed from `query` and from `primary_query`. Used by AI query expansion to award title boosts for results whose titles match the original user query.

**Output JSON:** Array of `SearchResult` objects sorted by computed score descending (or by `sort_override` field when specified). Each result gains a `score` field with the computed relevance value.

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
final_score = (base_score * source_weight) + title_boost + content_boost + recency_boost + priority_boost
```

Where `base_score` is the upstream engine score (e.g., from Pagefind) if present, otherwise 1.0. `source_weight` dampens results from secondary sources (default 1.0). `priority_boost` is added when the result URL matches a configured priority page and the query contains that page's keywords.

---

### `merge_results(input: &str) -> Result<String, JsError>`

Merge N scored result sets with per-set weights and deduplication.

**Input JSON:**
```json
{
  "sets": [
    { "results": [/* array of SearchResult */], "weight": 1.0 },
    { "results": [/* array of SearchResult */], "weight": 0.7 }
  ],
  "deduplicate_by": "url",
  "case_sensitive": false,
  "exclude_urls": ["/admin"],
  "normalize_urls": true,
  "debug": false
}
```

- `sets` — required array. Each entry has `results` (array of SearchResult) and `weight` (f64 multiplier).
- `deduplicate_by` — optional string. When set to `"url"` or `"title"`, duplicates are collapsed keeping the highest-weighted entry.
- `case_sensitive` — optional boolean (default false). Whether deduplication key comparison is case-sensitive.
- `exclude_urls` — optional string array. URLs containing any of these substrings are removed from results.
- `normalize_urls` — optional boolean (default false). When true, URLs are normalized (trailing slash stripped, lowercased) before deduplication.
- `debug` — optional boolean (default false). When true, output changes to `{"results": [...], "debug": {...}}` with per-set input counts, dedup statistics, and exclusion counts.

**Output JSON (debug=false):** Merged, weighted, and deduplicated results array sorted by score descending.

**Output JSON (debug=true):**
```json
{
  "results": [/* merged results */],
  "debug": {
    "sets": [{"input_count": 5, "weight": 1.0}],
    "total_before_dedup": 10,
    "total_after_dedup": 7,
    "excluded_count": 1
  }
}
```

---

### `match_priority_pages(input: &str) -> Result<String, JsError>`

Find priority pages matching a query.

**Input JSON:**
```json
{ "query": "search terms", "priority_pages": [...] }
```

- `query` — required string.
- `priority_pages` — required array of PriorityPage objects (see Section 4).

**Output JSON:** Array of matching PriorityPage objects.

---

### `parse_expansion(input: &str) -> Result<String, JsError>`

Parse an LLM expansion response into an array of search terms. Accepts two input forms:

**1. Bare string** — raw LLM response; language defaults to `"en"`.

Handles three text formats with graceful fallback:

- **JSON array** (preferred): `["term1", "term2", "term3"]`
- **Markdown-wrapped JSON**: `` ```json ["term1", "term2"] ``` ``
- **Fallback**: newline- or comma-separated plain text

**2. JSON object** — full configuration including language, generic-term filtering, and merging with existing terms:
```json
{
  "text": "[\"term1\", \"term2\"]",
  "language": "en",
  "generic_terms": ["platform", "solution"],
  "existing_terms": ["drupal"],
  "filter_single_word_generic": true,
  "keep_acronyms": true,
  "keep_proper_nouns": true,
  "min_term_length": 2
}
```

All results are filtered through the stop word list for the given language. Single-character strings and pure numbers are removed.

**Output JSON:** Array of cleaned expansion terms.

```json
["cardiac surgery", "heart procedure", "surgical intervention"]
```

---

### `batch_score_results(input: &str) -> Result<String, JsError>`

Score multiple queries against their respective result sets in a single WASM call. Reduces JS-to-WASM round-trip overhead when re-scoring several result batches at once.

**Input JSON:**
```json
{
  "queries": [
    {
      "query": "search terms",
      "results": [/* array of SearchResult */],
      "config": { "language": "en" }
    },
    {
      "query": "other query",
      "results": [/* array of SearchResult */]
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
  [ { "url": "https://a.com", "score": 2.1, "..." : "..." } ],
  [ { "url": "https://b.com", "score": 1.4, "..." : "..." } ]
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
  "site_description": "the premier widget supplier",
  "dynamic_anchors": ["Focus on pricing.", "Do not mention competitors."]
}
```

- `prompt_name` — required. One of: `"expand_query"`, `"summarize"`, `"follow_up"`.
- `site_name` — optional. Substituted for `{SITE_NAME}` in the template.
- `site_description` — optional. Substituted for `{SITE_DESCRIPTION}` in the template.
- `dynamic_anchors` — optional string array. When the template contains a `{DYNAMIC_ANCHORS}` placeholder, the anchors are joined with newlines and injected. When anchors are absent or the template has no placeholder, the call is a no-op.

**Output:** The resolved prompt string (plain text, not JSON).

**Error:** `JsError` if `prompt_name` is missing or not recognized.

---

### `get_prompt(name: &str) -> Result<String, JsError>`

Get a raw prompt template by name without variable substitution. Useful for displaying templates to administrators or pre-processing before substitution.

**Input:** Prompt name string — one of `"expand_query"`, `"summarize"`, `"follow_up"`.

**Output:** Raw template string containing `{SITE_NAME}` and `{SITE_DESCRIPTION}` placeholders.

**Error:** `JsError` if the name is not recognized.

---

### `extract_context(input: &str) -> Result<String, JsError>`

Extract the most relevant portion of article content for LLM context.

**Input JSON:**
```json
{
  "content": "full article text...",
  "query": "search terms",
  "config": { "max_length": 6000, "intro_length": 2000, "snippet_radius": 500 }
}
```

- `content` — required string. Full article text.
- `query` — required string. Search query for keyword anchoring.
- `config` — optional object. `max_length` (default 6000), `intro_length` (default 2000), `snippet_radius` (default 500), `separator` (default `"\n\n[...]\n\n"`).

**Output JSON:** Extracted context string.

---

### `batch_extract_context(input: &str) -> Result<String, JsError>`

Extract context from multiple content items in one call.

**Input JSON:**
```json
{
  "items": [{ "content": "...", "url": "...", "title": "..." }],
  "query": "search terms",
  "config": { "max_length": 6000 }
}
```

- `items` — required array of objects with `content`, `url`, and `title` fields.
- `query` — required string.
- `config` — optional (same as `extract_context`).

**Output JSON:** Array of `{ "url": "...", "title": "...", "context": "..." }` objects.

---

### `sanitize_query(input: &str) -> Result<String, JsError>`

Redact PII from a query string before analytics logging.

**Input JSON:**
```json
{
  "query": "contact user@example.com",
  "config": {
    "redact_email": true,
    "redact_phone": true,
    "redact_ssn": true,
    "redact_credit_card": true,
    "redact_ip": true,
    "custom_patterns": [{ "regex": "\\bPAT-\\d+\\b", "replacement": "[PATIENT_ID]" }]
  }
}
```

- `query` — required string.
- `config` — optional. All boolean fields default to `true`. Custom patterns use the `regex` crate syntax.

**Output JSON:** Sanitized query string.

---

### `truncate_conversation(input: &str) -> Result<String, JsError>`

Trim conversation history to fit within a character limit.

**Input JSON:**
```json
{
  "messages": [{ "role": "user", "content": "..." }],
  "config": { "max_length": 12000, "preserve_first_n": 2, "removal_unit": 2 }
}
```

- `messages` — required array of `{ "role": "...", "content": "..." }` objects.
- `config` — optional. `max_length` (default 12000), `preserve_first_n` (default 2), `removal_unit` (default 2).

**Output JSON:** Trimmed messages array.

---

### `version() -> String`

Return the scolta-core crate version string (e.g., `"1.0.0-rc4"`). No input.

---

### `describe() -> String`

Return a JSON manifest of all exported functions with metadata. Used by platform adapters to verify compatibility at runtime.

**Output JSON:**
```json
{
  "name": "scolta-core",
  "version": "1.0.0-rc4",
  "wasm_interface_version": 4,
  "description": "Scolta browser WASM — client-side search scoring, prompt management, query expansion, context extraction, PII sanitization, and conversation trimming",
  "functions": {
    "score_results": { "since": "0.1.0", "stability": "stable", "input_type": "json", "output_type": "json" },
    "merge_results": { "since": "0.1.0", "stability": "stable", "input_type": "json", "output_type": "json" },
    "match_priority_pages": { "since": "0.2.3", "stability": "stable", "input_type": "json", "output_type": "json" },
    "parse_expansion": { "since": "0.1.0", "stability": "stable", "input_type": "string", "output_type": "json" },
    "batch_score_results": { "since": "0.2.2", "stability": "stable", "input_type": "json", "output_type": "json" },
    "resolve_prompt": { "since": "0.1.0", "stability": "stable", "input_type": "json", "output_type": "string" },
    "get_prompt": { "since": "0.1.0", "stability": "stable", "input_type": "string", "output_type": "string" },
    "extract_context": { "since": "0.2.3", "stability": "stable", "input_type": "json", "output_type": "string" },
    "batch_extract_context": { "since": "0.2.3", "stability": "stable", "input_type": "json", "output_type": "json" },
    "sanitize_query": { "since": "0.2.3", "stability": "stable", "input_type": "json", "output_type": "string" },
    "truncate_conversation": { "since": "0.2.3", "stability": "stable", "input_type": "json", "output_type": "json" },
    "version": { "since": "0.1.0", "stability": "stable", "input_type": "none", "output_type": "string" },
    "describe": { "since": "0.1.0", "stability": "stable", "input_type": "none", "output_type": "json" }
  }
}
```

`wasm_interface_version` tracks binary compatibility. Platform adapters check this value at load time and throw if it doesn't match the expected version.

---

## 3. Data Formats

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
| `content_all_terms_multiplier` | f64 | 1.2 | Multiplier applied when ALL terms appear in content |
| `phrase_adjacent_multiplier` | f64 | 2.5 | Multiplier on content boost when all query terms are adjacent in Pagefind positions |
| `phrase_near_multiplier` | f64 | 1.5 | Multiplier on content boost when all terms are within `phrase_near_window` positions |
| `phrase_near_window` | u32 | 5 | Maximum word-position span for the "near phrase" bonus |
| `phrase_window` | u32 | 15 | Maximum word-position span beyond which no phrase bonus applies |
| `excerpt_length` | u32 | 300 | Max excerpt length in characters |
| `results_per_page` | u32 | 10 | Results per page for pagination |
| `max_pagefind_results` | u32 | 50 | Max results from Pagefind to consider |
| `language` | string | `"en"` | ISO 639-1 code for stop word filtering (30 languages supported; CJK/unknown returns no filtering) |
| `custom_stop_words` | string[] | `[]` | Additional stop words layered on top of the language list |
| `priority_pages` | PriorityPage[] | `[]` | Priority pages that receive a score boost when query keywords match |

Valid ranges are checked by `ScoringConfig::clamp_and_validate()`. Values outside reasonable ranges are clamped and a warning is returned for each.

**Recency strategy details:**

| Strategy | Behaviour |
|---|---|
| `"exponential"` | `MAX * exp(-age/HALF_LIFE * ln2)` (default) |
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
| `date` | string | no (default `""`) | ISO 8601 date (YYYY-MM-DD) |
| `score` | f64 | no (default 0) | Upstream search score; used as base score |
| `source_weight` | f64 | no (default 1.0) | Dampening factor for secondary-source results |
| `locations` | u32[] | no | Pagefind word-position data for phrase-proximity scoring |
| `content_type` | string | no | Content type (e.g., `"article"`, `"page"`) |
| `site_name` | string | no | Site name or source |
| *(any)* | any | no | Additional fields are preserved via `serde(flatten)` |

### PriorityPage

| Field | Type | Required | Description |
|---|---|---|---|
| `url_pattern` | string | yes | URL path to match against result URLs |
| `keywords` | string[] | yes | Query keywords that trigger the boost (case-insensitive) |
| `boost` | f64 | yes | Score boost to apply when a keyword matches |
| `custom_excerpt` | string | no | Replacement excerpt shown instead of the Pagefind-generated one |
| `page_id` | string | no | Optional identifier for client-side use |

### Prompt Templates

Three templates are available via `get_prompt` / `resolve_prompt`:

| Name | Purpose | Placeholders |
|---|---|---|
| `expand_query` | Asks the LLM for 2-4 alternative search terms | `{SITE_NAME}`, `{SITE_DESCRIPTION}` |
| `summarize` | Asks the LLM to summarize search result excerpts | `{SITE_NAME}`, `{SITE_DESCRIPTION}`, `{DYNAMIC_ANCHORS}` |
| `follow_up` | Handles follow-up questions in a search conversation | `{SITE_NAME}`, `{SITE_DESCRIPTION}`, `{DYNAMIC_ANCHORS}` |

### Expansion Response Formats

`parse_expansion` accepts responses in priority order:

1. **Bare JSON array** — `["term1", "term2", "term3"]`
2. **Markdown-wrapped JSON** — `` ```json ["term1"] ``` `` or `` ``` ["term1"] ``` ``
3. **Fallback** — plain text split on newlines and commas; quotes and whitespace stripped

In all cases, stop words, single-character strings, and pure numbers are filtered out.

---

## 4. Error Handling

The `ScoltaError` enum covers all error conditions. In wasm-bindgen exports, errors are converted to `JsError` and surface as JavaScript exceptions.

| Variant | When |
|---|---|
| `InvalidJson { function, detail }` | Input could not be parsed as valid JSON |
| `MissingField { function, field }` | A required input field was absent |
| `InvalidFieldType { function, field, expected }` | A field was present but had the wrong type |
| `UnknownPrompt { name }` | `resolve_prompt` or `get_prompt` received an unrecognized prompt name |
| `ParseError { function, detail }` | Failed to parse or process input data |

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

## 5. Building

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

### Run tests

```bash
cargo test                           # unit + integration tests
cargo clippy -- -D warnings          # lint
cargo fmt --check                    # formatting
```

### WASM binary size

The release binary is optimized with `opt-level = "s"`, LTO, symbol stripping, and `codegen-units = 1`. Typical size is under 500 KB. The CI build job reports the binary size and warns if it exceeds 500 KB.

---

## 6. Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `wasm-bindgen` | 0.2 | WASM/JavaScript interop |
| `js-sys` | 0.3 | JavaScript type bindings |
| `serde` | 1 | Serialization framework (with derive feature) |
| `serde_json` | 1 | JSON parsing and serialization |
| `regex` | 1 | Pattern matching for PII redaction |
