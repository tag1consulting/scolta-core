# Scolta Core

[![CI](https://github.com/tag1consulting/scolta-core/actions/workflows/ci.yml/badge.svg)](https://github.com/tag1consulting/scolta-core/actions/workflows/ci.yml)

Scolta is a browser-side search engine: the index lives in static files, scoring runs in the browser via WebAssembly, and an optional AI layer handles query expansion and summarization. No search server required. "Scolta" is archaic Italian for sentinel — someone watching for what matters.

This crate is the WebAssembly core. It compiles to a browser WASM module that platform adapters (WordPress, Drupal, Laravel) load to run scoring, prompt resolution, and result merging client-side.

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
cargo check --target wasm32-wasip1  # confirm WASM compilation
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
browser.rs      #[wasm_bindgen] exports — thin wrappers over lib.rs inner functions
lib.rs          Inner functions (testable API surface)
scoring.rs      Search result scoring (recency decay, title/content boost, result merging)
expansion.rs    LLM expansion response parsing (JSON, markdown, plaintext fallback)
prompts.rs      Prompt template constants and variable resolution
html.rs         HTML cleaning and Pagefind-compatible HTML generation (build-time only)
config.rs       ScoringConfig parsing, validation, JS export
common.rs       Stop words, term extraction (single source of truth)
debug.rs        Performance measurement utilities (server-side only)
error.rs        Typed error enum with function-name attribution
```

**Browser WASM exports (8 functions):** `resolve_prompt`, `get_prompt`, `score_results`, `merge_results`, `to_js_scoring_config`, `parse_expansion`, `version`, `describe`.

**Server-only functions (not in browser bundle):** `clean_html`, `build_pagefind_html`, `debug_call`.

`describe()` is the runtime function catalog — the authoritative list of what this module exports. Platform adapters call `describe()` at startup to verify interface compatibility.

## Testing

```bash
cargo test                       # all tests
cargo test --test integration    # integration tests only
```

Test fixtures in `tests/fixtures/`:
- `drupal-page.html` — sample Drupal page
- `wordpress-post.html` — sample WordPress post
- `expected-clean.txt` — expected output after HTML cleaning

Adding a new public function requires:

- An inner function in `lib.rs`
- A `#[wasm_bindgen]` export in `browser.rs`
- An entry in `describe()` with `since` and `stability` fields
- A test in `tests/integration.rs`

## License

MIT
