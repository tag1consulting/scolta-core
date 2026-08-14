# regression — the public query corpus and benchmarks

This is the durable, public record of what Scolta returns for a fixed set of queries, release over
release. The private harness in `scolta-fleet` tells you a count or a response shape moved; this corpus
tells you the *content* of an answer moved, that a query's expanded terms shifted or its summary got
worse, and it tracks the numbers as benchmarks, so the commit history reads as progress, not noise.

It's checked into scolta-core (public) so anyone can see how a release compares to the last one. The
per-release churn is the point: each release adds a benchmark snapshot and a sample, and the history is
the trend line.

The fixed set of queries every release is measured against is documented in `QUERIES.md`. That file is
the list of record, and the corpus and benchmarks are built from it.

AI answers aren't deterministic, so this does not diff raw answer text. It records four things, each
chosen to stay meaningful across runs:

1. **Deterministic index results:** the ordered result set the index returns. This comes from the index,
   not the model, so the same query on the same index gives the same answer. A change here is a real
   signal and it **blocks**: treat it as a regression until a human explains it.
2. **Property checks on the AI answer:** assertions that survive nondeterminism (the expansion contains a
   term, the summary cites an on-site URL, the sort intent is NEVER, nothing is fabricated). A failed
   property **blocks**.
3. **Benchmarks:** the numbers, aggregated per release: how many queries ran, how many properties
   passed, how many deterministic results changed, citation on-site rate, AI-usable rate, and timings.
   These don't block on their own; they're the progress record, and a sharp move is worth a look.
4. **One sampled answer per release:** the actual expand/summary text from one run, archived so a person
   can read how the wording drifts. Not asserted against.

## Layout

```
regression/
  README.md                     <- this file
  QUERIES.md                    <- the fixed benchmark query set, per demo: the list of record
  corpus/
    <demo>.json                 <- expected index results and property checks, one query per row
  benchmarks/
    history.md                  <- append-only: one row per release, headline metrics (the trend)
    <version>.json              <- the full per-release metrics
  samples/
    <version>/
      <demo>.md                 <- the archived AI answers captured for that release
```

`<demo>` matches the demo id in `scolta-fleet/fleet.json` (drupal-git, wp-recipes, laravel-med, and so
on). `<version>` is the released tag captured against, for example `1.2.0`.

## corpus/<demo>.json

```json
{
  "demo": "drupal-git",
  "indexRef": "commit or content hash the deterministic results were captured against",
  "queries": [
    {
      "query": "merge conflict resolution",
      "results": {
        "count": 37,
        "orderedUrls": ["/git/merge-conflicts", "/git/resolving-conflicts", "..."]
      },
      "answerProperties": {
        "expandContains": ["conflict", "merge"],
        "summaryCitesOnSite": true,
        "sortIntent": "NO",
        "noFabrication": true
      }
    }
  ]
}
```

- `results` is the deterministic part. `orderedUrls` is the result set in index order; `count` is the
  total. A diff on either blocks.
- `answerProperties` is what must be true of the AI answer without pinning its exact text. Only assert
  things that are stable: a term that must appear, a citation that must be on-site, the sort-intent
  class, the no-fabrication rule. Don't assert word count or exact phrasing.
- `indexRef` records what the deterministic results were captured against, so a result change can be told
  apart from an index rebuild.

## benchmarks/<version>.json and history.md

`<version>.json` is the full per-release metric set, per demo and per query where it makes sense:

```json
{
  "version": "1.2.0",
  "capturedFrom": "report filename in scolta-fleet/reports the capture used",
  "totals": {
    "queries": 24,
    "propertiesPassed": 71,
    "propertiesTotal": 72,
    "deterministicResultsChanged": 0,
    "citationOnSiteRate": 0.96,
    "aiUsableDemos": "9/9"
  },
  "perQuery": [
    { "demo": "drupal-git", "query": "merge conflict resolution",
      "resultCount": 37, "expandTerms": 4, "summaryChars": 1317,
      "citations": 1, "citationsOnSite": 1, "buildMs": 8200, "expandMs": 640 }
  ]
}
```

`history.md` is an append-only markdown table, one row per release, so the progress is legible without
opening JSON:

```
| version | queries | props pass | det. changed | on-site cites | AI usable | median results |
|---------|---------|-----------|--------------|---------------|-----------|----------------|
| 1.2.0   | 24      | 71/72     | 0            | 0.96          | 9/9       | 33             |
```

Add the new row in the same PR that cuts the release. Never rewrite an old row: a past benchmark is a
past benchmark, the same rule `baselines.json` follows in scolta-fleet.

## samples/<version>/<demo>.md

One human-readable file per demo per release: for each query, the expanded terms and the summary text
from one capture run. Plain markdown. Nobody diffs this automatically. It's the archive you read when a
property check fails and you want to see what actually came back, and the record you scroll to compare
one release against the next by eye.

## Updating the corpus on a release

1. Run the harness in capture mode against the demos (it needs ddev and the API keys, so it runs from
   `scolta-fleet`):

   ```
   npx tsx src/cli.ts capture --corpus ../scolta-core/regression
   ```

   The command name is provisional: capture mode is the follow-up that populates this, and it does not
   exist in the harness yet. See "Status".
2. It writes `corpus/<demo>.json` deterministic results, `benchmarks/<version>.json`, the new
   `history.md` row, and `samples/<version>/<demo>.md`.
3. Review the diff. A changed `orderedUrls`/`count` or a failed property is a regression: explain it or
   fix it before release. A benchmark that moved sharply is worth a look. A changed sample is expected.
4. Commit the reviewed corpus in the same release. The churn is the progress record, which is wanted here.

## Status

This is the spec and the layout. `corpus/`, `benchmarks/` and `samples/` are empty scaffolds, each
holding only a `.gitkeep`. Filling them needs a capture run against the live demos, which needs the
harness plus ddev and API keys, and needs the capture mode itself built. That first population is a
follow-up task, separate from landing these docs. Do not hand-write corpus data: a number nobody measured
reads exactly like evidence, which is why `baselines.json` in scolta-fleet replaced four copied fragment
counts with nulls. The private `baselines.json` keeps holding the numeric tripwires in the meantime; this
corpus is the content-level and benchmark record that complements it.
