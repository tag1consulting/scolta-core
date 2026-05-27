# Scolta Core - Implementation Summary

Rust WebAssembly crate for the Scolta search engine core. Provides client-side search scoring, prompt management, query expansion, context extraction, PII sanitization, and conversation trimming via wasm-bindgen.

## Project Structure

```
scolta-core/
├── Cargo.toml              # Package manifest (edition 2021, wasm-bindgen 0.2)
├── rustfmt.toml            # Formatting config (edition 2021)
├── README.md               # Usage and build instructions
├── API.md                  # Complete API reference
├── IMPLEMENTATION.md       # This file
├── VERSIONING.md           # Version policy and function lifecycle
├── CHANGELOG.md            # Release history
├── src/
│   ├── lib.rs              # inner module + orchestration (plain Rust, tested without WASM)
│   ├── browser.rs          # wasm-bindgen exports (thin wrappers over inner::)
│   ├── common.rs           # Language-aware stop words, term extraction, validation
│   ├── config.rs           # ScoringConfig parsing and validation
│   ├── context.rs          # LLM context extraction (intro + keyword-anchored snippets)
│   ├── conversation.rs     # Conversation history trimming
│   ├── error.rs            # Typed error enum (ScoltaError) with function attribution
│   ├── expansion.rs        # LLM response parsing (JSON, markdown, fallback)
│   ├── prompts.rs          # Prompt templates and resolution
│   ├── sanitize.rs         # PII redaction (email, phone, SSN, credit card, IP)
│   ├── scoring.rs          # Search scoring algorithm with recency/relevance/phrase
│   └── stop_words.rs       # Language-specific stop word lists (30 languages)
└── tests/
    └── integration.rs      # Integration tests
```

## File-by-File Implementation

### Cargo.toml
- Package name: `scolta-core`
- Version: `1.0.0-rc4`
- Edition: `2021`
- Library type: `cdylib` + `rlib` (WASM module + library)
- Dependencies:
  - `wasm-bindgen = "0.2"` — WASM/JavaScript interop
  - `js-sys = "0.3"` — JavaScript type bindings
  - `serde = "1"` — Serialization with derive support
  - `serde_json = "1"` — JSON support
  - `regex = "1"` — Pattern matching for PII redaction
- Release profile: `opt-level = "s"`, LTO, symbol stripping, `codegen-units = 1`, `panic = "abort"`

### src/browser.rs
13 browser WASM exports via `#[wasm_bindgen]` — thin serialization wrappers over `inner::` functions. Delegates all logic to `inner::`. Each function parses a JSON string input, calls the corresponding `inner::` function, and serializes the result back to a JSON string (or propagates errors as `JsError`).

Exports: `score_results`, `merge_results`, `match_priority_pages`, `parse_expansion`, `batch_score_results`, `resolve_prompt`, `get_prompt`, `extract_context`, `batch_extract_context`, `sanitize_query`, `truncate_conversation`, `version`, `describe`.

### src/lib.rs
`inner` module with plain Rust implementations, callable from browser exports and unit tests. All inner functions return `Result<T, ScoltaError>` (typed errors with function attribution). Also contains the `describe()` function catalog and `WASM_INTERFACE_VERSION`.

### src/common.rs
Language-aware stop word filtering, term validation, and term extraction. Both `scoring` and `expansion` import from here.

Key types and functions:
- `is_stop_word(term, language)` — case-insensitive check against language-specific list
- `is_stop_word_with_custom(term, language, custom)` — also checks custom stop words
- `is_valid_term(term, language)` — filters empty, short, numeric, and stop words
- `extract_terms(query, language)` — split, lowercase, filter
- `extract_query(query, language)` — returns `QueryInfo { terms, is_phrase, forced_phrase }`

### src/config.rs
Configuration parsing and validation.

- `from_json(json)` — parse JSON into `ScoringConfig`, missing fields use defaults
- `from_json_validated(json)` — same as `from_json` but clamps out-of-range values and returns warnings

### src/context.rs
LLM context extraction. Combines a fixed-length intro with keyword-anchored snippets, merges overlapping ranges, and truncates at sentence boundaries.

- `ContextConfig` — `max_length` (6000), `intro_length` (2000), `snippet_radius` (500), `separator`
- `extract_context(content, query, config)` — single document extraction
- `batch_extract_context(items, query, config)` — multi-document extraction

### src/conversation.rs
Conversation history trimming for multi-turn AI interactions. Removes the oldest message pairs when total length exceeds a limit. Always preserves the first N messages (system prompt, initial context).

- `ConversationConfig` — `max_length` (12000), `preserve_first_n` (2), `removal_unit` (2)
- `truncate_conversation(messages, config)` — returns trimmed message array

### src/error.rs
Typed error enum. Every variant includes the originating function name for developer-friendly diagnostics.

Variants:
- `InvalidJson { function, detail }` — input not valid JSON
- `MissingField { function, field }` — required field absent
- `InvalidFieldType { function, field, expected }` — wrong type
- `UnknownPrompt { name }` — bad prompt template name
- `ParseError { function, detail }` — processing failure

Implements `Display`, `Error`, and convenience constructors.

### src/expansion.rs
Parse and process LLM expansion responses. Handles three input formats with a fallback chain, filters results through language-aware stop word lists, applies generic-term filtering, and merges with existing term sets.

- `ExpansionConfig` — language, generic_terms, filter_single_word_generic, keep_acronyms, keep_proper_nouns, min_term_length, existing_terms
- `parse_expansion(input)` — bare string parser
- `parse_expansion_with_language(input, language)` — language-aware parsing
- `parse_expansion_with_config(text, config)` — full configuration

### src/prompts.rs
Three canonical prompt templates with variable substitution:

- `EXPAND_QUERY` — expands user queries into 2-4 alternative terms
- `SUMMARIZE` — generates summaries from search result excerpts
- `FOLLOW_UP` — handles follow-up questions in conversations

Templates use `{SITE_NAME}`, `{SITE_DESCRIPTION}`, and optionally `{DYNAMIC_ANCHORS}` placeholders.

### src/sanitize.rs
PII redaction for query analytics. Removes email, phone, SSN, credit card, and IP patterns using compiled regexes (cached via `OnceLock`). Supports custom patterns.

- `SanitizationConfig` — boolean toggles per pattern type, plus custom patterns
- `sanitize_query(query, config)` — returns redacted string

### src/scoring.rs
Search result scoring and ranking — the canonical Scolta ranking algorithm.

#### Scoring Formula
```
final_score = (base_score * source_weight) + title_boost + content_boost + recency_boost + priority_boost
```

Where:
- `base_score` — upstream search engine score (from Pagefind), default 1.0
- `source_weight` — dampening factor for secondary sources, default 1.0
- `title_boost` — `title_match_boost` when any query term matches title; multiplied by `title_all_terms_multiplier` when ALL terms match
- `content_boost` — `content_match_boost` when any term matches excerpt; multiplied by `content_all_terms_multiplier` when ALL terms match; further multiplied by phrase proximity (adjacent: `phrase_adjacent_multiplier`, near: `phrase_near_multiplier`)
- `recency_boost` — decay function based on `recency_strategy` (exponential, linear, step, none, custom)
- `priority_boost` — added when result URL matches a priority page and query contains keywords

#### ScoringConfig Struct
```rust
pub struct ScoringConfig {
    pub recency_boost_max: f64,              // 0.5
    pub recency_half_life_days: u32,         // 365
    pub recency_penalty_after_days: u32,     // 1825
    pub recency_max_penalty: f64,            // 0.3
    pub recency_strategy: String,            // "exponential"
    pub recency_curve: Vec<[f64; 2]>,        // []
    pub title_match_boost: f64,              // 1.0
    pub title_all_terms_multiplier: f64,     // 1.5
    pub content_match_boost: f64,            // 0.4
    pub content_all_terms_multiplier: f64,   // 1.2
    pub phrase_adjacent_multiplier: f64,     // 2.5
    pub phrase_near_multiplier: f64,         // 1.5
    pub phrase_near_window: u32,             // 5
    pub phrase_window: u32,                  // 15
    pub excerpt_length: u32,                 // 300
    pub results_per_page: u32,              // 10
    pub max_pagefind_results: u32,          // 50
    pub language: String,                    // "en"
    pub custom_stop_words: Vec<String>,      // []
    pub priority_pages: Vec<PriorityPage>,   // []
}
```

Also provides: `merge_results` (N-set weighted merge with deduplication), `match_priority_pages` (keyword-URL matching), `apply_sort_override` (metadata field sorting), and `ConfigWarning` for out-of-range value reporting.

### src/stop_words.rs
Static stop word arrays for 30 languages (ISO 639-1 codes). All entries are lowercase. CJK languages (zh, ja, ko) and unknown codes return an empty slice.

- `get_stop_words(language)` — single entry point for stop word lookup

### tests/integration.rs
Integration tests covering all 13 exported functions through the `inner::` API surface. Tests verify: describe manifest completeness, score ranking correctness, priority page boost, sort overrides, merge deduplication, old format rejection, priority page matching, expansion filtering/merging, context extraction, sanitization, conversation truncation, batch scoring, prompt resolution, and version output.

## Building and Testing

### Native Build
```bash
cargo build --release
cargo test
```

### WebAssembly Build
```bash
wasm-pack build --target web --release
```

Output: `pkg/scolta_core_bg.wasm`, `pkg/scolta_core.js`, `pkg/scolta_core.d.ts`

### Formatting
```bash
cargo fmt
```

Uses default Rust formatting (edition 2021).

## Key Design Decisions

1. **No unwrap() outside tests** — all error handling uses Result/Option
2. **Stop word filtering** — consistent across title, content, and expansion parsing; language-aware
3. **Serde pass-through** — SearchResult uses `#[serde(flatten)]` for extensibility
4. **Default config** — ScoringConfig implements Default for convenience
5. **Module structure** — clear separation of concerns with dedicated modules
6. **Doc comments** — every public function and struct documented
7. **Test coverage** — all major functions have unit and integration tests
8. **WASM compatibility** — no system time assumptions
9. **OnceLock regex caching** — compiled regex patterns cached for PII redaction
10. **Content all-terms multiplier is multiplicative** — `content_match_boost * content_all_terms_multiplier` (not assignment)

## Dependencies

- **wasm-bindgen 0.2** — WASM/JavaScript interop
- **js-sys 0.3** — JavaScript type bindings
- **serde 1** — serialization
- **serde_json 1** — JSON
- **regex 1** — PII pattern matching

Typical WASM binary size: under 500 KB (after strip and LTO)

## Compatibility

- **Rust**: Edition 2021
- **Target**: `wasm32-unknown-unknown` (via wasm-pack)
- **JavaScript**: ES2020+ (for ES module imports)

## License

MIT
