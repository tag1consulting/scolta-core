# Scolta Core

<!-- TODO: Add CI badge once repo is on GitHub -->
<!-- [![CI](https://github.com/tag1consulting/scolta-core/actions/workflows/ci.yml/badge.svg)](https://github.com/tag1consulting/scolta-core/actions/workflows/ci.yml) -->

WebAssembly module for the Scolta search engine, providing cross-platform search scoring, prompt management, and content processing.

## Overview

This crate compiles to WebAssembly via Extism PDK and exports 11 plugin functions:

### Plugin Functions

#### Prompt Management
- `resolve_prompt(json) -> string` - Resolve a prompt template with site-specific details
- `get_prompt(string) -> string` - Get raw prompt template by name (plain string input, not JSON)

#### HTML Processing
- `clean_html(json) -> string` - Strip page chrome and extract main content
- `build_pagefind_html(json) -> string` - Generate Pagefind-compatible HTML

#### Search Scoring
- `score_results(json) -> json` - Score and re-rank search results
- `merge_results(json) -> json` - Merge original + expanded results with dedup
- `to_js_scoring_config(json) -> json` - Export scoring config for JavaScript

#### Utilities
- `parse_expansion(string) -> json` - Parse LLM expansion response into terms
- `version() -> string` - Get crate version
- `describe() -> json` - Self-documenting function catalog for runtime discovery
- `debug_call(json) -> json` - Profile any function with timing/size metrics

## Building

### Native Build (for testing)
```bash
cargo build --release
cargo test
```

### WebAssembly Build
```bash
cargo build --target wasm32-wasip1 --release
```

The compiled module is located at:
```
target/wasm32-wasip1/release/scolta_core.wasm
```

## Module Structure

- **common.rs** - Shared stop words, term extraction, validation (single source of truth)
- **error.rs** - Typed error enum with function-name attribution
- **prompts.rs** - Prompt template constants and resolution
- **html.rs** - HTML cleaning and Pagefind HTML generation (OnceLock-cached regex)
- **scoring.rs** - Search result scoring algorithm with recency and relevance factors
- **config.rs** - Configuration parsing, validation, and JS export
- **expansion.rs** - LLM response parsing (JSON, markdown, fallback)
- **debug.rs** - Performance measurement utilities
- **lib.rs** - Extism plugin entry points, inner functions, and orchestration

## Key Algorithms

### Recency Scoring
Exponential decay with configurable half-life:
- Recent content (< half_life_days): positive boost
- Old content (> penalty_after_days): negative penalty
- Smooth interpolation between regions

### Relevance Scoring
Composite score combining:
- Title match (all terms → higher multiplier)
- Content match (excerpt/body search)
- Recency factor (date-based decay)

### Result Merging
- Deduplicates by URL
- Weights original vs expanded results separately
- Re-scores and re-ranks final set

## Configuration

Default ScoringConfig:
```rust
{
    recency_boost_max: 0.5,
    recency_half_life_days: 365,
    recency_penalty_after_days: 1825,
    recency_max_penalty: 0.3,
    title_match_boost: 1.0,
    title_all_terms_multiplier: 1.5,
    content_match_boost: 0.4,
    content_all_terms_multiplier: 0.48,
    expand_primary_weight: 0.7,
    excerpt_length: 300,
    results_per_page: 10,
    max_pagefind_results: 50,
}
```

## Testing

Run all tests:
```bash
cargo test
```

Run integration tests:
```bash
cargo test --test integration
```

Test fixtures are in `tests/fixtures/`:
- `drupal-page.html` - Sample Drupal page with typical structure
- `wordpress-post.html` - Sample WordPress post
- `expected-clean.txt` - Documentation of expected cleaned output

## Dependencies

- **extism-pdk** (1.2) - Extism plugin development kit
- **serde** (1.0) - Serialization framework
- **serde_json** (1.0) - JSON support
- **regex** (1.0) - HTML tag stripping and pattern matching

## License

MIT
