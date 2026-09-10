# The browser bundle and how it stays in sync

For anyone changing the browser bundle or wondering how a given package gets it. Read it before you edit
anything under an `assets/` directory in any Scolta repo.

`scolta-php` owns the canonical copy of the browser bundle. Some packages ship their own committed copy
of it; others read it from their Composer vendor directory at run time. Which packages do which is set by
how each platform installs, not by preference. Committed copies drift silently if nothing watches them,
so CI checks one of them, and the gaps are named below.

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
two `*.pagefind` WASM blobs). The WordPress copy does not include it, and neither does the four-file set
scolta-drupal deploys out of vendor; the node and Python copies do, because their published artifacts
ship the whole runtime.

Checksums for the four live in `scolta-php/assets/ASSETS.sha256`, one line per file. There is also a
standalone `assets/js/scolta.js.sha256`. It is not a second checksum: it is extracted from the manifest,
so the hash is computed in one place. Regenerate both together with `composer update-js-checksum` in
scolta-php, never by hashing the file again by hand.

That manifest matters more now than it did when every adapter carried a byte-compared copy, not less.
scolta-laravel reads the bare hash at run time to decide whether its published asset is stale, and the
Drupal deployer compares hashes to decide what to re-copy. A wrong manifest misleads two adapters on
live sites, rather than failing a CI check in one repo.

## Which packages carry a copy, and why

Composer runs a package's `post-install-cmd` and `post-update-cmd` scripts only for the root package,
never for a dependency. So a site that installs an adapter never gets the assets copied out of `vendor/`
by Composer itself. Either the file is committed already, or the adapter places it itself — at run time,
with its own code. That is what splits the table below: read it for whether the package you are changing
carries a copy, and where.

| Package | Carries a copy? | Committed at | Why |
|---|---|---|---|
| `scolta-wp` | yes | `assets/js/…`, `assets/css/…`, `assets/wasm/…` | Installed as a zip-drop plugin, with no Composer at the install site. |
| `scolta-node` | yes | `assets/{css,js,wasm,pagefind}/…` (via `scripts/vendor-assets.mjs`) | The published npm package has to contain the assets. |
| `scolta-python` | yes | `src/scolta/assets/{css,js,wasm,pagefind}/…` (via `scripts/vendor_assets.py`) | The published pip package has to contain the assets. |
| `scolta-drupal` | no | (none) | Installed through Composer, so scolta-php lands in `vendor/`, which is above the docroot and not web-accessible. `Drupal\scolta\Service\AssetDeployer` copies the four files into `public://scolta-assets` and `scolta.libraries.yml` references them by stream-wrapper URI. |
| `scolta-laravel` | no | (none) | Installed through Composer, so scolta-php lands in `vendor/`. Laravel reads the assets from `vendor/tag1/scolta-php/` and exposes them with `vendor:publish`. |

So there are three carriers and two adapters that read from vendor. WordPress carries a copy because a
WordPress.org install is a zip drop with no Composer at the install site; node and Python carry one for
the same shape of reason, because the published artifact has to contain the file. Always confirm a
package's committed paths against its own vendor script before you rely on this table.

**scolta-drupal used to be a carrier and stopped** (its PR #227). It committed the bundle because
drupal.org ships the tarball built from git and `hook_install()` runs once per site ever, so an updating
site served whatever was committed. `AssetDeployer` now copies the bundle from the installed
`tag1/scolta-php` into `public://scolta-assets` at module install, in `scolta_update_10005()`, and on
every cache rebuild via `hook_rebuild()`, comparing size then hash so a damaged files directory
self-heals; each file lands via copy-to-temp plus rename, so a client never sees a truncated file
mid-deploy. `composer update` plus `drush cr` now ships a new bundle with no module release and no
re-vendor commit, and the public files directory is writable on immutable-code hosts (Pantheon, Acquia
production) where the module directory is not. `composer copy-assets` and the `assets-in-sync` job went
with the committed copy, and `assets-in-sync` came off that repo's required-checks list. Do not model
anything on scolta-drupal's old parity job; the working example is scolta-wp's.

`scolta-wp` deliberately commits no `.sha256` sidecar. It had one; nothing generated it and nothing
read it, so it drifted for two revisions and was removed. scolta-php owns the canonical record, and the
parity check compares asset bytes rather than a claim about them.

Re-vendoring is a deliberate change that a person reviews and that lands in a CHANGELOG. It is never done
at install time or inside a CI check. A `post-install-cmd` that copied assets was removed on purpose;
don't add one back. Each carrier has its own command: `composer copy-assets` in scolta-wp,
`node scripts/vendor-assets.mjs` in scolta-node, `python scripts/vendor_assets.py` in scolta-python. The
node and Python scripts copy from a sibling `../scolta-php` checkout through a fail-closed extension
allowlist, so a `.sha256`, `.d.ts` or `.map` can never leak into a published package.

## How it's enforced

**The parity check, `assets-in-sync`.** One repo runs one: scolta-wp
(`scolta-wp/.github/workflows/ci.yml`). It is a public CI job, so it runs on an outside contributor's
pull request too. In one job it rewrites `composer.json` to resolve `tag1/scolta-php` from `dev-main`
through a Composer VCS repository, then runs `cmp` on each of the four committed assets against
`vendor/tag1/scolta-php/assets/<path>`. Byte comparison, not checksums: it needs no manifest and works
against every scolta-php version. No skip, no tolerance. A stale copy goes red, and so does a coordinated
change whose scolta-php side hasn't merged yet: the failure message tells you which, and the second case
is the common one. Do not run `composer copy-assets` to make that one green; that overwrites the new
bundle with the old one, which is the exact failure the check exists to catch. Prove the check can fail
by corrupting one committed asset in a scratch commit.

**What runs at run time instead.** The two vendor-reading adapters need no CI parity job, because there
is no committed copy to fall behind: they compare against the vendored canonical on the running site.
The Drupal deployer re-copies any file whose size or hash differs from `vendor/tag1/scolta-php/assets/`
on every cache rebuild, so staleness is corrected rather than reported. scolta-laravel's
`src/Services/AssetStatus.php` compares the sha256 of the published `scolta.js` against the bare hash in
`vendor/tag1/scolta-php/assets/js/scolta.js.sha256` and reports a stale publish. Neither is a weaker
parity check: there is nothing to keep in parity, because the vendored file is the only source either
site ever serves from.

**What nothing watches.** `scolta-node` and `scolta-python` commit a copy and have no parity check.
Nothing goes red when their copy falls behind scolta-php, and on 2026-08-09, when scolta-drupal was still
a carrier, the four carriers were measured in three different states. Modelling a check on scolta-wp's is
not a straight copy: neither repo has a Composer link to scolta-php, so it would have to fetch the source
through a public `actions/checkout`, and both would need a one-time re-vendor first so the new check
lands green. Until that exists, treat their copies as unwatched and re-vendor deliberately when the
bundle changes.

**There is no cross-repo re-vendor workflow.** scolta-php has no workflow that opens PRs against the
carriers; propagating a bundle change is a person running each carrier's own command and opening each PR.
scolta-drupal and scolta-laravel need no such PR at all — they pick the change up through
`composer.lock`. If one is built later it must keep the same shape as the manual step: never commit to
`main`, never auto-merge, and name the source commit in a real `## [Unreleased]` CHANGELOG entry. It
would also need a token with contents and PR write on the carriers, because `GITHUB_TOKEN` cannot open a
cross-repo PR that triggers CI.

**Why not a copy-on-install hook.** Copying on install spreads the file around; it doesn't check anything.
The old manifest check could never fail, because the copy hook ran first and overwrote the tracked file
from the same source it then compared against: a fixer and a checker in one pipeline, and the fixer wins.
A `|| true` fallback also let the copy "succeed" while copying nothing. The parity check compares the
committed bytes against the resolved upstream, and nothing rewrites the file before the comparison. This
is not an argument against an adapter deploying assets from vendor at run time, which is what
scolta-drupal does: that code places the file and never claims to be checking one.

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
