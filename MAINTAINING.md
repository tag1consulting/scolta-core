# MAINTAINING — Scolta

This is the maintenance guide for all Scolta projects. It lives in scolta-core because that repo is
public, so every package can link to it. If a fact is true for more than one repo, it lives here. If a
fact is true for only one repo, it lives in that repo's own `MAINTAINING.md`, except scolta-core's own
facts, which are the last section of this file.

The other guides here in scolta-core: `RELEASING.md` (how to cut a release), `REGRESSION.md` (the tests
to run before you ship), `ASSETS.md` (the browser bundle that gets copied into some packages),
`VERSIONING.md` (the compatibility contract users read), and the regression corpus under `regression/`.

The tooling these guides describe (the harness, the coherence checks, the demo manifest, the blessed
counts) lives in the `scolta-fleet` repo, which is private. This guide is the public description of how
it all fits together; scolta-fleet is where you run it.

One rule to keep this from going stale: don't write version numbers, floors, or lock states into these
docs. They are out of date within a week. The scripts that print them are not. So a doc says "run the
floor-matrix script," not "the floor is 1.2." Read live numbers from the tool, never from prose.

## 1. The packages

There are eleven packages, a private fleet repo (`scolta-fleet`) that holds the cross-repo tooling, and a
set of demo repos (§7).

| Package | Language | Depends on | What it is |
|---|---|---|---|
| `scolta-core` | Rust → WASM | (nothing) | The scoring, query-expansion, sanitize and stop-word engine. Compiles to the browser bundle. |
| `scolta-php` | PHP | scolta-core | The reference server library: index builder plus AI proxy. Owns the canonical browser bundle. |
| `scolta-laravel` | PHP | scolta-php | Laravel adapter |
| `scolta-drupal` | PHP | scolta-php | Drupal module |
| `scolta-wp` | PHP | scolta-php | WordPress plugin |
| `scolta-python` | Python | scolta-core | Python binding: index builder plus AI proxy |
| `scolta-django` | Python | scolta-python | Django / Wagtail adapter |
| `scolta-node` | TypeScript | scolta-core | Node binding: index builder plus AI proxy |
| `scolta-next` | TypeScript | scolta-node | Next.js adapter |
| `scolta-nuxt` | TypeScript | scolta-node | Nuxt adapter |
| `scolta-astro` | JS/TS | scolta-node | Astro adapter |

`scolta-php` is the reference implementation, and the ports say so in their own `CLAUDE.md`: read the PHP
source before porting a piece. `scolta-node` follows it closely, using `scolta-python` as the structural
model. Most of the shared logic is not ported three times: it runs once in the WASM that every package
vendors.

The adapters are glue only. None of them reimplements scoring, HTML cleaning, indexing, tokenizing or
prompt logic, and none of them depends on `scolta-core` directly: a PHP adapter depends on `scolta-php`,
a TypeScript adapter on `scolta-node`, the Django adapter on `scolta-python`.

## 2. Where each package publishes

Each package publishes to the registry its platform expects, not to one shared place. This is the part
people get wrong, so it is spelled out.

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

- The major version moves together across all packages. Minor and patch move on their own, per package.
  A new major is a change to five or more repos at once.
- Keep exactly one unreleased dev line at a time. Right after you tag a release, bump to the next dev
  suffix (PHP and Rust use `X.Y.Z-dev`, Python uses `X.Y.Z.dev0`) and add an
  `## [X.Y.Z] - Unreleased` stub to the CHANGELOG.
- When you open a new cycle, move `extra.branch-alias` (`dev-main` → `X.Y.x-dev`) in the same commit as
  the version bump. If the alias lags, the new minor can't be reached by its own `^X.Y` constraint.
  This is the break the coherence check exists to catch (§6).

### Where the version lives

| Package | Version lives in |
|---|---|
| `scolta-core` | `Cargo.toml` |
| `scolta-php`, `scolta-laravel` | `composer.json` `version` |
| `scolta-drupal` | `scolta.info.yml`, and nowhere else |
| `scolta-wp` | the plugin header `Version:` in `scolta.php` (the source), the `SCOLTA_VERSION` constant, and `readme.txt` `Stable Tag`: all three must match |
| `scolta-python`, `scolta-django` | `src/<package>/__init__.py` `__version__`, single-sourced into the package metadata by `[tool.hatch.version]` in `pyproject.toml` |
| `scolta-node`, `scolta-next`, `scolta-nuxt`, `scolta-astro` | `package.json` |

**The `version` key in `composer.json` is not uniform, and the difference is deliberate.**
`scolta-php` and `scolta-laravel` declare one, and their CI validates its format. `scolta-drupal` and
`scolta-wp` must never declare one, and their CI hard-fails if one appears: a declared version overrides
the version Composer derives from the branch or tag, Packagist ignores that but the drupal.org Composer
facade honours it, and a site tracking a dev branch could then `composer update` but never
`composer install` from the resulting lock. Neither Drupal nor WordPress needs it: drupal.org injects the
version into `scolta.info.yml` at packaging time, and WordPress reads the plugin header.

`@since` annotations in source are a third place a version appears, and no check compares them to the
version file. Sweeping them when you rename the line is a human step (each repo's `CLAUDE.md` requires
`@since` on new public API).

### Stemming is pinned across every port

Every port stems queries the way Pagefind does, and they are pinned to the same crate version rather
than to whatever their ecosystem's stemmer library ships. `scolta-php` guards it with a concordance
fixture checked against `BUNDLED_VERSION` in CI; `scolta-node` compiles the crate itself under
`tools/stemmer-wasm` and keeps a golden corpus; `scolta-python` vendors generated Snowball stemmers
because no published `snowballstemmer` release reproduces the crate byte for byte. When the pinned
version moves, it moves in all three, together, or the ports stop agreeing on what a query matches.

## 4. Release order

Release in dependency order. The point that matters is that a package's registry has served the new
version before anything that depends on it tags. Full steps are in `RELEASING.md`.

1. `scolta-core` (tag it by hand, then publish the crate by hand).
2. `scolta-php` → wait for Packagist → then `scolta-drupal`, `scolta-laravel`, `scolta-wp` (these three
   can go in parallel).
3. `scolta-python` → then `scolta-django`.
4. `scolta-node` → then `scolta-next`, `scolta-nuxt`, `scolta-astro`.

An adapter's `X.Y.0` cannot ship before its library's `X.Y.0` exists. For the three PHP adapters the
`lock-guard` job in `release.yml` enforces that mechanically: it refuses to publish while the committed
lock names a development version of `tag1/scolta-php`.

**How much of this is automated differs by ecosystem, so check before you assume a tag ships anything.**
The npm packages publish from a tag through Trusted Publishing. The PHP packages publish through
Packagist's webhook on the tag. scolta-core's tag builds and attaches the WASM tarball but does not run
`cargo publish`. `scolta-python` and `scolta-django` have no release workflow at all: they are built and
uploaded by hand.

## 5. Rules that apply to every repo

- Push to `tag1consulting/<pkg>`, never a personal fork. Check `git remote get-url origin` before every
  tag push: a tag pushed to a fork fires no workflows.
- Move `extra.branch-alias` in the commit that opens a new dev cycle (§3).
- Respect each repo's rule about the `composer.json` `version` key (§3). It differs by package.
- A red `main` blocks the tag on that repo. If you think a red is an infrastructure fluke, reproduce it
  in a throwaway worktree at the failing commit with real credentials before you dismiss it. Don't assume.
- A release is finished when the registry serves it, not when the tag exists (§2).
- Never build a release zip locally. Let CI build it: a local build pulls in a filesystem `vendor/`.
- Only push to GitHub. Never push to GitLab.
- Don't mention Claude, an AI, or automated authorship anywhere: commits, PR text, changelogs, comments.
- A docs-only pull request does not need a CHANGELOG entry. Every repo's `CLAUDE.md` says the entry is
  required when a PR changes code, and the CHANGELOG jobs in CI are scoped to code paths to match.

## 6. The fleet checks

These live in the `scolta-fleet` repo. There are two layers, and neither one covers the other:

- **Per package, against itself.** `checkPackage` asks whether one package contradicts itself across
  `composer.json` `version`, `composer.json` `extra.branch-alias.dev-main`, a Drupal `.info.yml`, and a
  WordPress plugin header. It is decidable from one checkout, so it is bundled into a dependency-free
  file and vendored into the four PHP package repos as `scripts/check-coherence.mjs`, where each runs it
  as its own `Version coherence` CI job. The bundle is generated: edit the TypeScript in scolta-fleet and
  run `npm run vendor:coherence`. Never hand-edit a vendored copy. The Python and TypeScript packages do
  not run this check today.
- **The workspace.** `src/coherence-cli.ts --packages DIR --demos DIR` needs every repo checked out side
  by side, so it runs in scolta-fleet only. It catches an adapter expressing the `tag1/scolta-php`
  dependency in a way that stops resolving when the branch alias moves, and a demo pinning a package one
  of its own dependencies already provides. It runs nightly on a schedule, not on scolta-fleet pull
  requests, because it reports on eleven other repos' default branches.

Two scripts reproduce the numbers you should never write down: `scripts/adapter-floor-matrix.sh` (the
root-vs-adapter resolution table) and `scripts/version-resolution-matrix.sh` (what a caret range accepts
after an alias moves). Run them; don't quote their output from memory.

Two files hold the data: `fleet.json` (one row per demo) and `baselines.json` (blessed counts, each row
naming the run that produced it). Never re-bless a count without first checking where it came from.

## 7. The demos and the regression tests

The demo repos are not packages. They have no tags, no registry, and no release workflow.

The regression harness (in scolta-fleet, written in TypeScript) builds each demo's index and checks the
fragment count, the shape of each endpoint's response, and that every result and citation URL returns
200. It does not check summary quality, sort-intent correctness, or whether the AI grounded its answer:
a person checks those. Full method is in `REGRESSION.md`.

The public record of what each query returns, release over release, is the corpus in `regression/`. It
holds the query set, the deterministic index results (which are stable and diffable), property checks on
the AI answer, and one sampled answer per release. That corpus is how someone outside the team can see
whether a release made a query worse. See `regression/README.md`.

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
   still matches scolta-php. See `ASSETS.md`.

---

## scolta-core (this repo)

The facts specific to scolta-core, kept here because the shared guide already lives in this repo.

**What it is.** A Rust crate compiled to WASM: the scoring, query-expansion, sanitize, and stop-word
engine that runs in the browser bundle every adapter ships, plus the default prompt templates
(`src/prompts.rs`) and the expansion rules (`src/expansion.rs`). It depends on nothing else in Scolta;
everything else compiles or vendors it.

**Where the version lives.** `Cargo.toml`.

**Where it publishes.** crates.io, as `scolta-core`. To confirm it published:
`curl -s https://crates.io/api/v1/crates/scolta-core | jq .crate.newest_version`.

**CI checks.** `Lint` (`cargo fmt --check`, clippy on the host and on the `wasm32` target, all with
`-D warnings`); `Unit tests` (`cargo test`); `WASM build` (`wasm-pack build --target web`, an
output-file check and a binary size cap); `Release tarball content sweep` (builds the tarball and runs
`scripts/validate-tarball.sh`, a fail-closed allowlist that rejects wasm-pack drift cruft);
`CHANGELOG enforcement` (pull requests only, and only when `src/`, `Cargo.toml` or `tests/` changed);
and a version-format check on `Cargo.toml`.

**On release day.** Tag it by hand. The PHP release scripts loop over `composer.json` files and skip
`Cargo.toml`, so core has been left a release behind twice (0.3.6 and 0.3.7). The tag builds the WASM,
validates the tarball, and attaches it to a GitHub release. It does not publish the crate: run
`cargo publish --dry-run` and then `cargo publish` yourself. A first publish also needs a metadata check.

**Watch out for.** The prompt text and the expansion rules are authored here, and the bindings carry
mirrors of them: `scolta-node` and `scolta-python` each run a prompt-identity test in CI against this
repo's `src/prompts.rs`, checked out live. A change here that the mirrors don't match turns those repos
red, so a prompt change ships here first and the mirrors follow. `VERSIONING.md` lives here too; keep its
package list at eleven.
