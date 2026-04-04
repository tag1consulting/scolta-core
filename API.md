# Scolta Core — API Reference

WebAssembly module for the Scolta search engine. This is the **source of truth** for search scoring, HTML processing, prompt management, and query expansion. All platform adapters (PHP, Python, JS, Rust) call into this WASM module via [Extism](https://extism.org/) to get identical behavior.

Version: **0.1.0** · Target: `wasm32-wasip1` · Framework: Extism PDK

---

## Quick Start

The module exports 11 functions. Call `describe` with no input to get a machine-readable catalog of all exports at runtime.

| Function | Input | Output | Purpose |
|---|---|---|---|
| `resolve_prompt` | JSON object | string | Substitute variables into a prompt template |
| `get_prompt` | plain string | string | Get raw template with `{PLACEHOLDERS}` intact |
| `clean_html` | JSON object | string | Extract main content from HTML, strip chrome |
| `build_pagefind_html` | JSON object | string | Generate Pagefind-compatible HTML document |
| `score_results` | JSON object | JSON array | Score and rank search results |
| `merge_results` | JSON object | JSON array | Merge primary + expanded results, deduplicate |
| `to_js_scoring_config` | JSON object | JSON object | Export config as SCREAMING_SNAKE_CASE for JS frontends |
| `parse_expansion` | plain string | JSON array | Parse LLM expansion response into term array |
| `version` | (none) | string | Get crate version |
| `describe` | (none) | JSON object | Self-documenting function catalog |
| `debug_call` | JSON object | JSON object | Profile any function with timing metrics |

---

## Data Types

### SearchResult

```json
{
  "url": "https://example.com/page",
  "title": "Page Title",
  "excerpt": "Text excerpt from the page...",
  "date": "2026-01-15",
  "score": 0.0,
  "content_type": "article",
  "site_name": "Example Site"
}
```

Required fields: `url`, `title`, `excerpt`, `date`. All others default to empty/zero if omitted. Unknown fields are preserved through `score_results` and `merge_results` (they pass through via `#[serde(flatten)]`).

### ScoringConfig

All fields are optional. Missing fields use defaults.

| Field | Type | Default | Range | Description |
|---|---|---|---|---|
| `recency_boost_max` | f64 | 0.5 | 0.0–2.0 | Maximum additive boost for recent content |
| `recency_half_life_days` | u32 | 365 | 1–3650 | Content younger than this gets a boost |
| `recency_penalty_after_days` | u32 | 1825 | 1–7300 | Content older than this gets a penalty |
| `recency_max_penalty` | f64 | 0.3 | 0.0–1.0 | Maximum penalty subtracted from old content |
| `title_match_boost` | f64 | 1.0 | 0.0–5.0 | Boost when any query term is in the title |
| `title_all_terms_multiplier` | f64 | 1.5 | 0.0–5.0 | Boost when ALL query terms are in the title |
| `content_match_boost` | f64 | 0.4 | 0.0–5.0 | Boost when any query term is in the excerpt |
| `content_all_terms_multiplier` | f64 | 0.48 | 0.0–5.0 | Boost when ALL query terms are in the excerpt |
| `expand_primary_weight` | f64 | 0.7 | 0.0–1.0 | Weight for primary results in merge (expanded gets `1.0 - this`) |
| `excerpt_length` | u32 | 300 | 50–2000 | Maximum excerpt length in characters |
| `results_per_page` | u32 | 10 | 1–100 | Results per page for pagination |
| `max_pagefind_results` | u32 | 50 | 1–500 | Maximum results from Pagefind to consider |

Values outside their ranges still work but produce warnings from `validate()`. Use `to_js_scoring_config` to also pass through AI feature flags (`ai_expand_query`, `ai_summarize`, `ai_summary_top_n`, `ai_summary_max_chars`, `ai_max_followups`).

---

## Error Format

Every error message includes the originating function name so you can diagnose problems from log files without tracing the call stack:

```
score_results: missing required field 'query'
clean_html: invalid JSON input: expected JSON object
resolve_prompt: unknown prompt template 'nonexistent'
debug_call: unknown function 'fake_function'
merge_results: failed to parse original results: missing field `url`
```

Error categories: `InvalidJson`, `MissingField`, `InvalidFieldType`, `UnknownPrompt`, `UnknownFunction`, `ParseError`, `ConfigWarning`.

---

## Functions

### resolve_prompt

Resolve a prompt template with site-specific variable substitution.

**Input** (JSON object):
```json
{
  "prompt_name": "expand_query",
  "site_name": "ACME Corp",
  "site_description": "the premier widget supplier"
}
```

**Output** (string): The template with `{SITE_NAME}` and `{SITE_DESCRIPTION}` replaced.

| Field | Required | Default | Description |
|---|---|---|---|
| `prompt_name` | yes | — | One of: `"expand_query"`, `"summarize"`, `"follow_up"` |
| `site_name` | no | `""` | Substituted for `{SITE_NAME}` |
| `site_description` | no | `""` | Substituted for `{SITE_DESCRIPTION}` |

**Errors:** unknown `prompt_name`, missing `prompt_name`, input not a JSON object.

---

### get_prompt

Get the raw prompt template without substitution.

**Input** (plain string): The prompt name — `"expand_query"`, `"summarize"`, or `"follow_up"`.

This is NOT a JSON object. Pass the name as a bare string.

**Output** (string): The raw template with `{SITE_NAME}` and `{SITE_DESCRIPTION}` placeholders intact.

**Errors:** unknown prompt name.

---

### clean_html

Strip page chrome and extract main content as plain text.

**Input** (JSON object):
```json
{
  "html": "<!DOCTYPE html><html>...</html>",
  "title": "Page Title"
}
```

| Field | Required | Default | Description |
|---|---|---|---|
| `html` | yes | — | Complete HTML document or fragment |
| `title` | no | `""` | If provided, removes duplicate title from beginning of output |

**Output** (string): Cleaned plain text with normalized whitespace.

**Processing pipeline:**

1. Strip HTML comments
2. Extract `<div id="main-content">` if present (case-insensitive, handles nested divs), otherwise fall back to `<body>`
3. Remove footer regions (`<footer>`, `.site-footer`, `#footer`)
4. Remove `<script>`, `<style>`, `<nav>` elements (handles multiline content)
5. Strip all remaining HTML tags
6. Normalize whitespace (collapse runs of spaces/newlines to single space)
7. Remove duplicate title from the beginning of output

All regex matching is case-insensitive and handles multiline content correctly. Regex patterns are compiled once via `OnceLock` for performance.

**Errors:** missing `html` field, input not a JSON object.

---

### build_pagefind_html

Generate a minimal HTML document compatible with [Pagefind](https://pagefind.app/) indexing.

**Input** (JSON object):
```json
{
  "id": "doc-123",
  "title": "Building Scalable Drupal",
  "body": "Drupal is a powerful CMS...",
  "url": "https://example.com/article",
  "date": "2024-01-15",
  "site_name": "Example Site"
}
```

| Field | Required | Default | Description |
|---|---|---|---|
| `id` | yes | — | Unique document identifier (becomes body `id` attribute) |
| `title` | yes | — | Page title (in `<title>` and `<h1>`) |
| `body` | yes | — | Cleaned text content |
| `url` | yes | — | Full URL (stored as `data-pagefind-meta="url:..."`) |
| `date` | no | `""` | ISO 8601 date (stored as `data-pagefind-meta="date:..."` if non-empty) |
| `site_name` | no | `""` | Site name (stored as `data-pagefind-filter="site:..."` if non-empty) |

**Output** (string): Complete HTML document with `data-pagefind-body`, `data-pagefind-meta`, and `data-pagefind-filter` attributes. All field values are HTML-escaped.

**Errors:** missing required fields, input not a JSON object.

---

### score_results

Score and re-rank search results by relevance.

**Input** (JSON object):
```json
{
  "query": "drupal performance",
  "results": [
    {"url": "https://a.com", "title": "Drupal Guide", "excerpt": "All about Drupal...", "date": "2026-03-01"},
    {"url": "https://b.com", "title": "About Us", "excerpt": "Company info", "date": "2020-01-01"}
  ],
  "config": {}
}
```

| Field | Required | Default | Description |
|---|---|---|---|
| `query` | yes | — | User search query |
| `results` | yes | — | Array of SearchResult objects |
| `config` | no | `{}` | ScoringConfig overrides |

**Output** (JSON array): Same SearchResult objects with `score` field computed and sorted descending.

**Scoring formula (additive):**
```
final_score = base_score + title_boost + content_boost + recency_boost
```

- `base_score`: The result's existing `score` if > 0, otherwise 1.0. Scolta supplements upstream ranking (e.g., from Pagefind), it does not replace it.
- `title_boost`: Proportional — `title_match_boost * (matching_terms / total_terms)`. If ALL query terms match, multiplied by `title_all_terms_multiplier`.
- `content_boost`: Same proportional logic as title, with `content_match_boost` / `content_all_terms_multiplier`.
- `recency_boost`: Exponential decay based on document age. Recent (< `recency_penalty_after_days`) → positive boost up to `recency_boost_max`, decaying with half-life `recency_half_life_days` using `max_boost * exp(-age / half_life * ln2)`. Old (> `recency_penalty_after_days`) → negative penalty up to `-recency_max_penalty`. Unparseable dates → neutral 0.0.

Query term extraction filters stop words (the, is, what, how, etc.) and single-character terms before matching.

**Errors:** missing fields, unparseable results array, input not a JSON object.

---

### merge_results

Merge primary search results with query-expansion results, deduplicating by URL.

**Input** (JSON object):
```json
{
  "original": [
    {"url": "https://a.com", "title": "A", "excerpt": "...", "date": "2026-01-01", "score": 10.0}
  ],
  "expanded": [
    {"url": "https://a.com", "title": "A", "excerpt": "...", "date": "2026-01-01", "score": 5.0},
    {"url": "https://b.com", "title": "B", "excerpt": "...", "date": "2025-06-01", "score": 3.0}
  ],
  "config": {"expand_primary_weight": 0.7}
}
```

| Field | Required | Default | Description |
|---|---|---|---|
| `original` | yes | — | Results from primary search query |
| `expanded` | yes | — | Results from expanded query variations |
| `config` | no | `{}` | ScoringConfig overrides (only `expand_primary_weight` applies here) |

**Output** (JSON array): Merged results sorted by combined score.

**Deduplication logic:**

- Original result scores are multiplied by `expand_primary_weight` (default 0.7)
- Expanded result scores are multiplied by `1.0 - expand_primary_weight` (default 0.3)
- When the same URL appears in both sets, scores are summed; the first occurrence's metadata is kept
- Final list is sorted by combined score (descending)

**Errors:** missing fields, unparseable results arrays, input not a JSON object.

---

### to_js_scoring_config

Export scoring configuration as SCREAMING_SNAKE_CASE JSON for JavaScript frontend integration. This is a convenience function for JS consumers — other language adapters should use config fields directly.

**Input** (JSON object): ScoringConfig fields plus optional AI toggle flags.

**Output** (JSON object):
```json
{
  "RECENCY_BOOST_MAX": 0.5,
  "RECENCY_HALF_LIFE_DAYS": 365,
  "RECENCY_PENALTY_AFTER_DAYS": 1825,
  "RECENCY_MAX_PENALTY": 0.3,
  "TITLE_MATCH_BOOST": 1.0,
  "TITLE_ALL_TERMS_MULTIPLIER": 1.5,
  "CONTENT_MATCH_BOOST": 0.4,
  "CONTENT_ALL_TERMS_MULTIPLIER": 0.48,
  "EXPAND_PRIMARY_WEIGHT": 0.7,
  "EXCERPT_LENGTH": 300,
  "RESULTS_PER_PAGE": 10,
  "MAX_PAGEFIND_RESULTS": 50,
  "AI_EXPAND_QUERY": true,
  "AI_SUMMARIZE": true,
  "AI_SUMMARY_TOP_N": 5,
  "AI_SUMMARY_MAX_CHARS": 2000,
  "AI_MAX_FOLLOWUPS": 3
}
```

AI toggle fields (`ai_expand_query`, `ai_summarize`, `ai_summary_top_n`, `ai_summary_max_chars`, `ai_max_followups`) are passed through from the input. They are frontend feature flags, not part of the scoring algorithm.

**Errors:** input not a JSON object.

---

### parse_expansion

Parse an LLM expansion response into a filtered term array.

**Input** (plain string): Raw LLM response text. Not JSON-wrapped — pass the response body directly.

**Output** (JSON array): Cleaned search terms.

**Parsing priority:**

1. Markdown-wrapped JSON: `` ```json ["term1", "term2"] ``` ``
2. Bare JSON array: `["term1", "term2"]`
3. Fallback: split by newlines and commas

**Filtering** (applied to all parsed terms):

- Removes empty strings
- Removes strings shorter than 2 characters
- Removes pure numbers (e.g., `"123"`)
- Removes stop words (the, a, is, of, etc.)

This function never errors — it always returns an array (possibly empty).

---

### version

**Input:** none.

**Output** (string): Semantic version from `Cargo.toml` (currently `"0.1.0"`).

---

### describe

**Input:** none.

**Output** (JSON object): Machine-readable catalog of all exported functions, including names, descriptions, input types, required/optional fields, and output descriptions. Use this for runtime discovery and tooling integration.

```json
{
  "name": "scolta-core",
  "version": "0.1.0",
  "description": "Scolta search engine core — ...",
  "functions": {
    "resolve_prompt": { "description": "...", "input_type": "json", "input_fields": {...}, ... },
    "get_prompt": { "description": "...", "input_type": "string", ... },
    ...
  }
}
```

---

### debug_call

Profile any function with timing and size metrics. Useful for performance tuning and debugging during adapter development.

**Input** (JSON object):
```json
{
  "function": "clean_html",
  "input": "{\"html\": \"<html><p>Test</p></html>\", \"title\": \"\"}"
}
```

| Field | Required | Default | Description |
|---|---|---|---|
| `function` | yes | — | Name of any exported function (see table above) |
| `input` | no | `""` | Stringified input for the target function |

**Output** (JSON object):
```json
{
  "output": "Test",
  "error": null,
  "time_us": 1234,
  "input_size": 47,
  "output_size": 4
}
```

| Field | Type | Description |
|---|---|---|
| `output` | string or null | Function output on success, null on error |
| `error` | string or null | Error message on failure, null on success |
| `time_us` | integer | Elapsed time in microseconds |
| `input_size` | integer | Input string length in bytes |
| `output_size` | integer | Output string length in bytes (0 on error) |

**Valid function names:** `resolve_prompt`, `get_prompt`, `clean_html`, `build_pagefind_html`, `score_results`, `merge_results`, `to_js_scoring_config`, `parse_expansion`, `version`, `describe`.

**Errors:** unknown function name, input not a JSON object.

---

## Performance Characteristics

| Function | Typical time | Notes |
|---|---|---|
| `resolve_prompt` | < 1ms | String substitution |
| `get_prompt` | < 0.1ms | Constant lookup |
| `clean_html` | 5–50ms | Depends on HTML size; regex compiled once via OnceLock |
| `build_pagefind_html` | < 2ms | Template generation with HTML escaping |
| `to_js_scoring_config` | < 1ms | Object creation |
| `score_results` | 5–200ms | Linear in result count |
| `merge_results` | 10–100ms | Linear in total result count |
| `parse_expansion` | 1–5ms | String parsing |
| `version` | < 0.1ms | Constant return |
| `describe` | < 0.5ms | Builds JSON object |

No persistent state between calls. WASM memory overhead: ~256KB minimum. The `chrono` crate is intentionally avoided (saves ~400KB in the binary) — date math uses Howard Hinnant's `civil_from_days` algorithm inline.

---

## How to Use with PHP

PHP adapters use [Extism PHP SDK](https://github.com/extism/php-sdk) to load and call the WASM module.

### Setup

```php
use Extism\Plugin;
use Extism\Manifest;
use Extism\PathWasm;

$wasm = new PathWasm('/path/to/scolta_core.wasm');
$manifest = new Manifest($wasm);
$plugin = new Plugin($manifest, true); // true = enable WASI
```

### Calling functions

Every call follows the same pattern: encode input as a JSON string, call the function, decode the output.

```php
// resolve_prompt — input is a JSON object
$input = json_encode([
    'prompt_name' => 'expand_query',
    'site_name' => 'My Drupal Site',
    'site_description' => 'a community resource for developers',
]);
$prompt = $plugin->call('resolve_prompt', $input);
// $prompt is a plain string — the resolved template

// get_prompt — input is a bare string, NOT JSON
$template = $plugin->call('get_prompt', 'expand_query');
// $template contains {SITE_NAME} and {SITE_DESCRIPTION} placeholders

// clean_html
$cleaned = $plugin->call('clean_html', json_encode([
    'html' => $rawHtml,
    'title' => $pageTitle,
]));

// build_pagefind_html
$pagefindDoc = $plugin->call('build_pagefind_html', json_encode([
    'id' => $node->id(),
    'title' => $node->getTitle(),
    'body' => $cleaned,
    'url' => $node->toUrl()->toString(),
    'date' => $node->getCreatedTime() ? date('Y-m-d', $node->getCreatedTime()) : '',
    'site_name' => \Drupal::config('system.site')->get('name'),
]));

// score_results
$scored = json_decode($plugin->call('score_results', json_encode([
    'query' => $userQuery,
    'results' => $pagefindResults,
    'config' => ['recency_boost_max' => 0.8],
])), true);
// $scored is a PHP array of results sorted by score

// merge_results
$merged = json_decode($plugin->call('merge_results', json_encode([
    'original' => $primaryResults,
    'expanded' => $expandedResults,
    'config' => ['expand_primary_weight' => 0.7],
])), true);

// parse_expansion — input is a bare string (the raw LLM response)
$terms = json_decode($plugin->call('parse_expansion', $llmResponse), true);
// $terms is a PHP array of strings

// version
$version = $plugin->call('version', '');
```

### Error handling

```php
try {
    $result = $plugin->call('score_results', json_encode(['query' => 'test']));
} catch (\Extism\PluginException $e) {
    // Error message includes the function name:
    // "score_results: missing required field 'results'"
    error_log('Scolta error: ' . $e->getMessage());
}
```

### Config validation

```php
// Get the JS config (includes validation implicitly via from_json)
$jsConfig = json_decode($plugin->call('to_js_scoring_config', json_encode([
    'recency_boost_max' => 0.5,
    'ai_expand_query' => true,
    'ai_summarize' => true,
])), true);
// Use $jsConfig to populate window.scolta in your frontend JS
```

---

## How to Use with Python

Python adapters use [Extism Python SDK](https://github.com/extism/python-sdk).

### Setup

```python
import extism
import json

manifest = {"wasm": [{"path": "/path/to/scolta_core.wasm"}]}
plugin = extism.Plugin(manifest, wasi=True)
```

### Calling functions

```python
# resolve_prompt
prompt = plugin.call("resolve_prompt", json.dumps({
    "prompt_name": "expand_query",
    "site_name": "My Site",
    "site_description": "a knowledge base for developers",
}).encode()).decode()

# get_prompt — bare string input, not JSON
template = plugin.call("get_prompt", b"summarize").decode()

# clean_html
cleaned = plugin.call("clean_html", json.dumps({
    "html": raw_html,
    "title": page_title,
}).encode()).decode()

# build_pagefind_html
pagefind_doc = plugin.call("build_pagefind_html", json.dumps({
    "id": doc_id,
    "title": title,
    "body": cleaned,
    "url": url,
    "date": "2026-01-15",
    "site_name": "My Site",
}).encode()).decode()

# score_results
scored = json.loads(plugin.call("score_results", json.dumps({
    "query": user_query,
    "results": pagefind_results,
    "config": {},
}).encode()))

# merge_results
merged = json.loads(plugin.call("merge_results", json.dumps({
    "original": primary_results,
    "expanded": expanded_results,
    "config": {"expand_primary_weight": 0.7},
}).encode()))

# parse_expansion — bare string input
terms = json.loads(plugin.call("parse_expansion", llm_response.encode()))

# version
version = plugin.call("version", b"").decode()

# describe — runtime introspection
catalog = json.loads(plugin.call("describe", b""))
for name, info in catalog["functions"].items():
    print(f"{name}: {info['description']}")
```

### Error handling

```python
try:
    result = plugin.call("score_results", json.dumps({"query": "test"}).encode())
except extism.Error as e:
    # "score_results: missing required field 'results'"
    print(f"Scolta error: {e}")
```

### Debug profiling

```python
profile = json.loads(plugin.call("debug_call", json.dumps({
    "function": "clean_html",
    "input": json.dumps({"html": raw_html, "title": ""}),
}).encode()))

if profile["error"]:
    print(f"Error: {profile['error']}")
else:
    print(f"Output length: {profile['output_size']} bytes")
    print(f"Time: {profile['time_us']}µs")
```

---

## How to Use with JavaScript

JavaScript adapters use [Extism JS SDK](https://github.com/extism/js-sdk).

### Setup (Node.js)

```javascript
import createPlugin from '@extism/extism';

const plugin = await createPlugin(
  { wasm: [{ path: './scolta_core.wasm' }] },
  { useWasi: true }
);
```

### Setup (Browser with Extism)

```javascript
import createPlugin from 'https://cdn.jsdelivr.net/npm/@extism/extism/dist/browser/mod.js';

const plugin = await createPlugin(
  { wasm: [{ url: '/wasm/scolta_core.wasm' }] },
  { useWasi: true }
);
```

### Calling functions

```javascript
// Helper: call with JSON input, parse JSON output
async function callJson(plugin, fn, input) {
  const result = await plugin.call(fn, JSON.stringify(input));
  return JSON.parse(new TextDecoder().decode(result.buffer));
}

// Helper: call with JSON input, get string output
async function callString(plugin, fn, input) {
  const result = await plugin.call(fn, JSON.stringify(input));
  return new TextDecoder().decode(result.buffer);
}

// resolve_prompt
const prompt = await callString(plugin, 'resolve_prompt', {
  prompt_name: 'expand_query',
  site_name: 'My Site',
  site_description: 'a developer resource',
});

// get_prompt — bare string input
const result = await plugin.call('get_prompt', 'summarize');
const template = new TextDecoder().decode(result.buffer);

// clean_html
const cleaned = await callString(plugin, 'clean_html', {
  html: rawHtml,
  title: pageTitle,
});

// build_pagefind_html
const pagefindDoc = await callString(plugin, 'build_pagefind_html', {
  id: 'doc-42',
  title: 'Article Title',
  body: cleaned,
  url: 'https://example.com/article',
  date: '2026-01-15',
  site_name: 'My Site',
});

// score_results
const scored = await callJson(plugin, 'score_results', {
  query: userQuery,
  results: pagefindResults,
  config: { recency_boost_max: 0.5 },
});

// merge_results
const merged = await callJson(plugin, 'merge_results', {
  original: primaryResults,
  expanded: expandedResults,
  config: { expand_primary_weight: 0.7 },
});

// to_js_scoring_config — set up window.scolta
window.scolta = await callJson(plugin, 'to_js_scoring_config', {
  recency_boost_max: 0.5,
  ai_expand_query: true,
  ai_summarize: true,
  ai_summary_top_n: 5,
});

// parse_expansion — bare string input
const expansionResult = await plugin.call('parse_expansion', llmResponse);
const terms = JSON.parse(new TextDecoder().decode(expansionResult.buffer));

// version
const versionResult = await plugin.call('version', '');
const version = new TextDecoder().decode(versionResult.buffer);

// describe — runtime function catalog
const catalog = await callJson(plugin, 'describe', {});
console.log('Available functions:', Object.keys(catalog.functions));
```

### Error handling

```javascript
try {
  const result = await callJson(plugin, 'score_results', { query: 'test' });
} catch (err) {
  // "score_results: missing required field 'results'"
  console.error('Scolta error:', err.message);
}
```

---

## How to Use with Rust

Rust consumers can either call the WASM module via Extism host SDK, or use the crate directly as a library dependency (since it also builds as `rlib`).

### Option A: Direct library dependency (same process)

Add to `Cargo.toml`:
```toml
[dependencies]
scolta-core = { path = "../scolta-core" }
serde_json = "1"
```

Call the `inner::` functions directly — no serialization overhead:

```rust
use scolta_core::inner;
use serde_json::json;

fn main() {
    // resolve_prompt
    let input = json!({
        "prompt_name": "expand_query",
        "site_name": "My Site",
        "site_description": "a developer resource"
    });
    let prompt = inner::resolve_prompt(&input).expect("resolve failed");
    println!("{}", prompt);

    // get_prompt — takes &str, not JSON
    let template = inner::get_prompt("summarize").expect("unknown prompt");

    // clean_html
    let cleaned = inner::clean_html(&json!({
        "html": "<html><body><p>Hello</p><script>x()</script></body></html>",
        "title": ""
    })).expect("clean failed");

    // build_pagefind_html
    let doc = inner::build_pagefind_html(&json!({
        "id": "doc-1",
        "title": "Test",
        "body": "Content",
        "url": "https://example.com",
        "date": "2026-01-15",
        "site_name": "Example"
    })).expect("build failed");

    // score_results
    let scored = inner::score_results(&json!({
        "query": "drupal performance",
        "results": [
            {"url": "https://a.com", "title": "Drupal Guide", "excerpt": "About Drupal", "date": "2026-03-01"}
        ],
        "config": {}
    })).expect("scoring failed");

    // merge_results
    let merged = inner::merge_results(&json!({
        "original": [{"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 10.0}],
        "expanded": [{"url": "https://b.com", "title": "B", "excerpt": "b", "date": "2025-06-01", "score": 3.0}],
        "config": {"expand_primary_weight": 0.7}
    })).expect("merge failed");

    // parse_expansion — takes &str
    let terms = inner::parse_expansion(r#"["term1", "term2", "term3"]"#);

    // version
    let ver = inner::version();

    // describe
    let catalog = inner::describe();
    println!("{}", serde_json::to_string_pretty(&catalog).unwrap());
}
```

### Option B: Using individual modules directly

For finer-grained control, use the public modules:

```rust
use scolta_core::scoring::{ScoringConfig, SearchResult, score_results, merge_results};
use scolta_core::config::{from_json, from_json_validated, to_js_scoring_config};
use scolta_core::html::{clean_html, build_pagefind_html};
use scolta_core::expansion::parse_expansion;
use scolta_core::prompts::{get_template, resolve_template};
use scolta_core::common::{extract_terms, is_stop_word, is_valid_term};
use scolta_core::debug::{measure_call, debug_result_to_json};

// Config with validation
let config_json = serde_json::json!({"recency_boost_max": 5.0});
let (config, warnings) = from_json_validated(&config_json);
for w in &warnings {
    eprintln!("Warning: {} — {}", w.field, w.message);
}

// Scoring
let mut results: Vec<SearchResult> = serde_json::from_value(results_json).unwrap();
score_results(&mut results, "search query", &config);

// Term extraction
let terms = extract_terms("what is drupal performance tuning");
// → ["drupal", "performance", "tuning"]
```

### Option C: Extism host SDK (cross-process)

```rust
use extism::*;

fn main() -> Result<(), Error> {
    let manifest = Manifest::new([Wasm::file("/path/to/scolta_core.wasm")]);
    let mut plugin = Plugin::new(&manifest, [], true)?; // true = WASI

    // All calls use byte slices
    let output = plugin.call::<&str, &str>("resolve_prompt", r#"{"prompt_name":"expand_query","site_name":"Test","site_description":"a test"}"#)?;
    println!("{}", output);

    let scored = plugin.call::<&str, &str>("score_results", &serde_json::to_string(&input)?)?;
    let results: Vec<serde_json::Value> = serde_json::from_str(scored)?;

    Ok(())
}
```

### Error handling (Rust-native)

When using `inner::` functions, errors are typed `ScoltaError`:

```rust
use scolta_core::error::ScoltaError;

match inner::resolve_prompt(&input) {
    Ok(prompt) => println!("{}", prompt),
    Err(ScoltaError::UnknownPrompt { name }) => {
        eprintln!("No such prompt: {}", name);
    }
    Err(ScoltaError::MissingField { function, field }) => {
        eprintln!("{} requires field '{}'", function, field);
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

---

## Building

```bash
# Install the WASM target
rustup target add wasm32-wasip1

# Build the plugin
cargo build --target wasm32-wasip1 --release

# Output: target/wasm32-wasip1/release/scolta_core.wasm

# Run tests (native target)
cargo test

# Run tests with output
cargo test -- --nocapture
```

The release profile is optimized for size (`opt-level = "s"`, LTO, strip symbols, single codegen unit, panic=abort).

---

## Backward Compatibility

This module replaces the PHP-native scoring logic. The canonical field and function names are defined here; adapters map to them.

| WASM function | PHP equivalent |
|---|---|
| `resolve_prompt` | `ScoltaPrompts::resolveTemplate()` |
| `get_prompt` | `ScoltaPrompts::getTemplate()` |
| `clean_html` | `ContentExporter::cleanHtml()` |
| `build_pagefind_html` | `ContentExporter::buildPagefindHtml()` |
| `to_js_scoring_config` | `ScoltaConfig::toJsScoringConfig()` |
| `score_results` | `ScoltaScoring::scoreResults()` |
| `merge_results` | `ScoltaScoring::mergeResults()` |
| `parse_expansion` | (new — was inline in PHP) |
| `version` | (new) |
| `describe` | (new) |
| `debug_call` | (new) |

Key differences from the PHP implementation:

- `resolve_prompt` reads `"prompt_name"` (not `"template"`)
- `merge_results` uses `config.expand_primary_weight` exclusively (no top-level `"primary_weight"` override)
- `debug_call` returns separate `output` and `error` fields (not a single output string)
- `content_all_terms_multiplier` is an explicit config field (was hardcoded as `content_match_boost * 1.2` in PHP)
- Unparseable dates produce neutral recency (0.0 additive), not a silent boost
- All errors include the originating function name
