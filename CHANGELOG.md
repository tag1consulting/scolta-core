# Changelog

All notable changes to scolta-core will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/). Major versions are synchronized across all Scolta packages; minor and patch versions are released independently per package.

## [Unreleased]

### Fixed
- **Category and context queries now decompose into instances instead of restating themselves (`EXPAND_QUERY` task definition, rules 13-14, Examples block).** A query naming a grouping ("data protection regulations", "crewed Apollo missions") was expanded into near-synonyms of the grouping — "privacy laws", "data security requirements", "compliance frameworks" — rather than into the instances that actually appear in prose. Rules 13-14 already existed to prevent exactly this and were not firing.

  The cause was not the strength of those rules but the **task definition above them**, which said "only return different phrasings that would find additional relevant content". That sentence describes the failure mode, and rules 13-14 read as exceptions to it that the model declined to take. This was measured rather than assumed: two candidate fixes confined to rules 13-14 — an operative `MUST` plus a `NOT`-example in the shape rules 16-17 already use, and a mechanical "try to name three instances" trigger replacing rule 13's escape hatch — each moved **nothing at all**, returning byte-identical expansions on six live queries over three cache-cleared rounds apiece. A third candidate that added both to the rules while also adding worked examples was *worse* than the examples alone, which is the attention cost of a longer rule list showing up as a measurement.

  The definition now names both kinds of expansion — an alternate PHRASING and a DECOMPOSITION into the concrete instances the query covers — and states which to prefer and why. Three supporting changes: **rule 13 drops a closed-set presupposition** ("only decompose when you can name *the members* confidently"), which is why it never fired on open-ended groupings — every query that failed live was open-ended (regulations, platforms, cookout dishes) and every one that passed was a closed canonical set (noble gases, version control systems); **rule 13 loses two of its own examples** (`"European cars" → ["German cars", "Italian cars", "French cars"]` and `"Southeast Asian food" → ["Thai", ...]`), which taught category→sub-category substitution, structurally the same move as the bad expansions, and names that shape as forbidden instead; and **the trailing Examples block gains three decomposition cases**, since five of its eight entries demonstrated paraphrase and none showed a query with a generic head noun ("regulations", "menu", "platforms") decomposing. Adding those examples alone flipped one query on its own.

  The clause subordinating the new preference to rule 15 is load-bearing in both directions. Without it, "Apollo 24 mission" began asserting that the nonexistent mission **is** the Apollo-Soyuz Test Project in 2 of 5 live rounds — pressure to name instances overriding the unrecognized-entity guard — and with it the query returns to clean in 5 of 5. It is also what made the category queries decompose at all: stating that you cannot name instances of what you do not recognize implies that you can and should for what you do.

  Measured live against four demo corpora, five cache-cleared rounds per query at 1.8-7.4s call latencies. Of the five reported failures, two now pass consistently (`data protection regulations` → `["GDPR", "HIPAA", "CCPA", "privacy laws"]`, 9/9 rounds; `crewed Apollo missions` → `["Apollo 7" … "Apollo 12"]`, 10/10), one is borderline (`responding to a data breach`, 6/9), and **two do not move** (`a backyard barbecue menu`, `growing an online following`), the latter gaining one wanted term but staying under the bar. The regression set held or improved (noble gases 6/6 hits; first aid kit contents 3 → 5 hits and now a real decomposition rather than crammed pairs; version control systems still passes with 4 terms rather than 6), and the no-fabrication set is clean on all five probes. **[parity]** Expansions change for category, context, and process queries; adapter prompt mirrors must carry the identical text.
- **`sanitize_query` no longer logs IPv6 addresses or unhyphenated SSNs in the clear.** The SSN pattern was hyphen-only (`\b\d{3}-\d{2}-\d{4}\b`) and the IP pattern was IPv4-only, so `123 45 6789`, `123456789`, `2001:0db8:85a3:0000:0000:8a2e:0370:7334` and `fe80::1` all passed through untouched into analytics logs — in a function documented as redacting SSN and IP, and relied on as the only sanitization layer by the PHP, WordPress, Drupal, Laravel and 11ty consumers.

  **SSN** is now an explicit alternation over the three reported forms — `\b(?:\d{3}-\d{2}-\d{4}|\d{3}\s\d{2}\s\d{4}|\d{9})\b` — rather than one pattern with independently optional separators. The optional-separator form does not express "3-2-4" at all but "nine digits with optional breaks after the third and fifth", which also matches every 5+4 grouping: ZIP+4 (`12345-6789`), store-locator queries and part numbers were all redacted as SSNs. Each branch now pins its own separator positions. Mixed separators (`123-45 6789`) are consequently not supported; no such form was reported, and admitting it reintroduces the structure that caused the collision. The `\b` anchors keep the pattern off longer digit runs, so a 10-digit phone and a 16-digit card still redact as `[PHONE]` and `[CC]`.

  **IPv6** is matched by a loose candidate pattern plus a real `std::net::Ipv6Addr` parse, not by a hand-rolled grammar. Hand-rolling produced two defects: a group bound one short of the grammar, which matched `1::2:3:4:5:6:7` only as far as `1::2:3:4:5:6` and left a `:7` fragment in the log, and a compressed branch that could not distinguish an address from namespace syntax. Parsing fixes both and adds leading-`::` (`::1`) and IPv4-mapped (`::ffff:192.0.2.1`) support, which the regex could not express safely. Parsing alone is *not* sufficient: `abc::def`, `db::add`, `ec::add` and `cafe::babe` are all syntactically valid IPv6 addresses, and on a Rust or C++ documentation index that is live corpus. A candidate is therefore redacted only when it parses AND either contains a decimal digit or contains a colon outside the `::` run — neither of which `namespace::member` can produce. The trade is that a digit-free two-group address (`cafe::babe`) is left alone; every real-world prefix (link-local `fe80::/10`, global unicast `2000::/3`, loopback `::1`, multicast `ff02::/16`) carries a digit in its first group, so this costs documentation examples rather than hosts. `12:34:56` and MAC addresses are rejected by the parser outright. The prefix scan is capped at the longest textual IPv6 address (45 bytes, `ffff:…:255.255.255.255`) so the pass stays linear in query length: the candidate pattern matches unbounded hex-and-colon runs and `from_str` is itself linear, so an uncapped scan was quadratic on untrusted query text. Capping is behavior-preserving, since a longer prefix could never parse.

  No config, signature or `WASM_INTERFACE_VERSION` change. ([#53](https://github.com/tag1consulting/scolta-core/issues/53)) **[parity]** Behavior changes for existing sites: an isolated nine-digit run now logs as `[SSN]` whatever it is, including a 9-digit ZIP, because an unformatted SSN is indistinguishable from one; and IPv6 hosts now log as `[IP]`. Adapters ship a vendored WASM build, so the fix reaches sites only after a scolta-core release and re-vendor; consumers running their own regex pass on top (tag1.com-11ty-website#1292) can keep it as defense-in-depth — note that pass uses a `{2,7}`-group IPv6 pattern, which redacts timestamps and MAC addresses that this one deliberately does not.
- **Quality / experience queries now surface the content that embodies them (`EXPAND_QUERY` rule 17).** A query that names a feeling, reaction, or judgment rather than a topic ("scary moment", "inspiring story") was expanded into synonyms of the adjective ("frightening experience", "terrifying incident") — but authors narrate a tense episode by describing the concrete thing that went wrong (the malfunction, the alarm, the aborted attempt), and almost never label it "scary". The synonym expansion therefore matched the wrong posts (solemn pieces that happen to contain "moment") and missed the genuinely tense ones (a program alarm, a near-abort), which shared no vocabulary with the expansion at all. This is the same expansion-restates-the-query failure that rules 13-14 (category/context) and rule 16 (named entity) fix, in the variant those rules do not reach: an abstract quality. New **rule 17 (QUALITY / EXPERIENCE → CONCRETE INSTANCES)** expands such a query into the concrete events, systems, or situations that embody the quality in prose, forbids restating the query adjective as a synonym, and reconciles the term cap ("up to 6 concrete instances"). Rule 15 still bounds it, but rule 17 states that its fallback is itself concrete, because "fall back to neutral topic phrasings when unsure" was being read as licence to emit the very genre labels the rule bans. The rule covers every valence, not only things that went wrong: with negative-valence guidance alone the model had no template for humour or admiration and restated the adjective on exactly those queries ("amusing story", "uplifting narrative"), so the funny and inspiring cases are named and their vocabulary banned explicitly. Examples are drawn from two unrelated domains (wildlife photography, software) and from no Scolta demo corpus, and are marked as illustrations rather than a term bank after the model was observed emitting them verbatim for an unrelated corpus. **[parity]** Expansions change for quality/experience queries; adapter prompt mirrors must carry the identical rule text.
- **Identifier / proper-noun queries no longer miss the content that describes them (`EXPAND_QUERY` rule 16, `SUMMARIZE` grounding rules, `FOLLOW_UP` grounding check).** A query centred on a named entity or event ("Apollo 13 crisis", "Apollo 1 fire") was expanded with terms that all kept the entity anchor — "Apollo 13 accident", "Apollo 13 explosion", "Apollo 13 oxygen tank failure" — and on a real corpus every one of those returned **zero** results, because authors write "the mission", "Lovell", "the lifeboat" far more often than they repeat the mission number. The bare in-corpus vocabulary that does match ("oxygen tank explosion", "lunar module lifeboat") was never emitted. This is the same expansion-restates-the-query failure as the category/context cases fixed by rules 13-14, in the variant those rules do not reach: the restatement preserves an anchor instead of a category name. New **rule 16 (NAMED ENTITY / EVENT → DEFINING DETAILS)** expands such a query into the concrete details that identify it in prose (participants, components, distinctive phrases, causes, consequences), requires that at least half the terms drop the entity name entirely, and explicitly forbids appending the name to a list of near-synonyms. The term cap is reconciled ("up to 6 defining details"), and rule 15 still bounds it, so an unrecognized entity falls back to neutral phrasings instead of acquiring invented participants or parts. Examples are deliberately drawn from three unrelated domains (consumer product, vehicle spec, historical event) and from no Scolta demo corpus.
- **The AI summary can no longer claim the collection lacks content.** The `SUMMARIZE` grounding check previously *instructed* this failure: its CORPUS AWARENESS bullet told the model to say "[site] focuses on [scope], so it doesn't include a dedicated article on [topic]". The model only ever sees one search's slice of the corpus, so it can never support that claim — and it was observed asserting a full week-long article arc did not exist. The bullet is replaced by four rules: **PARTIAL VIEW** (the excerpts are a slice, never the collection), **NEVER ASSERT ABSENCE** (with the observed phrasings named as banned), **WEAK RESULT SETS** (attribute a thin result set to *this search*, not the collection, and suggest narrower terms), and the preserved no-invented-statistics guard from #33. `FOLLOW_UP`'s grounding check carried the same defect ("This collection doesn't appear to have content on [topic].") and is fixed the same way. The pinned snapshot fixture moves from `tests/fixtures/corpus_awareness_bullet.txt` to `tests/fixtures/absence_grounding_rules.txt`.
- **`SUMMARIZE` now understands a weak-match context marker.** When the full query matches nothing and results come from the broadened OR fallback, `scolta.js` prepends `[No result matched the full query; ...]` to the context. The prompt keys off that marker so a fallback result set is described as a limitation of the search rather than of the corpus.
- **`extract_context` no longer hardcodes English stop words.** Query-term extraction used `extract_terms(query, "en")` regardless of the configured language, so e.g. German stop words ("der", "und") anchored snippets they should never anchor, and the snippet budget was wasted on noise. `ContextConfig` gains a `language` field (ISO 639-1, default `"en"`) plumbed through the `extract_context`/`batch_extract_context` config JSON. **[parity]** Non-English sites get different (correct) extracted snippets; adapter fixtures must be regenerated.
- **Sentence-boundary intro truncation was a no-op.** `extract_context` sliced the intro to exactly `intro_length` chars and then called `truncate_at_sentence(intro_raw, intro_len)`, which returns its input unchanged when `len <= max` — so despite the documented algorithm, every intro was a mid-word hard cut. The slice now takes one char of lookahead so the truncation actually runs and intros end at the last sentence boundary within `intro_length`. **[parity]** Extracted intros change for any content longer than `max_length`; adapter fixtures must be regenerated.
- **Snippet byte offsets are now mapped from lowercased to original text.** Keyword positions were found in `remaining.to_lowercase()` but used to slice the original `remaining`; `to_lowercase` is not length-preserving ('İ' U+0130 grows 2 → 3 bytes), so content with case-expanding characters produced shifted snippets that could miss the matched keyword entirely. The search now records a lowered-byte → original-offset map and slices through it. Pinned by an İ-based regression test (the existing UTF-8 suite only covered length-preserving characters). **[parity]** Only affects content with case-expanding scripts.
- **`truncate_conversation` counts characters, not bytes.** The `max_length` limit is documented in characters but was summed via `content.len()` (bytes), over-trimming multibyte conversations (e.g. CJK, accented text) by up to 4×. Now counted with `chars().count()`, pinned by a multibyte test. **[parity]** PHP/Python/Node adapters must assert the same semantic (mb_strlen-equivalent, not strlen).
- **Custom PII redaction no longer fails open.** `sanitize_query` silently skipped any custom pattern whose regex failed to compile, and silently dropped malformed entries (missing/non-string `regex` or `replacement`) at config parse — a typo'd patient-ID pattern meant no redaction and no signal. Custom patterns are now validated and compiled when the config is parsed: a bad pattern is a `JsError` naming the offending `custom_patterns[i]`. Valid patterns are also no longer recompiled on every call — compiled regexes are cached per pattern string (built-ins already used `OnceLock`). No export signature changed (`sanitize_query` already returned `Result`), so `WASM_INTERFACE_VERSION` is unchanged. Covered by invalid-pattern, malformed-entry, wrong-type, and compile-once tests.
- **Config clamping is now actually wired in.** `API.md` documented that out-of-range scoring config values are clamped (`ScoringConfig::clamp_and_validate` via `config::from_json_validated`), but every production entry point used unvalidated `from_json`, so e.g. `recency_boost_max: 100.0` silently broke ranking exactly as the doc promised it wouldn't. `score_results` and `batch_score_results` now parse config through `from_json_validated`; clamped fields emit a `console.warn` in the browser (stderr on native). **[parity]** Sites shipping out-of-range config values will see ranking change to the documented clamped behavior; language adapters should mirror the same clamping.
- **`filter_single_word_generic` knob is now honored.** The flag was parsed from `parse_expansion` input and documented, but `should_keep_term` ignored it — single-word generic terms were always removed. The removal is now gated on the flag; the default (`true`) preserves existing behavior. `ExpansionConfig::default()` now also matches the documented defaults (a derived `Default` had all booleans `false`). Tested in both flag states.
- **Wrong-typed fields now report `InvalidFieldType` instead of "missing required field".** Every parse site mapped a present-but-wrong-typed field (e.g. `"query": 42`) to `MissingField`, which the error enum and `API.md` promised to distinguish. String fields and `batch_score_results.queries` now emit `InvalidFieldType` ("field 'query' must be a string"); genuinely absent fields still report `MissingField`. The test that codified the misleading message now asserts the distinction.
- **Negative `merge_results` set weights are clamped to 0.0.** A negative `MergeSet.weight` silently inverted that set's ranking (highest-scored results sank to the bottom); NaN weights also normalize to 0.0.

### Changed
- **Release tarball is now content-swept fail-closed (`scripts/validate-tarball.sh`).** The published artifact (`scolta-core-${VERSION}.tar.gz`) is scolta-core's only pre-built deliverable — the crate is not on crates.io and the compiled WASM is not committed — but its source dir `pkg/` belongs to wasm-pack, which also drops `package.json`, `README.md`, `LICENSE`, and a `.gitignore` (containing `*`) in there; a wasm-pack upgrade can change that set silently. The release tar selects members by name (so today's artifact is clean), but nothing GUARDED that selection or the payload size. New `scripts/validate-tarball.sh` extracts the tarball and enforces a fail-closed allowlist (`scolta_core_bg.wasm`, `scolta_core.js`, `scolta_core.d.ts`, `scolta_core_bg.wasm.d.ts`) plus a hard size cap on the WASM (2,500,000 bytes, ~2x the measured 1,244,431-byte release build); an unknown member or oversize payload fails with a wasm-pack-version hint pointing at the allowlist. It is called from `release.yml` after the build AND from a `ci.yml` job that builds + validates on every PR (the old `tarball-allowlist` job tar'd from the same hardcoded list it then asserted, so it could never catch drift; it now sweeps the whole `pkg/` dir for unexpected files and runs the shared validator). No code or WASM change. (Precedent: the scolta-wp 13 MB zip incident and WP.org dist-cruft flags.)
- **Lint, CI, and docs hardening.** Cargo.toml gains a `[lints.clippy]` table with cherry-picked pedantic lints (`cast_possible_truncation`, `missing_errors_doc`, `or_fun_call`); the unchecked `as u32` casts in config/context/conversation parsing now saturate at `u32::MAX` via `config::saturate_u32` instead of silently wrapping (the provably-bounded calendar-math casts in `scoring.rs` carry documented `#[allow]`s). CI clippy now runs with `--all-targets` (test code was never linted) and adds a `wasm32-unknown-unknown` run so the `cfg(target_arch = "wasm32")` branches are linted too. `release.yml` gains lint + test jobs and `needs: [lint, test]` on the release job, so an untested tag can no longer ship. All 8 previously-undocumented WASM exports have `# Stability` doc blocks, cross-checked against `describe()` by a new doc-sync guard test (`stability_doc_blocks_match_describe`); every `Result`-returning export and `inner::` function documents its `# Errors`. Cargo metadata gains `rust-version = "1.77"` (floor set by wasm-bindgen/js-sys), `homepage`, and `documentation`. CLAUDE.md: removed the stale "temporary crate-type switch" note (`rlib` is already in `crate-type`) and the reference to nonexistent Pagefind integration tests.

- **`merge_results` now delegates to `merge_results_with_debug`.** The two were a ~60-line copy-paste; the plain variant is now the debug variant minus the counters, pinned by an identical-output test.
- **`describe()` export returns `Result<String, JsError>` instead of swallowing serialization failure into an empty string.** The generated TypeScript signature is unchanged (`describe(): string`; throws on the unreachable error path), so callers and `WASM_INTERFACE_VERSION` are unaffected.
- Docs-only metadata/comment corrections (no behavior change): the `parse_expansion_with_config` filtering doc-block now matches `should_keep_term` — acronyms pass at ≤3 characters (not "≤4"), and single-word generic removal is described as gated on `filter_single_word_generic` (not unconditional). The `documentation = "https://docs.rs/scolta-core"` key is removed from `Cargo.toml`: the crate is not published to crates.io, so docs.rs hosts nothing; `homepage`/`repository` already point at the public GitHub repo and there is no separately hosted rustdoc location.
- Performance touch-ups, no behavior change: error constructors in parse paths are lazily evaluated (`ok_or` → `ok_or_else`); the lowercased `generic_terms` list is built once per `parse_expansion` call instead of once per term; stop-word checks no longer lowercase already-lowercased terms twice; the repeated parse/serialize wrapper in `browser.rs` is one generic `json_call` helper; the magic clock-fallback date in `scoring.rs` is a named, documented constant. Doc fix: `keep_acronyms` keeps terms ≤3 characters (code), not "≤4" as the doc comment claimed.

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
