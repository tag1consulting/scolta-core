# Regression testing before a release

For the maintainer running the pre-release pass. Run these before every release: they are step 2 of the
[RELEASING.md](RELEASING.md) checklist. There are two layers, the automated harness and then a manual
pass in a real browser. A green harness run does not prove the site works: it once reported 99 passing
and 0 failing while three WordPress demos returned HTTP 500 on every AI call to a real browser. So the
browser pass is required, not optional.

The harness and its data are maintainer-only, in the private `scolta-fleet` repo. `fleet.json` defines
the demos: which exist, their package line, URLs and build paths. Don't trust a demo list copied into a
doc, because demos get renamed. `baselines.json` holds the blessed numbers: the count rows and a
fragment count per demo, each naming the run that produced it. Terms used here (blessing, armed row,
capture row, track, overlay) are defined in [MAINTAINING.md](MAINTAINING.md), "Terms".

## Order

1. Confirm the fleet is coherent: every demo on a package line resolves the same version of it. The
   harness exits 2 if they differ. `--allow-skew` overrides that and marks the run "confounded"; don't
   bless anything from a run like that.
2. Run the harness (below) and read its report under `reports/`.
3. Do the browser pass on each demo.
4. Put the report filename in the release notes.

## 1. The automated harness (maintainer-only, in scolta-fleet, TypeScript)

```
npm ci
npx tsx src/cli.ts check --counts      # --counts is off by default and needs Playwright
npx tsx src/cli.ts validate            # rejects a baseline that cites a report file that isn't there
```

Which rows a run can fail on is set per row in `baselines.json`: armed rows fail the run when they miss,
capture rows only record what was observed. Read the file rather than a count quoted in prose. Rows also
carry a track: `base-only` is the tripwire, anchored in stemming, scoring and the index alone;
`settled-expanded` is recorded for orientation and is deliberately not measured here, because an
LLM-volatile number cannot be a pass/fail baseline.

Don't pass `--json` or `--markdown`. The harness writes its own timestamped files under `reports/`, and
every blessed baseline cites one by name. Other useful flags: `--demo ID` (repeatable), `--skip-build`,
`--allow-skew`, `--reports-dir` / `--report-name`.

It covers the nine Composer (PHP) demos: four Drupal, three WordPress, two Laravel. For each demo it:

- builds the index and checks the fragment count against the blessed value;
- calls health, expand-query, summarize and followup, and checks the response shape;
- fetches every result-card URL and every AI-citation URL and checks for HTTP 200;
- compares live counts against `baselines.json` within tolerance bands;
- sets the demo's memory limit from the manifest, so it doesn't leave a dirty `settings.php`;
- applies the overlay, so the run measures the code it says it measures. Where a demo commits the
  adapter rather than installing it, that directory is renamed aside for the duration and put back
  afterwards, because Composer installs the library *underneath* a checked-in plugin and the lock is
  then silent about the code actually calling it.

Exit codes: 0 everything passed, 1 an armed row missed, 2 a precondition failed, 3 a usage error.

It does not check: summary quality, the expanded-count track (it varies by design), whether the AI is
usable via the health endpoint (health is wrong in both directions, so use the expand call as the real
signal), sort-intent correctness, decomposition, or grounding. Those are the browser pass. Citation
resolution is measured but armed on no demo: its state moved between two runs of identical code on three
of nine demos, which is model variance, not a regression.

The node and python demos (next-git, nuxt-git, astro-git, wagtail-git) have no baselines and no overlay,
so the harness skips them. Report them as "not measured", never as passing. Don't copy a query count into
their fragment baseline; all four carried drupal-git's fragment count until it was removed, and five
ports agreeing on a number four of them were handed is not a parity assertion. A real number needs a run
scoped to just those, with `--allow-skew`, marked confounded, proposed rather than committed.

## 2. The browser pass (each demo)

Capture the config first. On WordPress: `wp option get scolta_settings --format=json | jq`. Do this
before you diagnose anything. Look at `ai_languages` (more than one arms the auto-filter, which triggers
OR-fallback inflation), `ai_summary_top_n`, `title_match_boost`, `expand_primary_weight`,
`recency_strategy`, and whether someone selected the "Recipe & Content Catalog" preset in admin (that
overwrites custom scoring, and can't be undone). Restore each demo to the values it actually had, not to
assumed defaults.

For each demo, in a real browser, check:

- **The on-screen count** after it settles (poll until it stops changing, about 2.5s, or the header says
  it includes expanded terms). A count below baseline means missing pages, not rounding.
- **The banner, header, and filter text.** Record the "No exact matches found, showing partial matches"
  banner next to any count: it marks an AND search that fell back to OR.
- **The facet panel.** It's driven by the index and is static; a missing facet index on a non-PHP port is
  not a regression.
- **The empty/error state, the console** (Scolta, WASM, or asset errors), and **the network tab** (count
  real requests, not devtools rows: a 308 redirect pair is one request; confirm with `curl` or
  `fetch(url, {redirect: 'manual'})`).
- **The admin settings page.** Every field shows the saved value. A hardcoded-provider form won't show up
  in a CLI audit.

Then the judgment checks: sort intent (YES / NO / NEVER, where a NEVER key on a demo that isn't a NEVER
demo is a P0 config leak; wp-recipes is the only real NEVER demo), decomposition (members vs synonyms, a
2-of-N oracle, case-insensitive substring; clear caches before a fresh pass, and re-run a failing row
after clearing rather than resampling), no fabrication (echoing a made-up proper noun back is not a
failure), and summary quality (poll the summary element on its own; check computed figures separately
from copied ones).

Asset freshness matters most on Laravel. `composer update` updates `vendor/` but does not re-publish
`public/vendor/`; `vendor:publish` runs once and doesn't notice staleness. Check the published asset by
comparing its sha256 against the canonical file, not with `file_exists()`: a present-but-stale JS file
passes a presence check but is still broken. Every PHP demo should serve the same canonical `scolta.js`.

Index layout differs by indexer. The PHP indexer writes to `{output_dir}/pagefind/`; the binary indexer
writes `{output_dir}/pagefind.js`. Every consumer, the search path, the health check, and the CLI
`status` command, has to handle both. The CLI status commands are the ones that get missed.

## 3. Regressions to watch for

Check each one. Most are fixes already in place that a change could undo.

- **Language auto-filter falling back to OR.** The auto-filter fires when `ai_languages` has more than
  one entry, the AND search returns nothing, and OR fallback takes over (each term at 0.6 weight), which
  inflates counts and floods results with title-keyword matches. Confirm how many languages are set.
- **Silently doubled defaults.** Summarization `max_tokens` went 512 to 1024 without being wired through
  the WP REST class; the context limit went 50KB to 100KB. A config field that isn't wired through is a
  silent change in behavior, and these two compound.
- **Stale Laravel published assets.** See asset freshness above.
- **Index-layout divergence.** See index layout above.
- **The expand-query response shape.** It changed from `{expanded, original}` to a flat array; the shape
  checks catch this, code review didn't. The shape parsed today is `{terms, sort_hint, subject_terms,
  filter_hint}`.
- **Stale language filter after a language switch.** Going `/en` to `/it` carries `f_language=en`; the
  detected page language has to override the URL filter on load and on popstate. Test drupal-git, the
  only multilingual demo.
- **Silent index truncation.** An interrupted build commits chunks without ordinals, so the index drops
  pages while reporting success, and the fragment count comes in below baseline. This is what the
  fragment counts exist to catch: drupal-git once produced 1150 fragments against a blessed 1426 and
  still reported success.
- **Recall dropping while counts spike.** A 4x to 50x drop on an armed row (sub-word expansion removed)
  and an OR-fallback spike are both regressions. Neither is a reason to re-bless.
- **An expired Amazee trial key.** Expand echoes the query back, summarize returns `{}`, and health still
  says `ai_configured: true`. Use the expand call, not health, as the AI signal.
- **A prompt change that only landed in one port.** `scolta-php`, `scolta-node` and `scolta-python` each
  run a prompt-text identity test against scolta-core's `src/prompts.rs` in their own CI, so a mirror
  that drifts turns those repos red rather than quietly changing what a demo returns.

## 4. The public regression corpus (planned)

The harness tells you a count or a response shape changed. It does not tell you the *content* of a
query's answer changed: that a query's expanded terms shifted, or its summary got worse. The corpus
proposed in [regression/](regression/) is meant to fill that, as a public record anyone can read to see
whether a release made a specific query worse.

It does not exist yet. `regression/` holds the spec and empty scaffolds, and the capture command that
would populate it has not been built. It is not a release step, and nothing above depends on it. It
becomes one when the capture command and the first measured corpus land, and this section says so on the
same day. Design and layout are in [regression/README.md](regression/README.md); the query set is in
[regression/QUERIES.md](regression/QUERIES.md).

## What a contributor without fleet access can do

The harness is maintainer-only, and the browser pass needs the demos. On a public pull request:

- Say which queries you expect to move and in which direction. That is what a maintainer measures.
- Run the affected repo's own suite and parity fixtures, and say in the PR which ones you ran.
- For an indexing change, a fragment count from your own local build against a corpus you name is useful
  evidence, even though it is not a blessing.

A maintainer runs the harness and posts the report filename on the pull request.

## The self-hosted runner

There isn't one yet. The `fleet-run` CI job is `if: false` on `runs-on: [self-hosted, scolta-fleet]`: no
hosted runner can drive the thirteen ddev projects. Until it's built, the full run is a manual step
someone does, not a check that runs on its own. What scolta-fleet's CI does run on every pull request is
the harness's own typecheck, unit tests and a structural validation of `fleet.json` and `baselines.json`,
so a malformed edit fails in seconds instead of an hour into a regression run.

Keep the negative controls in `reports/`. A run that recorded a dead result URL, and the restored run
beside it, are how you know the harness can still fail.
