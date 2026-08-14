# RELEASING — how to cut a release

How to release across all the Scolta packages. Read `MAINTAINING.md` first for the package list, where
each one publishes, and the version rules. The tests to run before you ship are in `REGRESSION.md`. The
tooling these steps call (the harness, the coherence checks) lives in the private `scolta-fleet` repo.

For every package, the release is finished when the registry serves the new version, not when the tag
exists.

## Order

Release in dependency order. Each step waits for the previous registry to serve the new version before
the packages that depend on it tag.

1. `scolta-core` (tag by hand, then `cargo publish` by hand).
2. `scolta-php` → wait for Packagist → then `scolta-drupal`, `scolta-laravel`, `scolta-wp` in parallel.
3. `scolta-python` → then `scolta-django`.
4. `scolta-node` → then `scolta-next`, `scolta-nuxt`, `scolta-astro`.

## Before you tag anything

1. Every target `main` is green. A red `main` blocks that repo's tag. If you think a red is an
   infrastructure fluke, reproduce it in a throwaway worktree at the failing commit with real
   credentials before you dismiss it.
2. Run the regression tests (`REGRESSION.md`) and note the report filename.
3. In scolta-fleet, run the workspace coherence check
   (`npx tsx src/coherence-cli.ts --packages DIR --demos DIR`) with the siblings checked out. Each
   package's own `Version coherence` CI job covers the per-package half. Fix any drift first.
4. On every repo, `git remote get-url origin` reads `tag1consulting/<pkg>`. A tag pushed to a fork fires
   no workflows.
5. Check that the last release actually reached every registry. A tag whose webhook failed never
   shipped: publish it before you cut a new one.

## Releasing one package

For each package: bump the version → write a `## [X.Y.Z]` CHANGELOG entry with the date → merge to
`main` → tag from the release merge commit (not `main`, which already carries `-dev`) → push only the
tag → wait for the registry → bump to the next `-dev` and re-open the CHANGELOG `Unreleased` stub.

Tags are annotated and named `vX.Y.Z`. Copy the CHANGELOG entry into the GitHub release notes as-is;
where a release workflow exists it extracts that section itself, so a missing or misnamed heading ships
empty notes.

**The tag filter is not the same everywhere.** scolta-core matches `v[0-9]+.[0-9]+.[0-9]+*` (the
trailing `*` catches `-rc` and `-beta`). The four PHP repos match that and the same glob without the `v`,
because drupal.org needs an unprefixed tag. The npm repos match `v*.*.*`. `scolta-python` and
`scolta-django` have no release workflow, so a tag there triggers nothing at all.

### scolta-core (crates.io)
Bump `Cargo.toml`. Run `cargo publish --dry-run` (a first publish also needs a metadata check). Tag. CI
builds the WASM with `wasm-pack`, validates the tarball against the allowlist in
`scripts/validate-tarball.sh`, and attaches it to the GitHub release: that tarball is the only prebuilt
WASM. Then run `cargo publish` yourself, because the workflow does not. Check it served:
`curl -s https://crates.io/api/v1/crates/scolta-core | jq .crate.newest_version`.

### scolta-php (Packagist)
Bump `composer.json`; if you are opening a cycle, move `extra.branch-alias` in the same commit. Tag,
then wait for Packagist. If it hasn't updated in 15 minutes, stop: that's a webhook or outage problem,
and you must not tag the adapters until it resolves. Check it served:
`composer clear-cache && composer show tag1/scolta-php -a | grep versions`.

### scolta-laravel (Packagist)
Set `tag1/scolta-php` to `^X.Y.0` at the version you just shipped (drop any `@dev`), then re-lock. Bump,
tag, check Packagist. `lock-guard` in `release.yml` refuses to publish while the lock names a development
version of scolta-php. Laravel keeps no copy of the bundle; its `vendor:publish` staleness check is a
regression test, not a release step (`REGRESSION.md`).

### scolta-drupal (drupal.org)
> This section depends on the `scripts/validate-release.php` decision that is still open
> (`MAINTAINING.md`, Still open #1). Finish it once that's settled. The known steps:

Bump `scolta.info.yml` (the real version file). Push two tags: `vX.Y.Z` on GitHub, and `X.Y.Z` (no `v`)
for drupal.org, which ignores `v`-prefixed tags. `git fetch origin`, then
`git push drupal origin/main:X.Y.Z`. Fast-forward the devel branch: `git push drupal origin/main:1.0.x`
(skip this and the branch sits stale for a whole cycle). Create the release node by hand on drupal.org:
the notes must be HTML, not markdown, so draft them for review rather than pasting markdown. A new minor
branch needs its release node before `composer require drupal/scolta:N.M.x-dev` will resolve. Check the
packages.drupal.org page shows the version. The GitHub tag itself only cuts a GitHub release; nothing
about drupal.org is automated.

### scolta-wp (Packagist and wordpress.org)
Bump all three version places (plugin header, `SCOLTA_VERSION`, `readme.txt` `Stable Tag`) and grep to
confirm they match; `scripts/plugin-version.sh` is what CI reads. Set `Tested up to:` in `readme.txt` to
the current WP core version or higher, and add the matching `readme.txt` changelog entry: both are gates
in `release.yml` (`check-wp-version`, `check-readme-changelog`), not PR checks, so they first fire at the
tag. Tag → Packagist. wordpress.org is a separate publish: the reviewed dist zip is what ships, and
wp.org rejects a non-numeric version, so never publish from a `-dev` or `-rc` commit. Don't build the zip
locally; CI builds it and `scripts/validate-dist.sh` must pass. SVN steps: update `trunk` from the zip,
`svn cp trunk tags/X.Y.Z`, then bump `Stable Tag`. The wp.org slug is `scolta-ai-search`. Check the
wp.org plugin page shows the version.

### scolta-python (PyPI, published as `scolta`)
Bump `src/scolta/__init__.py` `__version__`. That is the only place: `pyproject.toml` reads it through
`[tool.hatch.version]`, so the two cannot drift. Rehearse on TestPyPI: `uv build` →
`uvx twine check dist/*` → upload to testpypi → `pip install` from testpypi in a clean venv. **There is
no release workflow**, so the real upload is manual too: `uv build` then `twine upload dist/*`. CI's
`dist` job builds and validates the same artifacts on every pull request, which is the only gate in
front of the upload. Tag for the record. Check: `pip install scolta` in a fresh venv resolves the new
version.

### scolta-django (PyPI, published as `scolta-django`)
Release only after `scolta` is up. Same Python steps, same manual upload, same single version source
(`src/scolta_django/__init__.py`).

### scolta-node / -next / -nuxt / -astro (npm)
Release `scolta-node` first (its npm name is `scolta`); the adapters require `scolta@^X.Y.0`. Bump
`package.json`. Run `npm run check:pack` and `npm run check:publish` locally, the same guards the release
workflow runs: the tarball must stay inside the `files` allowlist and under its size cap, and must not
include a leftover `file:` or `link:` dependency. Tag; CI runs the build, the tests and both guards, then
`npm publish` through Trusted Publishing (OIDC), which attaches provenance automatically. Check:
`npm install` in a throwaway directory resolves from the registry, not a local path.

## Things that go wrong

- **Wait for the registry, not a fixed delay.** Tagging a dependent before its dependency is served on
  the registry locks in stale code.
- **Composer blocks on an advisory.** Composer 2.9 `audit.block-insecure` refuses a version with an open
  advisory during `composer update`. Fix it with `config.audit.ignore`, classifying each advisory as
  permanent, temporary, or dev-only. Test against the Composer you actually have installed, not the docs
  website.
- **A GitHub rate-limit error that looks like an advisory** (`Could not authenticate against github.com`,
  exit 100). It's the anonymous 60/hour/IP limit, not an advisory. Only the jobs that rewrite constraints
  fail; the others pass. Fix it with a workflow-level `env: COMPOSER_AUTH` using `GITHUB_TOKEN` (raises
  it to 1000/hour). Don't add a `permissions:` block.
- **`--no-dev` still resolves the whole graph.** A dev-only transitive dependency that raises its PHP
  floor breaks `composer update` even though nothing at runtime changed. Bump the workflow's PHP version.
- **Don't gitignore `composer.lock`.** Composer refuses a partial `--no-dev` update without one.
- **A CI job that rewrites the scolta-php constraint before resolving tests nothing.** The parity and
  upstream jobs do exactly that on purpose (they resolve `dev-main`); check for the rewrite before you
  read a green adapter build as a statement about the released line.
- **Drupal push traps.** Push `origin/main` after `git fetch`: your local `main` is a stale bookmark.
  Confirm the mirror actually moved with `git ls-remote --heads`; "Everything up-to-date" usually means
  the PR wasn't merged. Never put a trailing `# comment` on a push line: zsh sends the `#` as a refspec.
- **Don't build zips locally.** A path repo mirrors the filesystem, not the git tree, so `.gitignore`
  and `.gitattributes` don't apply and a nested `vendor/` gets dragged in. Fix the workflow and re-tag.
- **A missed prerequisite is not a gate failure.** If a dependency PR is green but unmerged, or the demos
  aren't rebuilt, stop before measuring: a warm cache can report a false green. A gate that actually ran
  and failed is a code problem. Don't treat them the same.
- **Serving the right bytes doesn't mean the browser ran them.** A demo's `composer update` can silently
  skip the asset copy. The browser regression tests are not optional (`REGRESSION.md`).

## Keep the release clean

Don't fix unrelated things during a release. If a prerequisite, doc, or test fails, stop and report it
rather than working around it. Every release ends when the registry serves the new version, confirmed.
