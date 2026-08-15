# The browser bundle and how it stays in sync

For anyone changing the browser bundle or wondering why a package carries a copy of it. Read it before
you edit anything under an `assets/` directory in any Scolta repo.

`scolta-php` owns the canonical copy of the browser bundle. Some packages ship their own committed copy
of it; others read it from their Composer vendor directory at run time. Which packages do which is set by
how each platform installs, not by preference. The copies drift silently if nothing watches them, so CI
checks some of them, and the gaps are named below.

## The canonical files

They live in `scolta-php/assets/` (the WASM is compiled from `scolta-core`, this repo). Four files are
the bundle proper:

```
scolta-php/assets/js/scolta.js
scolta-php/assets/css/scolta.css
scolta-php/assets/wasm/scolta_core.js
scolta-php/assets/wasm/scolta_core_bg.wasm
```

`assets/pagefind/` beside them holds the Pagefind runtime (`pagefind.js`, `pagefind-worker.js`, and the
two `*.pagefind` WASM blobs). The Drupal and WordPress copies do not include it; the node and Python
copies do, because their published artifacts ship the whole runtime.

Checksums for the four live in `scolta-php/assets/ASSETS.sha256`, one line per file. There is also a
standalone `assets/js/scolta.js.sha256`. It is not a second checksum: it is extracted from the manifest,
so the hash is computed in one place. Regenerate both together with `composer update-js-checksum` in
scolta-php, never by hashing the file again by hand. It exists because scolta-laravel reads that bare
hash at run time.

## Which packages carry a copy, and why

Composer runs a package's `post-install-cmd` and `post-update-cmd` scripts only for the root package,
never for a dependency. So a site that installs an adapter never gets the assets copied out of `vendor/`
at install time. If the install can't copy the file, the file has to be committed already. That is what
splits the table below: read it for whether the package you are changing carries a copy, and where.

| Package | Carries a copy? | Committed at | Why |
|---|---|---|---|
| `scolta-drupal` | yes | `js/scolta.js`, `css/scolta.css`, `js/wasm/scolta_core*.{js,wasm}` | A committed module ships its own assets. |
| `scolta-wp` | yes | `assets/js/…`, `assets/css/…`, `assets/wasm/…` | Installed as a zip-drop plugin, with no Composer at the install site. |
| `scolta-node` | yes | `assets/{css,js,wasm,pagefind}/…` (via `scripts/vendor-assets.mjs`) | The published npm package has to contain the assets. |
| `scolta-python` | yes | `src/scolta/assets/{css,js,wasm,pagefind}/…` (via `scripts/vendor_assets.py`) | The published pip package has to contain the assets. |
| `scolta-laravel` | no | (none) | Installed through Composer, so scolta-php lands in `vendor/`. Laravel reads the assets from `vendor/tag1/scolta-php/` and exposes them with `vendor:publish`. |

So the two CMS packages people ask about, Drupal and WordPress, both ship the copy. Laravel does not,
because it's an ordinary Composer install and reads from vendor. Node and Python ship a copy for the same
reason WordPress does: the published artifact has to contain the file. Always confirm a package's
committed paths against its own vendor script before you rely on this table.

`scolta-wp` deliberately commits no `.sha256` sidecar. It had one; nothing generated it and nothing
read it, so it drifted for two revisions and was removed. scolta-php owns the canonical record, and the
parity check compares asset bytes rather than a claim about them.

Re-vendoring is a deliberate change that a person reviews and that lands in a CHANGELOG. It is never done
at install time or inside a CI check. A `post-install-cmd` that copied assets was removed on purpose;
don't add one back. Each carrier has its own command: `composer copy-assets` in scolta-drupal and
scolta-wp, `node scripts/vendor-assets.mjs` in scolta-node, `python scripts/vendor_assets.py` in
scolta-python. The node and Python scripts copy from a sibling `../scolta-php` checkout through a
fail-closed extension allowlist, so a `.sha256`, `.d.ts` or `.map` can never leak into a published
package.

## How it's enforced

**The parity check, `assets-in-sync`.** It is a public CI job, so it runs on an outside contributor's
pull request too. Drupal and WordPress each run one, and the Drupal job is the
model (`scolta-drupal/.github/workflows/ci.yml`). In one CI job it rewrites `composer.json` to resolve
`tag1/scolta-php` from `dev-main` through a Composer VCS repository, then runs `cmp` on each of the four
committed assets against `vendor/tag1/scolta-php/assets/<path>`. Byte comparison, not checksums: it needs
no manifest and works against every scolta-php version. No skip, no tolerance. A stale copy goes red, and
so does a coordinated change whose scolta-php side hasn't merged yet: the failure message tells you which,
and the second case is the common one. Do not run the copy command to make that one green; that
overwrites the new bundle with the old one, which is the exact failure the check exists to catch. Prove
the check can fail by corrupting one committed asset in a scratch commit.

**What nothing watches.** `scolta-node` and `scolta-python` commit a copy and have no parity check.
Nothing goes
red when their copy falls behind scolta-php, and on 2026-08-09 the four carriers were measured in three
different states. Modelling a check on Drupal's is not a straight copy: neither repo has a Composer link
to scolta-php, so it would have to fetch the source through a public `actions/checkout`, and both would
need a one-time re-vendor first so the new check lands green. Until that exists, treat their copies as
unwatched and re-vendor deliberately when the bundle changes.

**There is no cross-repo re-vendor workflow.** scolta-php has no workflow that opens PRs against the
carriers; propagating a bundle change is a person running each carrier's own command and opening each PR.
If one is built later it must keep the same shape as the manual step: never commit to `main`, never
auto-merge, and name the source commit in a real `## [Unreleased]` CHANGELOG entry. It would also need a
token with contents and PR write on the carriers, because `GITHUB_TOKEN` cannot open a cross-repo PR that
triggers CI.

**Why not a copy-on-install hook.** Copying on install spreads the file around; it doesn't check anything.
The old manifest check could never fail, because the copy hook ran first and overwrote the tracked file
from the same source it then compared against: a fixer and a checker in one pipeline, and the fixer wins.
A `|| true` fallback also let the copy "succeed" while copying nothing. The parity check compares the
committed bytes against the resolved upstream, and nothing rewrites the file before the comparison.

## The version floor (why the check resolves `dev-main`)

Composer honors a stability flag only in the root package. So during a cycle an adapter's floor is
`^X.Y@dev` rather than the previous released line: `^X.(Y-1).0` quietly resolves to a release that's
missing classes the adapter calls (they're resolved at class load, so no `class_exists()` guard helps),
while `^X.Y@dev` fails loudly instead, which is what you want. When scolta-php `X.Y.0` tags, drop the
`@dev` and re-lock; `lock-guard` in `release.yml` then refuses to publish an adapter whose lock still
names a development version.

A branch's committed lock may name `dev-main`, but never a `dist.type=path` repo. That one describes a
single developer's machine, and the `lock-guard` job in each adapter's `ci.yml` rejects it on every
branch. Run `scripts/adapter-floor-matrix.sh` (in scolta-fleet) for the current resolution table instead
of quoting a floor from memory or from this file.
