# Versioning

For anyone who installs or depends on a Scolta package: how Scolta versions its packages, what compatibility guarantees you get, how functions move through their lifecycle, and how long old versions are supported. Maintainers should read [MAINTAINING.md](MAINTAINING.md) alongside it, which carries the operational detail.

## Packages

Scolta is a family of packages, not a single library:

```
scolta-core          Rust compiled to browser WASM: scoring, prompts, query expansion,
                     context extraction, result merging
scolta-php           PHP index builder and AI proxy; ships the browser bundle
scolta-drupal        Drupal module: depends on scolta-php
scolta-wp            WordPress plugin: depends on scolta-php
scolta-laravel       Laravel package: depends on scolta-php
scolta-python        Python index builder and AI proxy; ships the browser bundle
scolta-django        Django and Wagtail adapter: depends on scolta-python
scolta-node          TypeScript index builder and AI proxy; ships the browser bundle
scolta-next          Next.js adapter: depends on scolta-node
scolta-nuxt          Nuxt adapter: depends on scolta-node
scolta-astro         Astro adapter: depends on scolta-node
```

Eleven packages. The three bindings each implement their own server side and vendor the same browser
bundle; they do not wrap scolta-core at run time. [MAINTAINING.md](MAINTAINING.md) in this repo is the
list of record, carries the maintenance detail for each package, and describes the boundary in §1.

## Version Numbers

All packages follow [Semantic Versioning](https://semver.org/) (MAJOR.MINOR.PATCH), and **each package versions independently from its own git tags.** A release is the tag; the version written into the repository between releases is a working value that says where this package is heading, not a promise about any other package.

There is no rule that the major numbers match. scolta-core 1.4.2, scolta-php 1.7.0 and scolta-drupal 2.1.0 are a legitimate set if scolta-drupal's declared constraint accepts that scolta-php. **Compatibility is expressed in exactly one place: the dependency constraint a package declares for its upstream**, which Composer reads on every install and enforces. Matching majors across five repositories expressed nothing that the constraint did not already express, and it charged a release to every package whenever one of them broke its API.

**In short:**

- The constraint answers what works together. `"tag1/scolta-php": "^1.2.0"` in scolta-drupal means any scolta-php from 1.2.0 up to but not including 2.0.0.
- `composer require tag1/scolta-drupal` resolves a working set, because the constraint is what resolution reads.
- A major bump in one package obliges nothing in another. When an adapter starts using an API that only the new line has, it raises its constraint, and that is the whole of the coordination.

## What Each Number Means

**MAJOR** (1.x → 2.x): Breaking changes in this package. Deprecated functions are removed. The WASM interface may change. Each package bumps its own major when its own public API breaks; there is no all-package major release.

**MINOR** (1.2 → 1.3): New features, new functions, promotions from experimental to stable, deprecations announced. Fully backward compatible within the same major. Each package increments independently.

**PATCH** (1.3.0 → 1.3.1): Bug fixes, security patches, performance improvements. No API changes. Each package increments independently.

## Development Versions (-dev Suffix)

Between releases, the version in the repo carries a `-dev` pre-release suffix. The `-dev` suffix is a [semver pre-release identifier](https://semver.org/#spec-item-9) that sorts lower than the bare version (`1.1.0-dev < 1.1.0`).

**Why we use it:** Scolta is a multi-package project with multiple contributors (human and automated). When someone opens `Cargo.toml` or `composer.json` in the repo, `-dev` makes the state self-documenting: you can immediately tell whether you're looking at a tagged release or unreleased work in progress, without cross-referencing git tags.

**Ecosystem notes:** The `-dev` suffix is a first-class concept in Composer (PHP), where it maps to a stability level that prevents accidental installation in production (requires `minimum-stability: dev` or an explicit `@dev` flag). In Cargo (Rust), pre-release identifiers are valid semver and work correctly, but most Rust crates don't use them between releases. We do, because of the multi-package coordination benefit.

**The workflow:**

```
1.0.0          ← tagged release
1.1.0-dev      ← immediately after release, bump to next target + "-dev"
  ... commits, features, fixes ...
1.1.0          ← strip "-dev" to release
1.2.0-dev      ← immediately bump again
```

For patch-only work on a released version: `1.0.1-dev` → `1.0.1`.

**Rules:**

1. **After tagging a release**, immediately bump the version to the next target with `-dev` appended. If you just released `1.0.0`, the repo should show `1.1.0-dev` (or `1.0.1-dev` if you expect only patches).

2. **Multiple commits happen on a `-dev` version.** The `-dev` suffix means "this is unreleased work in progress." You do not increment the version for every commit during development.

3. **To release**, remove the `-dev` suffix, tag, publish. The version `0.4.0-dev` becomes `0.4.0`.

4. **The version in the repo is always either a tagged release or a `-dev` pre-release.** A bare version like `0.4.0` in the repo means it has been (or is about to be) tagged. If the tag doesn't exist yet, the version should still have `-dev`.

5. **Decide the target version based on what changed:**
   - Only bug fixes since last release → next patch (`1.0.1-dev`)
   - New features or deprecations → next minor (`1.1.0-dev`)
   - Breaking changes → next major (`2.0.0-dev`) for this package alone


**Where the version lives** differs per package, and for Drupal and WordPress the `composer.json` `version` key must be absent rather than present. These files hold the working version between releases; the version of record for a release is the tag. The table is in `MAINTAINING.md`, "Where the version lives", which is the single place that fact is maintained.

## Dependency Constraints

Each adapter declares what it needs from scolta-php using a caret constraint:

```json
// scolta-drupal composer.json
{
  "require": {
    "tag1/scolta-php": "^1.0"
  }
}
```

This means "any 1.x version of scolta-php that's at least 1.0.0." If you have scolta-php 1.7 installed, it satisfies the constraint. If scolta-drupal later uses a feature added in scolta-php 1.5, its constraint tightens to `^1.5`. Composer handles this automatically.

Adapters bundle the exact scolta-php version recorded in their committed `composer.lock`, resolved from Packagist. Tagging an adapter release does not require a matching scolta-php tag, and no CI job compares one package's version number against another's. To adopt a newer scolta-php, run `composer update tag1/scolta-php` in a PR, test, and commit the updated lock.

**One cross-repository rule survives, and it is about releases rather than numbers: a stable release must not depend on an unreleased upstream.** Each repository's release workflow enforces it with a lock guard that refuses to publish while the committed lock names a development version, which is why scolta-drupal 1.2.0 could not ship before scolta-php 1.2.0 existed. Everything else a package asserts about versions it asserts about itself: the `coherence` check refuses a package that states two different development lines about its own version, which is self-consistency and not agreement with a sibling.

scolta-core ships as a compiled WASM binary inside scolta-php, scolta-python and scolta-node. You don't install scolta-core separately: each binding vendors the browser bundle and serves it to the page. The scolta-core version a binding carries is recorded in that binding's changelog. The WASM runs in the browser only; the server side of each binding is written in that binding's own language ([MAINTAINING.md](MAINTAINING.md), §1).

## Function Lifecycle

Every function scolta-core exports to the browser, and every public method in a binding, has a lifecycle state. Four states, one direction:

```
experimental → stable → deprecated → removed
```

### States

**experimental.** New, still being shaped. The API may change or disappear in the next minor release. Use it, report bugs, but don't build production workflows around it yet.

**stable.** Proven, tested, recommended. Will not break within a major version. If we need to change a stable function's behavior, we deprecate the old one and introduce a new one alongside it.

**deprecated.** Still works, but has a replacement. Fires a deprecation warning in PHP (`E_USER_DEPRECATED`) and a `console.warn` in JavaScript. The warning tells you what to use instead and when the function will be removed (always the next major version). Your code keeps working; the warning is your notice to migrate.

**internal.** Not part of the public API. May change without notice in any release. If you're calling internal functions, you're on your own.

### How to Check

**In code:** Every function has `@since`, `@stability`, and (if applicable) `@deprecated` annotations:

```php
/**
 * Score and re-rank search results.
 *
 * @since 1.0.0
 * @stability stable
 */
public static function scoreResults(array $results, array $config, string $query): array

/**
 * Parse LLM expansion response into term list.
 *
 * @since 1.0.0
 * @deprecated 1.4.0 Use parseExpansion() instead. Removal: 2.0.0.
 * @stability deprecated
 */
public static function expandTermsParse(string $llmResponse): array
```

In Rust, the same information lives in doc comments and Rust's native `#[deprecated]` attribute:

```rust
/// Score and re-rank search results.
///
/// # Stability
/// - **Status:** stable
/// - **Since:** 1.0.0
#[wasm_bindgen]
pub fn score_results(input: &str) -> Result<String, JsError> { ... }
```

**At runtime:** scolta-core's browser WASM exports a `describe()` function that returns a machine-readable manifest of every exported function, its lifecycle state, when it was introduced, and (if deprecated) when it will be removed:

```json
{
  "name": "scolta-core",
  "version": "1.5.0",
  "wasm_interface_version": 4,
  "functions": {
    "score_results": {
      "since": "1.0.0",
      "stability": "stable"
    },
    "score_results_v2": {
      "since": "1.5.0",
      "stability": "experimental"
    },
    "expand_terms_parse": {
      "since": "1.0.0",
      "stability": "deprecated",
      "deprecated_in": "1.4.0",
      "replacement": "parse_expansion",
      "removal": "2.0.0"
    },
    "parse_expansion": {
      "since": "1.4.0",
      "stability": "stable"
    }
  }
}
```

The example above shows the shape; call `describe()` for the live manifest. It is the source of truth for what the loaded WASM exports and what state each export is in, and it is the answer to "which build is this page running". If you're building tooling on top of Scolta in the browser, read it there.

Its limits. Nothing on the server side reads it: there is no server-side WASM in any binding, so nothing generates PHP or Python deprecation warnings from it. No documentation is generated from it. No CI job compares it against the source annotations. Keeping the manifest, the annotations and this document in step is a review step, and the pull request template asks for it.

### Deprecation Timeline

A function must be deprecated for **at least one minor release** before it can be removed in the next major version. In practice, we deprecate as early as possible to give you maximum runway.

Example timeline, using a hypothetical function:

```
1.0.0  expandTermsParse() introduced (stable)
1.4.0  expandTermsParse() deprecated, replacement: parseExpansion()
       + @deprecated annotation naming the replacement and the removal version
       + a runtime deprecation warning where the language has one
       + describe() manifest updated, for a core export
       + CHANGELOG and UPGRADE.md document the migration
1.5 to 1.x expandTermsParse() still works, still fires warnings
2.0.0  expandTermsParse() removed
       + UPGRADE-2.0.md has before/after code examples
```

No function goes from stable to removed without passing through deprecated first. That is a review rule, not a CI gate: no job today compares a pull request against the previous release's public surface.

## WASM Interface Version

Separate from the package version, scolta-core declares a WASM interface version: a single integer, currently 4, that tracks binary compatibility between the WASM binary and the front end that loads it.

The interface version is an internal protocol version that is incremented whenever the WASM binary's function signatures or calling conventions change in a way that breaks binary compatibility with the code calling it. It does not align with the package major version. It has been incremented multiple times within the 0.x and 1.0-rc series as exports were added or removed.

Historical progression, each step named for the change that bumped it. Reproduce it with
`git log -S WASM_INTERFACE_VERSION` in this repo:

- Version 1: the constant introduced alongside the lifecycle annotations, over the wasm-bindgen exports
  as they then stood.
- Version 2: the server-side plugin target removed. `clean_html`, `build_pagefind_html` and `debug_call`
  went with it, and the browser exports became the whole surface.
- Version 3: `batch_score_results` added.
- Version 4: context extraction (`extract_context`, `batch_extract_context`), query sanitizing
  (`sanitize_query`), conversation trimming (`truncate_conversation`) and priority-page matching
  (`match_priority_pages`) added; `to_js_scoring_config` dropped. Current.

The interface version is tracked separately from the package version because:

- A major version bump in a binding might change its server-side API without changing the WASM interface.
- A WASM interface change always requires the front end to be updated, but not necessarily any public API change.

No binding enforces the interface version at load time today. It is a declaration, readable through `describe()`, that tells you which protocol a given bundle speaks. The thing that actually keeps a bundle and its front end together is that they ship as one vendored set of files: change the bundle, re-vendor it, and the parity checks in [ASSETS.md](ASSETS.md) catch a carrier left behind. Bump the integer when you change a signature, so the manifest tells the truth.

## Multi-Version Support

We maintain **at most two active major versions of a package** at a time.

| Branch | Bug fixes | Security fixes | New features |
|---|---|---|---|
| Current major (e.g., 2.x) | Yes | Yes | Yes |
| Previous major (e.g., 1.x) | Critical only (6 months) | Yes (12 months) | No |
| Older | End of life | End of life | n/a |

When a package's 2.0 ships, its 1.x branch enters maintenance. It gets security fixes for 12 months and critical bug fixes (data loss, index corruption, security) for 6 months. After 12 months, 1.x reaches end of life.

Security fixes are developed on the current branch and cherry-picked to the maintenance branch. New features are never backported.

### Upgrading Between Majors

Every major release ships with an `UPGRADE-X.0.md` file that lists every breaking change with before-and-after code examples. If you've addressed all deprecation warnings in your code during the 1.x cycle, upgrading to 2.0 should be straightforward: the deprecated functions you already migrated away from are the only things that get removed.

## For Contributors

If you're contributing to a Scolta package:

**Platform adapter contributors** (scolta-drupal, scolta-wp, scolta-laravel in PHP; scolta-django in Python; scolta-next, scolta-nuxt, scolta-astro in TypeScript) work only in their platform's language. You implement platform-specific integrations: config UI, routing, content export, CLI commands. You never touch the Rust crate or the WASM binary, and you don't implement scoring, HTML cleaning, indexing or prompt logic: scoring comes from the browser WASM, and the rest from the binding your adapter depends on.

You can still change what a search returns without writing a line of scoring code. What your adapter exports, in what order, with which fields and which build settings, decides what goes into the index. If your change moves result counts, say so in the pull request and name the query.

**Binding and core contributors** (scolta-core, scolta-php, scolta-python, scolta-node) implement the server side in their own language: HTML cleaning, indexing, tokenizing, stemming and the AI proxy are native in each binding, and parity with the PHP reference is held by fixtures and identity tests rather than by shared code ([MAINTAINING.md](MAINTAINING.md), §1). Follow the lifecycle rules:

1. New functions start as `experimental` unless the API is proven.
2. Promoting experimental to stable is a deliberate decision (requires a minor version bump).
3. Deprecating a stable function requires: an `@deprecated` annotation with the version and the replacement, a runtime deprecation warning where the language has one, a CHANGELOG fragment, and an entry in UPGRADE.md with migration instructions.
4. Removing a function requires: it was deprecated for at least one minor release, and the removal happens in a major version.

None of that is enforced by a CI job. The pull request templates in scolta-core and scolta-php ask for the annotations and the CHANGELOG entry; scolta-python and scolta-node have no template, so the reviewer is the only check. Say in the pull request which of the four applies to your change.
