# Maintaining Scolta

For anyone who maintains a Scolta package: the package list, where each one publishes, how versions
move, and what checks exist. Outside contributors can read it to see why a repo is arranged the way it
is; the steps marked **maintainer-only** need the private `scolta-fleet` repo.

This guide lives in scolta-core because that repo is public, so every package can link to it. If a fact
is true for more than one repo, it lives here. If a fact is true for only one repo, it lives in that
repo's own `MAINTAINING.md`, except scolta-core's own facts, which are the last section of this file.

The other guides in this repo:

| Guide | What it covers |
|---|---|
| [RELEASING.md](RELEASING.md) | How to cut a release, per package, and what goes wrong. |
| [REGRESSION.md](REGRESSION.md) | The tests to run before you ship. |
| [ASSETS.md](ASSETS.md) | The browser bundle and how the committed copies stay in sync. |
| [VERSIONING.md](VERSIONING.md) | The compatibility contract users read. |
| [API.md](API.md) | The exported WASM functions and their signatures. |
| [IMPLEMENTATION.md](IMPLEMENTATION.md) | How the crate is built internally. |
| [TESTING.md](TESTING.md) | How to test the crate. |
| [regression/](regression/) | The proposed public query corpus and benchmarks. |

The tooling these guides describe (the harness, the coherence checks, the demo manifest, the blessed
counts) lives in the `scolta-fleet` repo, which is private. This guide is the public description of how
it all fits together; scolta-fleet is where you run it.

One rule to keep this from going stale: don't write version numbers, floors, or lock states into these
docs. They are out of date within a week. The scripts that print them are not. So a doc says "run the
floor-matrix script," not "the floor is 1.2." Read live numbers from the tool, never from prose.

## Terms

Used throughout these guides and in scolta-fleet. Defined once here.

| Term | Meaning |
|---|---|
| **blessing** | Recording a measured number in `baselines.json` as the expected value, together with the report file that measured it. A blessing without a named report is refused. |
| **armed row** | A baseline row that fails the regression run when the live value misses it by more than its tolerance. |
| **capture row** | A baseline row that records what was observed and never fails a run. New queries start here. |
| **track** | Which reading a row measures. `base-only` is the index-anchored count, the pass/fail tripwire. `settled-expanded` is the count after live expansion terms are unioned in; it is recorded for orientation and is not measured by the harness, because an LLM-volatile number cannot be a pass/fail baseline. |
| **overlay** | How a run points a demo at unreleased code without editing the demo. The Composer overlay writes a separate generated root manifest and selects it with the `COMPOSER` environment variable, so the demo's own `composer.json` is never opened for writing. Where a demo commits the adapter rather than installing it, a second overlay renames that directory aside for the run and puts it back afterwards, because Composer installs the library underneath a committed copy. Demos with no overlay are not measured. |
| **coherence bundle** | `scripts/check-coherence.mjs`, a dependency-free file generated from TypeScript in scolta-fleet and vendored into the four PHP package repos, where each runs it as its own `Version coherence` CI job. Never hand-edit a vendored copy. |
| **confounded run** | A run made with `--allow-skew`, where demos resolved different versions of the same package. Nothing is blessed from a confounded run. |

## 1. The packages

Eleven packages, a private fleet repo (`scolta-fleet`) that holds the cross-repo tooling, and a set of
demo repos (§7).

| Package | Language | Sits on | What it is |
|---|---|---|---|
| `scolta-core` | Rust to WASM | (nothing) | The browser engine: scoring, query expansion, sanitize, stop words, context extraction, conversation trimming, and the canonical prompt text. |
| `scolta-php` | PHP | scolta-core (vendored bundle) | The reference server library: index builder plus AI proxy. Owns the canonical browser bundle. |
| `scolta-laravel` | PHP | scolta-php | Laravel adapter |
| `scolta-drupal` | PHP | scolta-php | Drupal module |
| `scolta-wp` | PHP | scolta-php | WordPress plugin |
| `scolta-python` | Python | scolta-core (vendored bundle) | Python binding: index builder plus AI proxy |
| `scolta-django` | Python | scolta-python | Django and Wagtail adapter |
| `scolta-node` | TypeScript | scolta-core (vendored bundle) | Node binding: index builder plus AI proxy |
| `scolta-next` | TypeScript | scolta-node | Next.js adapter |
| `scolta-nuxt` | TypeScript | scolta-node | Nuxt adapter |
| `scolta-astro` | JS/TS | scolta-node | Astro adapter |

### What is shared, what each binding owns

`scolta-core` compiles to WASM that runs in the browser. Scoring, query expansion parsing, sanitizing,
stop words, context extraction and conversation trimming happen there, once, in the front end every
package ships. Nothing server-side loads that WASM. scolta-php removed its server-side WASM path, and
`HealthChecker` still reports "Server-side WASM removed, HTML processing is now pure PHP".

Each binding implements its whole server side natively. There is no shared runtime between them:

| Concern | `scolta-php` | `scolta-python` | `scolta-node` |
|---|---|---|---|
| HTML cleaning | `src/Html/HtmlCleaner.php`, `src/Html/PagefindHtmlBuilder.php` | `src/scolta/html.py` | `src/html.ts` |
| Index building | `src/Index/PhpIndexer.php` and the rest of `src/Index/` | `src/scolta/index/` | `src/index/` |
| Tokenizing and stemming | `src/Index/Tokenizer.php`, vendored `src/Index/Snowball/` | vendored `src/scolta/index/snowball/` | `src/index/stemmer.ts` over a compiled `src/index/stemmer-wasm/` |
| Server-side AI and prompt text | `src/AiProvider/`, `src/Prompt/DefaultPrompts.php` | `src/scolta/ai/prompts.py` | `src/ai/prompts.ts` |

`scolta-php` is the reference implementation, and the ports say so in their own `CLAUDE.md`: read the PHP
source before porting a piece. `scolta-node` follows it closely, using `scolta-python` as the structural
model.

Parity between the three is held by tests, not by shared code:

- The prompt text is authored in scolta-core's `src/prompts.rs`. Each binding carries a mirror and runs a
  prompt-text identity test in CI against a live checkout of this repo, pointed at by the
  `SCOLTA_CORE_PROMPTS` environment variable. A mirror that drifts turns that binding red.
- Stemming is pinned to one crate version in all three ports (§3, "Stemming is pinned across every port").
- scolta-python and scolta-node assert against golden files generated from the real scolta-php classes.
  `parity/` in scolta-python regenerates the HTML, tokenizer and index fixtures by running scolta-php;
  scolta-node's `tests/html-parity.test.ts` asserts the same HTML fixture set. The fixtures are
  committed, so both suites run without PHP in CI.

The adapters are platform glue. None of them implements scoring, HTML cleaning, indexing, tokenizing or
prompt logic, and none depends on `scolta-core` directly: a PHP adapter depends on `scolta-php`, a
TypeScript adapter on `scolta-node`, the Django adapter on `scolta-python`. An adapter can still change
what an index contains, because it decides which content is exported and how the build runs.

## 2. Where each package publishes

Each package publishes to the registry its platform expects, not to one shared place. Read the table for
the registry name to check after a release, and for which publishes are manual.

| Package | Registry | Name | Notes |
|---|---|---|---|
| `scolta-core` | crates.io | `scolta-core` | The WASM tarball is also attached to the GitHub release. That tarball is the only prebuilt WASM. `cargo publish` is a manual step: the release workflow builds and attaches the tarball, and does not publish the crate. |
| `scolta-php` | Packagist | `tag1/scolta-php` | Every PHP adapter depends on this. |
| `scolta-laravel` | Packagist | `tag1/scolta-laravel` | |
| `scolta-wp` | Packagist and wordpress.org | `tag1/scolta-wp` / `scolta-ai-search` | wordpress.org is a separate SVN publish of a reviewed zip. |
| `scolta-drupal` | drupal.org only | `drupal/scolta` | Published through packages.drupal.org with a manual release node. Not Packagist: a `tag1/scolta-drupal` there would collide with the module name. |
| `scolta-python` | PyPI | `scolta` | Flat namespace. Ownership is shown by the PyPI org, not a vendor prefix. No publish workflow exists yet; see §4. |
| `scolta-django` | PyPI | `scolta-django` | Same: no publish workflow yet. |
| `scolta-node` | npm | `scolta` | |
| `scolta-next` | npm | `scolta-next` | |
| `scolta-nuxt` | npm | `scolta-nuxt` | |
| `scolta-astro` | npm | `scolta-astro` | |

A release is finished when the registry actually serves the new version, not when the tag exists. A tag
whose webhook failed never reaches users. Only ever push to GitHub. Never push to GitLab: it breaks the
pull mirror.

## 3. Version numbers

- Each package versions independently from its own git tags: major, minor and patch all move per
  package, and no check compares one package's version number against another's. Compatibility between
  packages is the dependency constraint an adapter declares for its upstream, not matching numbers
  ([VERSIONING.md](VERSIONING.md)). A package bumps its own major when its own public API breaks;
  nothing else is obliged to follow.
- Keep exactly one unreleased dev line at a time. Right after you tag a release, bump to the next dev
  suffix (PHP and Rust use `X.Y.Z-dev`, Python uses `X.Y.Z.dev0`) and add an
  `## [X.Y.Z] - Unreleased` stub to the CHANGELOG.
- When you open a new cycle, move `extra.branch-alias` (`dev-main` to `X.Y.x-dev`) in the same commit as
  the version bump. If the alias lags, the new minor can't be reached by its own `^X.Y` constraint.
  This is the break the coherence check exists to catch (§6).

### Where the version lives

| Package | Version lives in |
|---|---|
| `scolta-core` | `Cargo.toml` |
| `scolta-php`, `scolta-laravel` | `composer.json` `version` |
| `scolta-drupal` | `scolta.info.yml`, and nowhere else |
| `scolta-wp` | the plugin header `Version:` in `scolta.php` (the source), the `SCOLTA_VERSION` constant, and `readme.txt` `Stable Tag`: all three must match, and `scripts/plugin-version.sh` is what CI reads |
| `scolta-python`, `scolta-django` | `src/<package>/__init__.py` `__version__`, single-sourced into the package metadata by `[tool.hatch.version]` in `pyproject.toml` |
| `scolta-node`, `scolta-next`, `scolta-nuxt`, `scolta-astro` | `package.json` |

The `version` key in `composer.json` is not uniform, and the difference is deliberate. `scolta-php` and
`scolta-laravel` declare one, and their CI validates its format. `scolta-drupal` and `scolta-wp` must
never declare one, and their CI hard-fails if one appears: a declared version overrides the version
Composer derives from the branch or tag, Packagist ignores that but the drupal.org Composer facade
honours it, and a site tracking a dev branch could then `composer update` but never `composer install`
from the resulting lock. That broke a client build on 2026-07-27. Neither Drupal nor WordPress needs it: drupal.org injects the version into
`scolta.info.yml` at packaging time, and WordPress reads the plugin header.

`@since` annotations in source are a third place a version appears, and no check compares them to the
version file. Sweeping them when you rename the line is a human step (each repo's `CLAUDE.md` requires
`@since` on new public API).

### Stemming is pinned across every port

Every port stems queries the way Pagefind does. scolta-core has no stemmer: stemming is Pagefind's, and
each binding matches it independently rather than depending on whatever its own ecosystem's stemmer
library ships. `scolta-php` vendors generated Snowball stemmers and records the pin in
`src/Index/Snowball/PROVENANCE.md`, guarded by a concordance fixture checked against `BUNDLED_VERSION`
in CI. `scolta-node` compiles the pinned crate itself under `tools/stemmer-wasm` and keeps a golden
corpus with its own provenance file. `scolta-python` vendors generated Snowball stemmers too, because no
published `snowballstemmer` release reproduces the crate byte for byte. When the pinned version moves, it
moves in all three, together, or the ports stop agreeing on what a query matches.

## 4. Release order

Release in dependency order. The registry rule (a package's registry has to serve the new version before
anything that depends on it tags) binds steps 2 through 4. scolta-core goes first for a different reason:
nothing in step 2 reaches it through a registry, but the browser bundle has to be rebuilt and re-vendored
into the carriers before they tag. Full steps, and what a stale bundle costs, are in
[RELEASING.md](RELEASING.md).

1. `scolta-core` (tag it by hand, then publish the crate by hand).
2. `scolta-php` to Packagist, then `scolta-drupal`, `scolta-laravel`, `scolta-wp` (these three can go in
   parallel).
3. `scolta-python`, then `scolta-django`.
4. `scolta-node`, then `scolta-next`, `scolta-nuxt`, `scolta-astro`.

An adapter's `X.Y.0` cannot ship before its library's `X.Y.0` exists. For the three PHP adapters the
`lock-guard` job in `release.yml` enforces that mechanically: it refuses to publish while the committed
lock names a development version of `tag1/scolta-php`.

How much of this is automated differs by ecosystem, so check before you assume a tag ships anything. The
npm packages publish from a tag through Trusted Publishing. The PHP packages publish through Packagist's
webhook on the tag. scolta-core's tag builds and attaches the WASM tarball but does not run
`cargo publish`. `scolta-python` and `scolta-django` have no release workflow at all: they are built and
uploaded by hand.

## 5. Rules that apply to every repo

- Push to `tag1consulting/<pkg>`, never a personal fork. Check `git remote get-url origin` before every
  tag push: a tag pushed to a fork fires no workflows.
- Move `extra.branch-alias` in the commit that opens a new dev cycle (§3).
- Respect each repo's rule about the `composer.json` `version` key (§3). It differs by package.
- A red `main` blocks the tag on that repo. If you think a red is an infrastructure fluke, reproduce it
  in a throwaway worktree at the failing commit with real credentials before you dismiss it.
- A release is finished when the registry serves it, not when the tag exists (§2).
- Never build a release zip locally. Let CI build it: a local build pulls in a filesystem `vendor/`.
- Only push to GitHub. Never push to GitLab.
- A docs-only pull request does not need a CHANGELOG entry. Every repo's `CLAUDE.md` says the entry is
  required when a PR changes code, and the CHANGELOG jobs in CI are scoped to code paths to match.

## 6. The fleet checks

Maintainer-only: these live in the private `scolta-fleet` repo. There are two layers, and neither one
covers the other:

- **Per package, against itself.** `checkPackage` asks whether one package contradicts itself across
  `composer.json` `version`, `composer.json` `extra.branch-alias.dev-main`, a Drupal `.info.yml`, and a
  WordPress plugin header. It is decidable from one checkout, so it ships as the coherence bundle
  (`scripts/check-coherence.mjs`) vendored into the four PHP package repos, where each runs it as its
  own `Version coherence` CI job. That job is public and runs on every pull request, including yours.
  The bundle is generated: edit the TypeScript in scolta-fleet and run `npm run vendor:coherence`. Never
  hand-edit a vendored copy. The Python and TypeScript packages do not run this check today.
- **The workspace.** `src/coherence-cli.ts --packages DIR --demos DIR` needs every repo checked out side
  by side, so it runs in scolta-fleet only. It catches an adapter expressing the `tag1/scolta-php`
  dependency in a way that stops resolving when the branch alias moves, and a demo pinning a package one
  of its own dependencies already provides. It runs nightly on a schedule, not on scolta-fleet pull
  requests, because it reports on eleven other repos' default branches.

Two scripts reproduce the numbers you should never write down: `scripts/adapter-floor-matrix.sh` (the
root-vs-adapter resolution table) and `scripts/version-resolution-matrix.sh` (what a caret range accepts
after an alias moves). Run them; don't quote their output from memory.

Two files hold the data. `fleet.json` defines the demos: one row each, with the package line, the API
base, the build runner and the paths. `baselines.json` holds the blessed numbers: the count rows with
their tolerances and blessing dates, and a fragment count per demo. Fragment counts appear in both
files and the harness refuses to start when they disagree, so change them together. Never re-bless a
count without first checking where it came from.

## 7. The demos and the regression tests

The demo repos are not packages. They have no tags, no registry, and no release workflow.

The regression harness (maintainer-only, in scolta-fleet, written in TypeScript) builds each demo's
index and checks the fragment count, the shape of each endpoint's response, and that every result and
citation URL returns 200. It does not check summary quality, sort-intent correctness, or whether the AI
grounded its answer: a person checks those. Full method is in [REGRESSION.md](REGRESSION.md).

A public record of what each query returns, release over release, is proposed in
[regression/](regression/): the query set, the deterministic index results, property checks on the AI
answer, and one sampled answer per release. It is a spec today, with nothing captured. See
[regression/README.md](regression/README.md).

## What a contributor without fleet access can do

Most of §6 and §7 needs the private repo. Everything a public pull request is judged on does not:

- Each PHP package's `Version coherence` job, its `assets-in-sync` job where it has one, and its whole
  test suite run on your pull request. A green run there is the evidence that matters.
- If you change indexing, cleaning or tokenizing in a binding, run that repo's parity fixtures and say
  in the PR which ones you ran and whether any fixture had to be regenerated.
- If you change prompt text, change it in scolta-core first and say so; the mirrors follow in a second
  pull request.
- If you believe a change moves result counts on a live site, say which query and in which direction. A
  maintainer runs the fleet harness and posts the report filename on the PR.

## Still open

1. `scripts/validate-release.php` in scolta-drupal. It exists and runs in CI today, but advisory only:
   `version-consistency` invokes it behind `|| echo`, because a development branch legitimately carries a
   `-dev` version. Either it becomes the per-repo release gate that reads the platform's version file, or
   the fleet coherence check takes over its job. The scolta-drupal release section stays a stub until
   this is decided. (`scolta-wp` carries the same script, wired the same way.)
2. `scolta-python` and `scolta-django` have no release workflow, so a PyPI publish is a manual
   `uv build` plus `twine upload` with no gate in front of it.
3. Neither Python package declares trove classifiers, so `Framework :: Django` and `Framework :: Wagtail`
   are absent and the Django Packages and Wagtail directories cannot find `scolta-django`.
4. `scolta-node` and `scolta-python` commit a copy of the browser bundle with no CI check that the copy
   still matches scolta-php. See [ASSETS.md](ASSETS.md).
5. The public regression corpus under [regression/](regression/) is a spec with nothing captured yet. It
   becomes a release step once capture mode and a first measured corpus land, and not before.

---

## scolta-core (this repo)

The facts specific to scolta-core, kept here because the shared guide already lives in this repo.

**What it is.** A Rust crate compiled to WASM: the scoring, query-expansion, sanitize, and stop-word
engine that runs in the browser bundle every adapter ships, plus the default prompt templates
(`src/prompts.rs`) and the expansion rules (`src/expansion.rs`). It depends on nothing else in Scolta;
everything else compiles or vendors it. It has no indexer, no HTML cleaner and no stemmer: those are the
bindings' (§1).

**Where the version lives.** `Cargo.toml`.

**Where it publishes.** crates.io, as `scolta-core`. To confirm it published:
`curl -s https://crates.io/api/v1/crates/scolta-core | jq .crate.newest_version`.

**CI checks.** Six jobs in `.github/workflows/ci.yml`: `Lint` (`cargo fmt --check`, clippy on the host
and on the `wasm32` target, all with `-D warnings`); `Unit tests` (`cargo test`); `WASM build`
(`wasm-pack build --target web`, an output-file check and a binary size cap); `Release tarball content
sweep` (builds the tarball and runs `scripts/validate-tarball.sh`, a fail-closed allowlist that rejects
wasm-pack drift cruft); `CHANGELOG enforcement` (pull requests only, and only when `src/`, `Cargo.toml`
or `tests/` changed); and `version-check`, a format check on `Cargo.toml`. There is no check that
enforces the lifecycle rules in [VERSIONING.md](VERSIONING.md); the pull request template asks for them
by hand.

**On release day.** Tag it by hand. The PHP release scripts loop over `composer.json` files and skip
`Cargo.toml`, so core has been left a release behind twice (0.3.6 and 0.3.7). The tag builds the WASM,
validates the tarball, extracts the CHANGELOG entry, and attaches the tarball to a GitHub release. It
does not publish the crate: run `cargo publish --dry-run` and then `cargo publish` yourself. A first
publish also needs a metadata check.

**Watch out for.** The prompt text and the expansion rules are authored here, and all three bindings
carry mirrors of them. `scolta-php`, `scolta-node` and `scolta-python` each run a prompt-text identity
test in CI against this repo's `src/prompts.rs`, checked out live and pointed at by a
`SCOLTA_CORE_PROMPTS` environment variable. A change here that the mirrors don't match turns those three
repos red, so a prompt change ships here first and the mirrors follow. [VERSIONING.md](VERSIONING.md)
lives here too; keep its package list at eleven.
