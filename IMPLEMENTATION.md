# Scolta Core - Implementation Summary

Complete Rust WebAssembly crate for the Scolta search engine core, replacing PHP scolta-php library logic with cross-platform WASM module via Extism PDK.

## Project Structure

```
scolta-core/
├── Cargo.toml              # Package manifest (edition 2021, extism-pdk 1.2)
├── rustfmt.toml            # Formatting config (edition 2021)
├── README.md               # Usage and build instructions
├── API.md                  # Complete API reference with language-specific guides
├── IMPLEMENTATION.md       # This file
├── src/
│   ├── lib.rs              # 11 Extism plugin functions + inner module + orchestration
│   ├── common.rs           # Shared stop words, term extraction, validation
│   ├── error.rs            # Typed error enum (ScoltaError) with function attribution
│   ├── prompts.rs          # Prompt templates and resolution
│   ├── html.rs             # HTML processing (clean_html, build_pagefind_html)
│   ├── scoring.rs          # Search scoring algorithm with recency/relevance
│   ├── config.rs           # Configuration parsing, validation, and JS export
│   ├── expansion.rs        # LLM response parsing (JSON, markdown, fallback)
│   └── debug.rs            # Performance measurement and logging
└── tests/
    ├── integration.rs      # Comprehensive integration tests
    └── fixtures/
        ├── drupal-page.html
        ├── wordpress-post.html
        └── expected-clean.txt
```

## File-by-File Implementation

### Cargo.toml
- Package name: `scolta-core`
- Version: `0.1.0`
- Edition: `2021`
- Library type: `cdylib` (WebAssembly module)
- Dependencies:
  - `extism-pdk = "1.2"` - Extism plugin framework
  - `serde = "1"` - Serialization with derive support
  - `serde_json = "1"` - JSON support
  - `regex = "1"` - Pattern matching for HTML processing
- Release profile: Optimized for size and strip symbols

### rustfmt.toml
Minimal formatting enforcement:
- Edition: `2021`
- Uses cargo fmt defaults for all other settings

### src/lib.rs
Core plugin entry point exporting 11 Extism functions via `#[plugin_fn]`, with an `inner` module containing plain Rust equivalents for testing and `debug_call`. All inner functions return `Result<T, ScoltaError>` (typed errors with function attribution).

#### Plugin Functions (Extism-compatible)
1. `resolve_prompt(json) -> string`
   - Input: `{prompt_name, site_name, site_description}`
   - Returns: Resolved template with placeholders replaced

2. `get_prompt(string) -> string`
   - Input: Plain string prompt name (NOT JSON)
   - Returns: Raw template string with placeholders

3. `clean_html(json) -> string`
   - Input: `{html, title}`
   - Returns: Cleaned plain text

4. `build_pagefind_html(json) -> string`
   - Input: `{id, title, body, url, date, site_name}`
   - Returns: Complete HTML with data-pagefind-* attributes

5. `to_js_scoring_config(json) -> json`
   - Input: `{scoring config fields + AI toggle flags}`
   - Returns: JavaScript-friendly config object with SCREAMING_SNAKE_CASE keys

6. `score_results(json) -> json`
   - Input: `{query, results[], config}`
   - Returns: Scored and sorted results

7. `merge_results(json) -> json`
   - Input: `{original[], expanded[], config}`
   - Returns: Merged, deduplicated results

8. `parse_expansion(string) -> json`
   - Input: LLM response text
   - Returns: `[term1, term2, ...]` array

9. `version() -> string`
   - Returns: Crate version `"0.1.0"`

10. `describe() -> json`
    - Returns: Machine-readable catalog of all exported functions

11. `debug_call(json) -> json`
    - Input: `{function: string, input: string}`
    - Wraps any plugin function with timing/size metrics
    - Output: `{output, error, time_us, input_size, output_size}`
    - `output` is null on error; `error` is null on success

### src/common.rs
Shared constants and utilities — single source of truth for stop words and term validation. Both `scoring` and `expansion` import from here.

#### Public API
```rust
pub const STOP_WORDS: &[&str]                     // ~60 common English stop words
pub fn is_stop_word(term: &str) -> bool            // Case-insensitive check
pub fn is_valid_term(term: &str) -> bool           // Filters empty, short, numeric, stop words
pub fn extract_terms(query: &str) -> Vec<String>   // Split + lowercase + filter
```

### src/error.rs
Typed error enum replacing `Result<T, String>`. Every variant includes the originating function name for log-friendly diagnostics.

#### ScoltaError Variants
- `InvalidJson { function, detail }` — Input not valid JSON
- `MissingField { function, field }` — Required field absent
- `InvalidFieldType { function, field, expected }` — Wrong type
- `UnknownPrompt { name }` — Bad prompt template name
- `UnknownFunction { name }` — Bad debug_call function name
- `ParseError { function, detail }` — Processing failure
- `ConfigWarning { field, message }` — Out-of-range config value

Implements `Display` (human-readable), `Error`, and convenience constructors (`invalid_json()`, `missing_field()`, `parse_error()`).

### src/prompts.rs
Manages three canonical prompt templates:

#### Constants
- `EXPAND_QUERY` (558 words) - Expands user queries into 2-4 alternative terms
- `SUMMARIZE` (485 words) - Generates concise, scannable summaries
- `FOLLOW_UP` (380 words) - Handles follow-up questions in conversations

#### Key Rules Enforced
- Extract core topic, ignore question words
- Keep multi-word terms together
- Filter common stop words (the, is, of, etc.)
- Never return overly generic terms (services, information, resources)
- For person queries: only name variations, no job titles
- Return pure JSON with no markdown wrapping

#### Public API
```rust
pub fn get_template(name: &str) -> Option<&'static str>
pub fn resolve_template(name: &str, site_name: &str, site_description: &str) -> Option<String>
```

Unit tests verify:
- All three templates load correctly
- Template resolution replaces placeholders
- Invalid names return None

### src/html.rs
HTML processing with two main functions:

#### clean_html(html: &str, title: &str) -> String
Pipeline:
1. **Extract main content** - Look for `id="main-content"`, fall back to body
2. **Remove footer** - Strip footer tags, footer IDs/classes, region-footer
3. **Remove chrome** - Delete script, style, nav elements and contents
4. **Strip tags** - Remove all HTML tags via regex
5. **Normalize whitespace** - Collapse multiple spaces/newlines to single space
6. **Remove leading title** - Prevent duplication in index

#### build_pagefind_html(...) -> String
Generates minimal HTML document:
- Includes `data-pagefind-body` attribute
- Adds `data-pagefind-meta="url:..."` for URL
- Adds `data-pagefind-meta="date:..."` for date
- Adds `data-pagefind-filter="site:..."` for filtering
- Escapes all HTML entities in fields

#### Implementation Notes
- Uses regex for tag matching (with Result-based error handling)
- Case-insensitive matching for footer detection
- HTML entity escaping for special characters (&, <, >, ", ')
- All regex patterns wrapped in Result::Ok handling for robustness

Unit tests cover:
- Script/style/nav removal
- Whitespace normalization
- Tag stripping
- HTML escaping in output
- Pagefind metadata inclusion

### src/scoring.rs
Search result scoring with four main components:

#### ScoringConfig Struct
```rust
pub struct ScoringConfig {
    recency_boost_max: f64,              // 0.5
    recency_half_life_days: u32,         // 365
    recency_penalty_after_days: u32,     // 1825
    recency_max_penalty: f64,            // 0.3
    title_match_boost: f64,              // 1.0
    title_all_terms_multiplier: f64,     // 1.5
    content_match_boost: f64,            // 0.4
    content_all_terms_multiplier: f64,   // 0.48
    expand_primary_weight: f64,          // 0.7
    excerpt_length: u32,                 // 300
    results_per_page: u32,               // 10
    max_pagefind_results: u32,           // 50
}
```

Includes `validate()` method that returns `Vec<ConfigWarning>` for out-of-range values.

#### SearchResult Struct
Includes:
- url, title, excerpt, date (required)
- score (computed), content_type, site_name
- extra: Flat serde_json::Map for pass-through fields

#### Scoring Algorithms

**Recency Factor** `days_since_date() -> f64`
- Exponential decay with half-life
- Recent (< half_life_days): Positive boost up to `recency_boost_max`
- Old (> penalty_after_days): Negative penalty down to `-recency_max_penalty`
- Middle range: Linear interpolation between boost and penalty zones

**Title Match** `title_match_score() -> f64`
- Query terms extracted (stop words filtered)
- All terms in title: `title_all_terms_multiplier` (1.5)
- Any term in title: `title_match_boost` (1.0)
- No terms: 0.0

**Content Match** `content_match_score() -> f64`
- Same logic as title match but with content-specific multipliers
- All terms: `content_all_terms_multiplier` (0.48)
- Any term: `content_match_boost` (0.4)
- No terms: 0.0

**Composite Score** `score_result() -> f64`
```
base_score * (1.0 + title_score) * recency_multiplier * (1.0 + content_score)
```

**Result Merging** `merge_results() -> Vec<SearchResult>`
- Deduplicates by URL using HashMap
- Original results weighted by `expand_primary_weight` (0.7)
- Expanded results weighted by `1.0 - expand_primary_weight` (0.3)
- Combines scores for duplicates
- Final results sorted by combined score (descending)

#### Extract Terms Helper
Filters:
- Stop words (common English words, question words)
- Terms shorter than 2 characters
- Returns lowercase versions

Unit tests verify:
- Recency calculation for recent/old/future dates
- Title match scoring with full/partial/no matches
- Content match scoring
- Composite score calculation
- Result sorting by score
- Deduplication in merge

### src/config.rs
Configuration management for JavaScript integration:

#### to_js_scoring_config(config: &ScoringConfig) -> serde_json::Value
Exports config as JSON with uppercase keys:
```json
{
  "RECENCY_BOOST_MAX": 0.5,
  "RECENCY_HALF_LIFE_DAYS": 365,
  "RECENCY_PENALTY_AFTER_DAYS": 1825,
  "RECENCY_MAX_PENALTY": 0.3,
  "TITLE_MATCH_BOOST": 1.0,
  "TITLE_ALL_TERMS_MULTIPLIER": 1.5,
  "CONTENT_MATCH_BOOST": 0.4,
  "EXCERPT_LENGTH": 300,
  "RESULTS_PER_PAGE": 10,
  "MAX_PAGEFIND_RESULTS": 50,
  "AI_EXPAND_QUERY": true,
  "AI_SUMMARIZE": true,
  "AI_SUMMARY_TOP_N": 5,
  "AI_SUMMARY_MAX_CHARS": 2000,
  "EXPAND_PRIMARY_WEIGHT": 0.7,
  "AI_MAX_FOLLOWUPS": 3
}
```

#### from_json(json: &serde_json::Value) -> ScoringConfig
- Parses JSON object
- Falls back to defaults for missing fields
- Handles non-object input gracefully

#### from_json_validated(json: &serde_json::Value) -> (ScoringConfig, Vec<ConfigWarning>)
- Same as `from_json` but also runs `validate()` on the result
- Returns warnings for any out-of-range values

Unit tests verify:
- Correct field names in JS export
- Custom values preserved
- Defaults applied for missing values

### src/expansion.rs
Parse LLM expansion responses in multiple formats:

#### parse_expansion(response: &str) -> Vec<String>
Handles three input formats in priority order:

1. **JSON Array** (primary)
   - `["term1", "term2"]`
   - Direct serde_json parse

2. **Markdown-Wrapped JSON** (secondary)
   - ` ```json ["terms"] ``` `
   - ` ``` ["terms"] ``` `
   - Extracts JSON, then parses

3. **Fallback Parsing** (tertiary)
   - Split by newlines and commas
   - Strip quotes and whitespace
   - No JSON parsing required

#### Filtering
Removes:
- Empty strings
- Strings < 2 characters
- Pure numbers
- Common stop words (the, a, an, etc.)

Unit tests cover:
- JSON parsing
- Markdown extraction (json variant and generic)
- Fallback parsing (newlines, commas, mixed)
- Stop word filtering
- Number filtering
- Quote stripping

### src/debug.rs
Performance measurement and logging:

#### DebugResult Struct
```rust
pub struct DebugResult {
    pub output: Option<String>,   // None on error
    pub error: Option<String>,    // None on success
    pub time_us: u128,
    pub input_size: usize,
    pub output_size: usize,       // 0 on error
}
```

#### measure_call<F>() -> DebugResult
- Closure signature: `FnOnce() -> Result<String, String>`
- Records function execution time (microseconds)
- Tracks input/output sizes in bytes
- Non-blocking timing using std::time::Instant
- Prints to stderr in non-WASM environments (gated by `#[cfg(not(target_arch = "wasm32"))]`)

#### debug_result_to_json() -> serde_json::Value
Formats results as JSON with separate `output` and `error` fields

Unit tests verify:
- Timing accuracy
- Size calculation
- JSON formatting

### tests/integration.rs
Comprehensive integration tests covering:

#### Prompt Templates
- All three templates load correctly
- Resolution replaces placeholders
- Invalid names return None

#### HTML Processing
- Script/style/nav removal
- Whitespace normalization
- Tag stripping
- HTML escaping in output
- Pagefind metadata inclusion (url, date, filter)

#### Scoring
- Recency factor (recent/old content)
- Title match scoring (all/partial/no matches)
- Content match scoring
- Composite score calculation
- Result sorting by score descending
- Deduplication in merge (same URL combined)

#### Config Export
- Correct field names (uppercase)
- Correct default values
- Custom config preservation

#### Expansion Parsing
- JSON array parsing
- Markdown-wrapped JSON
- Fallback newline/comma parsing
- Stop word filtering
- Number filtering

#### Full Pipeline
- Template resolution → HTML cleaning → Pagefind HTML → Config export → Result scoring

### Test Fixtures

#### drupal-page.html
- Typical Drupal page structure
- Main content in `id="main-content"`
- Navigation navbar
- Footer with class="footer"
- Inline script and style tags

#### wordpress-post.html
- Typical WordPress post structure
- Main content in `id="main-content"`
- Post metadata
- Footer with `id="footer"`
- Multiple script tags

#### expected-clean.txt
Documentation of expected cleaned output for both fixtures

## Building and Testing

### Native Build
```bash
cd packages/scolta-core
cargo build --release
cargo test --release
```

### WebAssembly Build
```bash
cargo build --target wasm32-wasip1 --release
```

Output: `target/wasm32-wasip1/release/scolta_core.wasm`

### Formatting
```bash
cargo fmt
```

Uses default Rust formatting (4 spaces, 100 char line limit where possible)

## Key Design Decisions

1. **No unwrap() outside tests** - All error handling uses Result/Option
2. **Regex error handling** - Wrapped in Ok() handling for robustness
3. **HTML entity escaping** - Comprehensive escaping of &, <, >, ", '
4. **Stop word filtering** - Consistent across title, content, and expansion parsing
5. **Serde pass-through** - SearchResult uses flatten for extensibility
6. **Default config** - ScoringConfig implements Default for convenience
7. **Module structure** - Clear separation of concerns with dedicated modules
8. **Doc comments** - Every public function and struct documented
9. **Test coverage** - All major functions have unit and integration tests
10. **WASM compatibility** - No system time assumptions, Instant-based timing only

## Dependencies

- **extism-pdk 1.2** - 1.2MB (plugin framework)
- **serde 1.0** - ~100KB (serialization)
- **serde_json 1.0** - ~50KB (JSON)
- **regex 1.0** - ~250KB (pattern matching)

Total uncompressed: ~1.4MB
Typical WASM binary size: ~500KB (after strip and LTO)

## Future Enhancements

1. Custom regex patterns for domain-specific cleaning
2. Caching layer for template resolution
3. Batch scoring API
4. Custom stop word lists per language
5. Pluggable recency functions
6. Support for weighted query terms
7. Result caching in WASM memory
8. Streaming large result sets

## Compatibility

- **Rust**: 1.70+ (2021 edition)
- **Target**: `wasm32-wasip1` (WASI Preview 1)
- **Extism**: 1.0.0+
- **JavaScript**: ES2020+ (for JSON handling in frontend)

## License

MIT
