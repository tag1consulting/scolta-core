# Scolta Core

[![CI](https://github.com/tag1consulting/scolta-core/actions/workflows/ci.yml/badge.svg)](https://github.com/tag1consulting/scolta-core/actions/workflows/ci.yml)

Rust/WASM scoring, ranking, and AI layer that runs in the browser, on top of Pagefind.

## Status

Beta. Scolta is installable and in active use. The API surface documented here will not break within the 0.x minor series without a deprecation notice and a replacement in place. Expect breaking changes before 1.0. Test in staging before deploying to production. File bugs at the repo issue tracker — the project moves fast and early feedback shapes what ships.

## What Is Scolta?

Scolta is a scoring, ranking, and AI layer built on [Pagefind](https://pagefind.app/). Pagefind is the search engine: it builds a static inverted index at publish time, runs a browser-side WASM search engine, produces word-position data for phrase matching, and generates highlighted excerpts. Scolta's job begins where Pagefind's ends. It takes Pagefind's result set and re-ranks it using configurable boosts: title match weight, content match weight, recency decay curves, per-page priority overrides, and phrase-proximity multipliers built from Pagefind's word-position data. The final ranking is deterministic and reproducible — the same config produces the same scores in PHP, JavaScript, Go, or any other language.

The scoring runs entirely in the visitor's browser. The browser downloads a Pagefind index bundle (generated at build time) and a Scolta WASM module, then resolves queries locally without any server round-trip. No search server. No per-query API call. The LLM tier — query expansion, result summarization, follow-up question generation — is optional and separate. When enabled, it sends the query text and selected result snippets to a configured LLM provider (Anthropic, OpenAI, or a self-hosted Ollama endpoint). The base search tier shares nothing with any third party.

"Scolta" is archaic Italian for sentinel — someone watching for what matters. The name reflects the core job: stand between raw Pagefind output and the visitor, and surface the results worth seeing.

## Running Example

The examples in this README and the other Scolta repos use a recipe catalog as the concrete data set. Recipes are a good showcase because recipe vocabulary has genuine cross-dialect mismatches that basic keyword search handles poorly:

- A search for `aubergine parmesan` should surface *Eggplant Parmigiana* — the same dish, different word for the same vegetable.
- A search for `chinese noodle soup` should surface *Lanzhou Beef Noodles*, *Wonton Soup*, and *Dan Dan Noodles*.
- A search for `gluten free pasta` should surface *Zucchini Spaghetti with Pesto* and *Rice Noodle Stir-Fry*.
- A search for `quick dinner under 30 min` should surface recipes with short cook times.

The recipe fixture (20 HTML files with Pagefind-compatible markup) lives in [scolta-php](https://github.com/tag1consulting/scolta-php) at `tests/fixtures/recipes/`.

Here is what the raw WASM API looks like when used directly from JavaScript:

```javascript
import init, { score_results, version } from './pkg/scolta_core.js';

// Load the WASM module once, at page init
await init();
console.log(version()); // e.g. "0.3.2"

// Run a Pagefind query first
const pagefind = await import('/pagefind/pagefind.js');
await pagefind.init();
const raw = await pagefind.search('aubergine parmesan');
const rawResults = await Promise.all(raw.results.slice(0, 50).map(r => r.data()));

// Build a ScoringConfig — recipes don't have meaningful publish dates
const config = {
  title_match_boost: 1.5,
  title_all_terms_multiplier: 2.0,
  content_match_boost: 0.4,
  recency_strategy: 'none',
  language: 'en',
};

// Re-rank with Scolta
const scored = score_results(
  'aubergine parmesan',
  JSON.stringify(rawResults),
  JSON.stringify(config)
);
const results = JSON.parse(scored);

// results[0] is Eggplant Parmigiana
// Pagefind's stemmer matched "eggplant" from the body text where both terms appear.
// Scolta's title boost surfaced it above pages that mention aubergine only in passing.
console.log(results[0].url);          // "/recipes/eggplant-parmigiana"
console.log(results[0].meta.title);   // "Eggplant Parmigiana"
console.log(results[0].scolta_score); // e.g. 1.82
```

In practice, the platform adapters (WordPress, Drupal, Laravel) call `score_results` via `scolta.js`, which handles WASM loading and config serialization automatically. You only need the raw WASM API if you are building a custom front end or a new adapter.

## Installation

### WebAssembly build (for use in adapters)

```bash
cargo install wasm-pack   # one-time
wasm-pack build --target web --release
```

Output files:

```text
pkg/scolta_core_bg.wasm
pkg/scolta_core.js
pkg/scolta_core.d.ts
```

The platform adapters ship a pre-built copy of these files. Build from source only when modifying the core.

### Native build (for testing and development)

```bash
cargo build --release
cargo test
```

## Configuration and Quickstart

`score_results` takes a JSON-serialized `ScoringConfig`. The platform adapters serialize their config into this format automatically. If you are calling the WASM directly, pass a plain object:

**Recipe catalog** (no recency, title weight matters most):

```javascript
const config = {
  title_match_boost: 1.5,
  title_all_terms_multiplier: 2.0,
  content_match_boost: 0.4,
  recency_strategy: 'none',
  language: 'en',
};
```

**News site** (recent content ranks higher):

```javascript
const config = {
  title_match_boost: 1.0,
  content_match_boost: 0.4,
  recency_strategy: 'exponential',
  recency_boost_max: 0.8,
  recency_half_life_days: 30,
  language: 'en',
};
```

**Documentation site** (title precision matters, recency irrelevant):

```javascript
const config = {
  title_match_boost: 2.0,
  title_all_terms_multiplier: 2.5,
  content_match_boost: 0.4,
  recency_strategy: 'none',
  language: 'en',
};
```

See [scolta-php](https://github.com/tag1consulting/scolta-php) for the full config reference, including all scoring parameters and their defaults.

## What Scolta Replaces (and What It Doesn't)

Scolta is a practical replacement for hosted search SaaS (Algolia, Coveo, SearchStax) and for small-to-medium self-hosted search installations used purely as a search backend — Solr or Elasticsearch where your use case is content search, not log analytics or general data querying.

Scolta is not a replacement for:

- Full-text database search with row-level access control (per-document permissions enforced at query time).
- Log analytics or observability search built on Elasticsearch or OpenSearch.
- Vector databases used as a general retrieval layer for RAG pipelines (pgvector, Weaviate, Pinecone).
- Enterprise search with audit logging, retention policies, or SSO-gated document visibility.

If you need any of those, Scolta is the wrong tool. If you need fast, tunable, privacy-respecting search on a content site with an optional AI layer on top, Scolta is worth a look.

## Memory and Scale

The PHP indexer in [scolta-php](https://github.com/tag1consulting/scolta-php) runs on shared-host 128 MB `memory_limit` by default using the `conservative` profile. Scolta never silently auto-adjusts to a larger profile. Users on larger hosts can opt in to `balanced` or `aggressive` via the framework config page (which will suggest a profile based on the detected PHP memory limit but leaves the final choice to the admin), or via `--memory-budget=<profile|bytes>` at the CLI.

The trade-off: larger budget means fewer, larger chunks and faster indexing. Smaller budget means safer operation on constrained hosts.

Tested ceiling at the `conservative` profile: 50,000 pages. Higher page counts likely work; not certified yet.

The scoring WASM module has no significant memory overhead. It processes results in memory, but the input set is bounded by `max_pagefind_results` (default: 50 results per query).

## AI Features and Privacy

Scolta's AI tier (query expansion, result summarization, follow-up questions) is optional. When enabled:

- The LLM receives: the query text, and the titles and excerpts of the top N results (default: 5, configurable via `ai_summary_top_n`).
- The LLM does not receive: the full index contents, full page text, user session data, or visitor identity.
- Which provider receives the query data depends on your configuration: `anthropic`, `openai`, or a self-hosted endpoint via `ai_base_url`.

The base search tier — Pagefind index lookup and Scolta WASM scoring — shares nothing. It runs entirely in the visitor's browser, with no network calls beyond fetching the pre-built static index files.

## Verify It Works

```bash
cargo test                                        # unit + integration tests
cargo check --target wasm32-unknown-unknown       # WASM compilation check
cargo clippy -- -D warnings                       # lint
cargo fmt --check                                 # formatting
```

## Architecture

```text
┌─────────────────────────────────────────────────────────┐
│  Platform adapter (WordPress / Drupal / Laravel)        │
│  - Loads Pagefind WASM + Scolta WASM                    │
│  - Reads ScoringConfig                                  │
│  - Orchestrates AI calls (optional)                     │
└─────────────────────────────────────────────────────────┘
                │                           │
                ▼                           ▼
      ┌──────────────────┐       ┌──────────────────────┐
      │  Pagefind WASM   │       │  Scolta WASM (this)  │
      │  - Index lookup  │──────▶│  - Re-score          │
      │  - Excerpt       │       │  - Merge + dedupe    │
      │  - Positions     │       │  - Resolve prompts   │
      └──────────────────┘       │  - Parse AI output   │
                                 └──────────────────────┘
```

**Files in this crate:**

```text
browser.rs      WASM entry points (wasm_bindgen exports)
common.rs       Stop words, term extraction (single source of truth)
config.rs       ScoringConfig deserialization and validation
context.rs      Context extraction for LLM prompts
conversation.rs Conversation history trimming
error.rs        Typed error enum with function-name attribution
expansion.rs    AI expansion response parsing
lib.rs          Public API, describe() function catalog
prompts.rs      Prompt template resolution
sanitize.rs     PII sanitization
scoring.rs      Scoring algorithms and recency strategies
stop_words.rs   Language-specific stop word lists (30 languages)
```

**Browser WASM exports (9 functions):** `resolve_prompt`, `get_prompt`, `score_results`, `batch_score_results`, `merge_results`, `to_js_scoring_config`, `parse_expansion`, `version`, `describe`.

**Server-only functions (not in browser bundle):** `clean_html`, `build_pagefind_html` (produces HTML for Pagefind to index — does not index anything itself), `debug_call`.

`describe()` is the runtime function catalog. Platform adapters call it at startup to verify interface compatibility.

## Debugging

### "wasm-pack build fails with linker error"

The `wasm-bindgen` Cargo dependency and CLI tool versions must match exactly. Install the matching CLI:

```bash
cargo install --force wasm-bindgen-cli --version <VERSION_FROM_CARGO_TOML>
```

### "describe() is missing my new function"

Every new `#[wasm_bindgen]` export needs a corresponding entry in `describe()` in `browser.rs` with `since` and `stability` fields. CI runs a test named `describe_lists_all_functions` that fails if any export is missing.

### "Scoring results look wrong"

Run integration tests with `--nocapture` to see per-case output:

```bash
cargo test --test integration -- --nocapture
```

The server-only `debug_call()` wraps any function with timing and size logging.

### "Tests fail after changing stop words"

Stop word changes affect both this crate and the PHP indexer in scolta-php. Run both test suites after any stop word edit.

## Testing

```bash
cargo test                       # all unit tests
cargo clippy -- -D warnings      # lint
cargo fmt --check                # formatting
```

Adding a new public function requires:

- An inner function in `lib.rs`
- A `#[wasm_bindgen]` export in `browser.rs`
- An entry in `describe()` with `since` and `stability` fields
- Unit tests in the `#[cfg(test)]` block in `lib.rs`

## Built on Pagefind

**What Pagefind provides:**

- Static-file inverted index built at publish time
- Browser-side WASM search engine
- Word-position data for phrase and proximity matching
- Excerpt generation with match highlighting
- Stemming for 33+ languages
- Heading and anchor-level search
- Filter and sort support on indexed metadata

Scolta does not reimplement any of the above. A Scolta-powered search begins with a Pagefind query; Scolta only runs after Pagefind returns results.

If you only need search without the scoring customization or AI features, use Pagefind directly — it stands on its own.

## Credits

Scolta is built on [Pagefind](https://pagefind.app/) by [CloudCannon](https://cloudcannon.com/). Without Pagefind, Scolta has no search to score — the index format, WASM search engine, word-position data, and excerpt generation are all Pagefind's. Scolta's contribution is the layer that sits on top: configurable scoring, multi-adapter ranking parity, AI features, and platform glue.

## License

MIT

## Related Packages

- [scolta-php](https://github.com/tag1consulting/scolta-php) — PHP library that indexes content into Pagefind-compatible indexes, plus the shared orchestration, memory-budget management, and AI client used by all CMS adapters.
- [scolta-drupal](https://github.com/tag1consulting/scolta-drupal) — Drupal 10/11 Search API backend with Drush commands, admin settings form, and a search block.
- [scolta-laravel](https://github.com/tag1consulting/scolta-laravel) — Laravel 11/12 package with Artisan commands, a `Searchable` trait for Eloquent models, and a Blade search component.
- [scolta-wp](https://github.com/tag1consulting/scolta-wp) — WordPress 6.x plugin with WP-CLI commands, Settings API page, and a `[scolta_search]` shortcode.
