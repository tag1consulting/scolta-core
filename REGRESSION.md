# REGRESSION — tests to run before you ship

Run these before every release (they are step 2 of the `RELEASING.md` checklist). There are two layers:
the automated harness, then a manual pass in a real browser. A green harness run does not prove the site
works: it once reported 99 passing and 0 failing while three WordPress demos returned HTTP 500 on every
AI call to a real browser. So the browser pass is required, not optional.

The harness and its data live in the private `scolta-fleet` repo. `fleet.json` is the source of truth for
which demos exist and their baselines; don't trust a demo list copied into a doc, because demos get
renamed. `baselines.json` holds the blessed counts, and each one names the run that produced it. The
public, content-level record of what each query returns is the corpus in `regression/` (see the last
section).

## Order

1. Confirm the fleet is coherent: every demo on a package line resolves the same version of it. The
   harness exits 2 if they differ. `--allow-skew` overrides that and marks the run "confounded"; don't
   bless anything from a run like that.
2. Run the harness (below) and read its report under `reports/`.
3. Do the browser pass on each demo.
4. Update the `regression/` corpus and review the diff.
5. Put the report filename in the release notes.

## 1. The automated harness (in scolta-fleet, TypeScript)

```
npm ci
npx tsx src/cli.ts check --counts      # --counts is off by default and needs Playwright
npx tsx src/cli.ts validate            # rejects a baseline that cites a report file that isn't there
```

Which rows a run can fail on is per row in `baselines.json`: `armed` rows fail the run when they miss,
`capture` rows only record what was observed. Read the file rather than a count quoted in prose. Rows
also carry a track: `base-only` is the tripwire, anchored in stemming, scoring and the index alone;
`settled-expanded` is recorded for orientation and is deliberately not measured here, because an
LLM-volatile number cannot be a pass/fail baseline.

Don't pass `--json` or `--markdown`. The harness writes its own timestamped files under `reports/`, and
every blessed baseline cites one by name. Other useful flags: `--demo ID` (repeatable), `--skip-build`,
`--allow-skew`, `--reports-dir` / `--report-name`.

It covers the nine Composer (PHP) demos: four Drupal, three WordPress, two Laravel. For each demo it:

- builds the index and checks the fragment count against the `fleet.json` baseline;
- calls health, expand, summarize, and followup, and checks the response shape;
- fetches every result-card URL and every AI-citation URL and checks for HTTP 200;
- compares live counts against `baselines.json` within tolerance bands;
- sets the demo's memory limit from the manifest, so it doesn't leave a dirty `settings.php`;
- swaps a demo's committed adapter copy aside for the duration, because Composer installs *underneath* a
  checked-in plugin and the lock is then silent about the code actually calling the library.

Exit codes: 0 everything passed, 1 an armed row missed, 2 a precondition failed, 3 a usage error.

It does **not** check: summary quality, the expanded-count track (it varies by design), whether the AI is
usable via the health endpoint (health is wrong in both directions, so use the expand call as the real
signal), sort-intent correctness, decomposition, or grounding. Those are the browser pass and the corpus.
Citation resolution is measured but armed on no demo: its state moved between two runs of identical code
on three of nine demos, which is model variance, not a regression.

The node and python demos (next-git, nuxt-git, astro-git, wagtail-git) have no baselines and no overlay,
so the harness skips them. Report them as "not measured", never as passing. Don't copy a query count into
their fragment baseline; all four carried drupal-git's 1426 until it was removed, and five ports agreeing
on a number four of them were handed is not a parity assertion. A real number needs a run scoped to just
those, with `--allow-skew`, marked confounded, proposed rather than committed.

## 2. The browser pass (each demo)

**Capture the config first.** On WordPress: `wp option get scolta_settings --format=json | jq`. Do this
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

Then the judgment checks: **sort intent** (YES / NO / NEVER, where a NEVER key on a demo that isn't a
NEVER demo is a P0 config leak; wp-recipes is the only real NEVER demo), **decomposition** (members vs
synonyms, a 2-of-N oracle, case-insensitive substring; clear caches before a fresh pass, and re-run a
failing row after clearing rather than resampling), **no fabrication** (echoing a made-up proper noun
back is not a failure), and **summary quality** (poll the summary element on its own; check computed
figures separately from copied ones).

**Asset freshness (matters most on Laravel).** `composer update` updates `vendor/` but does not
re-publish `public/vendor/`; `vendor:publish` runs once and doesn't notice staleness. Check the published
asset by comparing its sha256 against the canonical file, not with `file_exists()`: a present-but-stale
JS file passes a presence check but is still broken. Every PHP demo should serve the same canonical
`scolta.js`.

**Index layout.** The PHP indexer writes to `{output_dir}/pagefind/`; the binary indexer writes
`{output_dir}/pagefind.js`. Every consumer, the search path, the health check, and the CLI `status`
command, has to handle both. The CLI status commands are the ones that get missed.

## 3. Regressions to watch for

Check each one. Most are fixes already in place that a change could undo.

- **Language auto-filter falling back to OR** (the worst one): the auto-filter fires when `ai_languages`
  has more than one entry, the AND search returns nothing, and OR fallback takes over (each term at 0.6
  weight), which inflates counts and floods results with title-keyword matches. Confirm how many
  languages are set.
- **Silently doubled defaults:** summarization `max_tokens` went 512 to 1024 without being wired through
  the WP REST class; the context limit went 50KB to 100KB. A config field that isn't wired through is a
  silent change in behavior, and these two compound.
- **Stale Laravel published assets:** see asset freshness above.
- **Index-layout divergence:** see index layout above.
- **The expand-query response shape** changed from `{expanded, original}` to a flat array; the shape
  checks catch this, code review didn't. The shape parsed today is `{terms, sort_hint, subject_terms,
  filter_hint}`.
- **Stale language filter after a language switch:** going `/en` to `/it` carries `f_language=en`; the
  detected page language has to override the URL filter on load and on popstate. Test the five-language
  git demo.
- **Silent index truncation:** an interrupted build commits chunks without ordinals, so the index drops
  pages while reporting success, and the fragment count comes in below baseline. This is what the
  fragment counts exist to catch: drupal-git produced 1150 instead of 1426 and still reported success.
- **Recall dropping vs counts spiking:** a 4x to 50x drop on an armed row (sub-word expansion removed)
  and an OR-fallback spike are both regressions. Neither is a reason to re-bless.
- **An expired Amazee trial key:** expand echoes the query back, summarize returns `{}`, and health still
  says `ai_configured: true`. Use the expand call, not health, as the AI signal.
- **A prompt change that only landed in one port.** `scolta-php`, `scolta-node` and `scolta-python` each
  run a prompt-text identity test against scolta-core's `src/prompts.rs` in their own CI, so a mirror
  that drifts turns those repos red rather than quietly changing what a demo returns.

## 4. The regression corpus (`regression/`)

The harness above tells you a count or a response shape changed. It does not tell you the *content* of a
query's answer changed: that a query's expanded terms shifted, or its summary got worse. The corpus in
`regression/` is the durable, public record that fills that gap. It is checked into this public repo so
anyone can see whether a release made a specific query worse, release over release.

For each demo and query it holds four things, chosen so the signal stays meaningful even though AI output
isn't deterministic:

- **The deterministic index results:** the ordered result set the index returns for the query. This
  comes from the index, not the model, so it's stable and a diff is a real signal. This is the part that
  blocks: an unexpected change is a regression until reviewed.
- **Property checks on the AI answer:** assertions that survive nondeterminism, such as the expansion
  containing a given term, the summary citing an on-site URL, the sort intent classifying as NEVER, the
  answer not fabricating. A failed property is a regression.
- **Benchmarks:** the numbers per release (queries run, properties passed, deterministic results changed,
  citation on-site rate, AI-usable rate, timings), appended one row per release to `benchmarks/history.md`.
  They don't block on their own; they're the progress trend, and a sharp move is worth a look.
- **One sampled answer per release:** the actual expand/summary text from one run, archived for the
  record. It is not asserted against; it's there so a person can eyeball how the wording drifts over
  releases.

How to update it on a release: run the harness in capture mode against the demos, write the deterministic
results, the benchmark row, and the sampled answers, review the diff, and commit, in the same PR that
cuts the release. A changed deterministic result or a failed property blocks until it's explained; a
benchmark that moved sharply is worth a look; a changed sample is for human eyes. The per-release churn in
this repo is intentional: it's the progress and benchmark record. The query set is in
`regression/QUERIES.md`, and the layout and capture command are in `regression/README.md`. The corpus is
not populated yet; the spec and the scaffold are what exist today.

## The self-hosted runner

There isn't one yet. The `fleet-run` CI job is `if: false` on `runs-on: [self-hosted, scolta-fleet]`: no
hosted runner can drive the thirteen ddev projects. Until it's built, the full run is a manual step
someone does, not a check that runs on its own. What scolta-fleet's CI does run on every pull request is
the harness's own typecheck, unit tests and a structural validation of `fleet.json` and `baselines.json`,
so a malformed edit fails in seconds instead of an hour into a regression run.

Keep the negative controls in `reports/`. A run that recorded a dead result URL, and the restored run
beside it, are how you know the harness can still fail.
