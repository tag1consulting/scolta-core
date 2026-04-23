# Scolta Core

[![CI](https://github.com/tag1consulting/scolta-core/actions/workflows/ci.yml/badge.svg)](https://github.com/tag1consulting/scolta-core/actions/workflows/ci.yml)

Scolta is a scoring, ranking, and AI layer built on [Pagefind](https://pagefind.app/). Pagefind is the search engine — it builds the static index, runs the browser-side WASM search, produces word-position data, and generates excerpts. Scolta takes Pagefind's results and re-ranks them with configurable title/content/recency/priority boosts, then optionally passes them through an AI layer for query expansion, summarization, and follow-up generation. No search server required. "Scolta" is archaic Italian for sentinel — someone watching for what matters.

This crate is the WebAssembly core. It compiles to a browser WASM module that platform adapters (WordPress, Drupal, Laravel) load to run scoring, prompt resolution, and result merging client-side.

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

**What Scolta adds on top:**

- **Configurable scoring layer.** Title match boost, content match boost, recency decay (exponential/linear/step/custom curves), priority-page boosts, phrase-adjacency and phrase-proximity multipliers using Pagefind's word-position data, per-source weighting, and deterministic cross-adapter ranking (PHP, Python, JS, Go all produce identical final scores from the same config).
- **AI integration.** Query expansion (LLM rewrites the query into related terms), result summarization, follow-up question generation, prompt template resolution, conversation-history trimming, and PII sanitization on inputs.
- **30-language stop-word lists** shared between the WASM scorer and the PHP indexer, so indexing and scoring agree on what a stop word is.
- **A PHP indexer** (`scolta-php`) as an optional alternative to the Pagefind CLI for hosts where running the Pagefind binary isn't practical — it produces a Pagefind-compatible index.
- **Platform adapters** for WordPress, Drupal, and Laravel that orchestrate the Pagefind + Scolta combination: config management, admin UI, prompt editing, and per-site customization.

If you only need search without the scoring customization or AI features, use Pagefind directly — it stands on its own. Reach for Scolta when you want Pagefind's search with custom ranking logic, or when you want to drop an AI layer on top of it.

## Quick Install

### Native build (for testing)

```bash
cargo build --release
cargo test
```

### WebAssembly build

```bash
cargo install wasm-pack   # one-time
wasm-pack build --target web --release
```

Output files used by platform adapters:

```text
pkg/scolta_core_bg.wasm
pkg/scolta_core.js
pkg/scolta_core.d.ts
```

The platform adapters ship a pre-built copy of these files. Build from source only if you are modifying the core.

## Verify It Works

```bash
cargo test                          # unit + integration tests
cargo check --target wasm32-unknown-unknown  # confirm WASM compilation
```

All tests pass with no native runtime dependencies.

## Optional Upgrades

**Updating wasm-bindgen:** The `wasm-bindgen` crate version in `Cargo.toml` and the installed `wasm-bindgen-cli` version must match exactly. Upgrade both together.

**Updating language stop words:** Edit the `STOP_WORDS_*` constants in `common.rs`. These are the single source of truth shared by the WASM scorer and the PHP indexer in scolta-php.

## Debugging

### "wasm-pack build fails with linker error"

The `wasm-bindgen` Cargo dependency and CLI tool versions must match. Install the matching CLI:

```bash
cargo install --force wasm-bindgen-cli --version <VERSION_FROM_CARGO_TOML>
```

### "describe() is missing my new function"

Every new `#[wasm_bindgen]` export must have a corresponding entry in `describe()` in `browser.rs` with `since` and `stability` fields. CI runs a test named `describe_lists_all_functions` that will fail if any export is missing.

### "Scoring results look wrong"

Run integration tests with `--nocapture` to see per-case output:

```bash
cargo test --test integration -- --nocapture
```

The server-side `debug_call()` function wraps any function with timing and size logging.

### "Tests fail after changing stop words"

Stop word changes affect both this crate and the PHP indexer in scolta-php. Run both test suites.

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

**Server-only functions (not in browser bundle):** `clean_html`, `build_pagefind_html` (produces HTML that Pagefind then indexes — does not index anything itself), `debug_call`.

`describe()` is the runtime function catalog — the authoritative list of what this module exports. Platform adapters call `describe()` at startup to verify interface compatibility.

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

## Credits

Scolta is built on [Pagefind](https://pagefind.app/) by [CloudCannon](https://cloudcannon.com/). Without Pagefind, Scolta has no search to score — the index format, WASM search engine, word-position data, and excerpt generation are all Pagefind's. Scolta's contribution is the layer that sits on top: configurable scoring, multi-adapter ranking parity, AI features, and platform glue.

## License

MIT
