# Versioning

How Scolta versions its packages, what compatibility guarantees you get, how functions move through their lifecycle, and how long old versions are supported.

## Packages

Scolta is a family of packages, not a single library:

```
scolta-core          Rust/WASM — scoring, prompts, HTML cleaning, result merging
scolta-php           PHP Composer package — wraps scolta-core for PHP platforms
scolta-python        Python pip package — wraps scolta-core for Python platforms
scolta.js            JavaScript — browser-side scoring, Pagefind integration, UI
scolta-drupal        Drupal module — depends on scolta-php
scolta-wp            WordPress plugin — depends on scolta-php
scolta-laravel       Laravel package — depends on scolta-php
```

## Version Numbers

All packages follow [Semantic Versioning](https://semver.org/) (MAJOR.MINOR.PATCH) with one additional rule: **the major version is synchronized across all packages.**

When Scolta is in the 1.x generation, every package has major version 1. scolta-core 1.4.2, scolta-php 1.7.0, scolta-drupal 1.3.1 — all compatible. When the project moves to 2.x, every package bumps to 2.0.0 in a coordinated release.

Minor and patch versions are independent. Each package ships features and bug fixes at its own pace. scolta-drupal might be at 1.3 while scolta-php is at 1.7 — that's normal. The major number tells you they belong to the same generation and work together.

**In short:**

- Same major number = compatible.
- `composer require tag1/scolta-drupal:^1.0` gives you a working set of packages. Always.
- When you upgrade to `^2.0`, update all Scolta packages together.

## What Each Number Means

**MAJOR** (1.x → 2.x): Breaking changes. Deprecated functions are removed. The WASM interface may change. All packages bump together. This is the only coordinated release.

**MINOR** (1.2 → 1.3): New features, new functions, promotions from experimental to stable, deprecations announced. Fully backward compatible within the same major. Each package increments independently.

**PATCH** (1.3.0 → 1.3.1): Bug fixes, security patches, performance improvements. No API changes. Each package increments independently.

## Development Versions (-dev Suffix)

Between releases, the version in the repo always carries a `-dev` pre-release suffix. This is standard practice across Cargo, Composer, npm, and pip — the `-dev` suffix is a semver pre-release identifier that sorts lower than the bare version (`0.2.0-dev < 0.2.0`).

**The workflow:**

```
0.1.0          ← tagged release
0.2.0-dev      ← immediately after release, bump to next target + "-dev"
  ... commits, features, fixes ...
0.2.0          ← strip "-dev" to release
0.3.0-dev      ← immediately bump again
```

For patch-only work on a released version: `0.1.1-dev` → `0.1.1`.

**Rules:**

1. **After tagging a release**, immediately bump the version to the next target with `-dev` appended. If you just released `0.3.0`, the repo should show `0.4.0-dev` (or `0.3.1-dev` if you expect only patches).

2. **Multiple commits happen on a `-dev` version.** The `-dev` suffix means "this is unreleased work in progress." You do not increment the version for every commit during development.

3. **To release**, remove the `-dev` suffix, tag, publish. The version `0.4.0-dev` becomes `0.4.0`.

4. **The version in the repo is always either a tagged release or a `-dev` pre-release.** A bare version like `0.4.0` in the repo means it has been (or is about to be) tagged. If the tag doesn't exist yet, the version should still have `-dev`.

5. **Decide the target version based on what changed:**
   - Only bug fixes since last release → next patch (`0.3.1-dev`)
   - New features or deprecations → next minor (`0.4.0-dev`)
   - Breaking changes → next major (`1.0.0-dev`) — coordinated across all packages

**Where the version lives:**

| Package | File | Field |
|---|---|---|
| scolta-core | `Cargo.toml` | `version = "0.1.0"` |
| scolta-php | `composer.json` | `"version": "0.1.0"` |
| scolta-drupal | `composer.json` | `"version": "0.1.0"` |
| scolta-wp | `composer.json` + `scolta.php` | `"version"` + `SCOLTA_VERSION` constant + plugin header |
| scolta-laravel | `composer.json` | `"version": "0.1.0"` |

For WordPress, the version appears in three places (composer.json, the plugin header comment, and the `SCOLTA_VERSION` constant). All three must match.

## Dependency Constraints

Each package declares what it needs from the tier above using caret constraints:

```json
// scolta-drupal composer.json
{
  "require": {
    "tag1/scolta-php": "^1.2"
  }
}
```

This means "any 1.x version of scolta-php that's at least 1.2.0." If you have scolta-php 1.7 installed, it satisfies the constraint. If scolta-drupal later uses a feature added in scolta-php 1.5, its constraint tightens to `^1.5`. Composer, Cargo, pip, and npm all handle this automatically.

scolta-core ships as a compiled WASM binary inside the scolta-php and scolta-python packages. You don't install scolta-core separately — it comes bundled. The scolta-core version used by scolta-php is documented in scolta-php's changelog.

## Function Lifecycle

Every exported function in scolta-core and every public method in scolta-php has a lifecycle state. Four states, one direction:

```
experimental → stable → deprecated → removed
```

### States

**experimental** — New, still being shaped. The API may change or disappear in the next minor release. Use it, report bugs, but don't build production workflows around it yet.

**stable** — Proven, tested, recommended. Will not break within a major version. If we need to change a stable function's behavior, we deprecate the old one and introduce a new one alongside it.

**deprecated** — Still works, but has a replacement. Fires a deprecation warning in PHP (`E_USER_DEPRECATED`) and a `console.warn` in JavaScript. The warning tells you what to use instead and when the function will be removed (always the next major version). Your code keeps working — you just get a heads-up to migrate.

**internal** — Not part of the public API. May change without notice in any release. If you're calling internal functions, you're on your own.

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
#[extism_pdk::plugin_fn]
pub fn score_results(input: String) -> String { ... }
```

**At runtime:** scolta-core exposes a `describe()` function that returns a machine-readable manifest of every exported function, its lifecycle state, when it was introduced, and (if deprecated) when it will be removed:

```json
{
  "version": "1.5.0",
  "wasm_interface_version": 1,
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

This manifest is the single source of truth. CI validates it. The PHP wrapper reads it at load time to generate deprecation warnings automatically. Documentation is generated from it. If you're building tooling on top of Scolta, `describe()` gives you everything you need.

### Deprecation Timeline

A function must be deprecated for **at least one minor release** before it can be removed in the next major version. In practice, we deprecate as early as possible to give you maximum runway.

Example timeline:

```
1.0.0  expandTermsParse() introduced (stable)
1.4.0  expandTermsParse() deprecated — replacement: parseExpansion()
       ↳ PHP: trigger_deprecation() fires on every call
       ↳ JS: console.warn on every call
       ↳ describe() manifest updated
       ↳ CHANGELOG and UPGRADE.md document the migration
1.5–1.x expandTermsParse() still works, still fires warnings
2.0.0  expandTermsParse() removed
       ↳ UPGRADE-2.0.md has before/after code examples
```

No function goes from stable to removed without passing through deprecated first. CI enforces this — a PR that removes a stable function without a deprecation phase will not merge.

## WASM Interface Version

Separate from the package version, scolta-core declares a **WASM interface version** — a single integer that tracks binary compatibility between scolta-core and its host wrappers (scolta-php, scolta-python, scolta.js).

```
WASM interface version 1 → scolta-core 1.0 through 1.x
WASM interface version 2 → scolta-core 2.0 through 2.x
```

The interface version increments when the WASM binary's function signatures or calling conventions change in a way that requires wrapper updates. In practice, this aligns with major versions, but it's tracked separately because:

- A major version bump in scolta-php might change PHP-side APIs without changing the WASM interface.
- A WASM interface change always requires wrapper updates, but not necessarily public API changes.

scolta-php checks the interface version at load time. If it loads a WASM binary with an unexpected interface version, it fails immediately with a clear error:

```
scolta-core reports WASM interface version 2, but this version of
scolta-php expects version 1. Update scolta-php to ^2.0.
```

You should never see this in normal use — it's a safety net for development and version mismatches during manual upgrades.

## Multi-Version Support

We maintain **at most two active major versions** at a time.

| Branch | Bug fixes | Security fixes | New features |
|---|---|---|---|
| Current major (e.g., 2.x) | Yes | Yes | Yes |
| Previous major (e.g., 1.x) | Critical only (6 months) | Yes (12 months) | No |
| Older | End of life | End of life | — |

When Scolta 2.0 ships, the 1.x branch enters maintenance. It gets security fixes for 12 months and critical bug fixes (data loss, index corruption, security) for 6 months. After 12 months, 1.x reaches end of life.

Security fixes are developed on the current branch and cherry-picked to the maintenance branch. New features are never backported.

### Upgrading Between Majors

Every major release ships with an `UPGRADE-X.0.md` file that lists every breaking change with before-and-after code examples. If you've addressed all deprecation warnings in your code during the 1.x cycle, upgrading to 2.0 should be straightforward — the deprecated functions you already migrated away from are the only things that get removed.

## For Contributors

If you're contributing to a Scolta package:

**Platform adapter contributors** (scolta-drupal, scolta-wp, scolta-laravel) work in pure PHP. You implement platform-specific integrations — config UI, routing, content export, CLI commands. You never touch the Rust crate or the WASM binary. Scoring, prompts, and HTML cleaning are handled by scolta-core through scolta-php. You can't introduce scoring drift because you don't implement scoring.

**Core contributors** (scolta-core, scolta-php) must follow the lifecycle rules:

1. New functions start as `experimental` unless the API is proven.
2. Promoting experimental to stable is a deliberate decision (requires a minor version bump).
3. Deprecating a stable function requires: `@deprecated` annotation with version and replacement, `trigger_deprecation()` call in PHP / `console.warn` in JS, a CHANGELOG fragment, and an entry in UPGRADE.md with migration instructions.
4. Removing a function requires: it was deprecated for at least one minor release, and the removal happens in a major version.

CI checks all of this. A PR that adds a public function without `@since` and `@stability` will fail. A PR that removes a stable function without a deprecation phase will fail. A PR that changes a function signature without a version bump will fail.
