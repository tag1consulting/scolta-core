# The public query corpus and benchmarks (proposal)

For anyone deciding whether to build this, or reading a future corpus once it exists. It is a design,
not a working record.

**Status: not built.** This directory holds the design and the query set, and nothing else. The
`corpus/`, `benchmarks/` and `samples/` directories described below do not exist: the pull request that
lands the first measured corpus creates them, so until then there is nowhere to drop a file by hand.
Filling them needs a capture run against the live demos, which needs the private harness plus ddev and
API keys, and needs capture mode itself built. It is not part of the release process and
[REGRESSION.md](../REGRESSION.md) does not depend on it. Do not hand-write corpus data: a number nobody
measured reads exactly like evidence, which is why `baselines.json` in scolta-fleet replaced four copied
fragment counts with nulls. That private file keeps holding the numeric tripwires in the meantime.

## What it would be

A durable, public record of what Scolta returns for a fixed set of queries, release over release. The
private harness in `scolta-fleet` tells you a count or a response shape moved; this corpus would tell you
the *content* of an answer moved, that a query's expanded terms shifted or its summary got worse, and
would track the numbers as benchmarks. Checked into scolta-core, which is public, so anyone can compare
one release against the last.

The query set is documented in [QUERIES.md](QUERIES.md), which explains what each query is for.
`baselines.json` in scolta-fleet stays the record for live counts and blessings.

AI answers aren't deterministic, so this would not diff raw answer text. It would record four things,
each chosen to stay meaningful across runs:

1. **Deterministic index results:** the ordered result set the index returns. This comes from the index,
   not the model, so the same query on the same index gives the same answer. A change here is a real
   signal and it blocks: treat it as a regression until a human explains it.
2. **Property checks on the AI answer:** assertions that survive nondeterminism (the expansion contains a
   term, the summary cites an on-site URL, the sort intent is NEVER, nothing is fabricated). A failed
   property blocks.
3. **Benchmarks:** the numbers, aggregated per release: how many queries ran, how many properties
   passed, how many deterministic results changed, citation on-site rate, AI-usable rate, and timings.
   These don't block on their own; they're the progress record, and a sharp move is worth a look.
4. **One sampled answer per release:** the actual expand/summary text from one run, archived so a person
   can read how the wording drifts. Not asserted against.

## Layout

What the capture command would create. Only the two markdown files exist today:

```
regression/
  README.md                     <- this file
  QUERIES.md                    <- the documented benchmark query set, per demo
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

The values above are a shape example, not a measurement.

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

`history.md` would be an append-only markdown table, one row per release, so the progress is legible
without opening JSON:

```
| version | queries | props pass | det. changed | on-site cites | AI usable | median results |
|---------|---------|-----------|--------------|---------------|-----------|----------------|
| 1.2.0   | 24      | 71/72     | 0            | 0.96          | 9/9       | 33             |
```

Add the new row in the same pull request that cuts the release. Never rewrite an old row: a past
benchmark is a past benchmark, the same rule `baselines.json` follows in scolta-fleet.

## samples/<version>/<demo>.md

One human-readable file per demo per release: for each query, the expanded terms and the summary text
from one capture run. Plain markdown. Nobody diffs this automatically. It's the archive you read when a
property check fails and you want to see what actually came back, and the record you scroll to compare
one release against the next by eye.

## How updating it would work

Once capture mode exists:

1. Run the harness in capture mode against the demos. It needs ddev and the API keys, so it runs from
   `scolta-fleet`. The command name is provisional and the mode is not built:

   ```
   npx tsx src/cli.ts capture --corpus ../scolta-core/regression
   ```
2. It writes `corpus/<demo>.json` deterministic results, `benchmarks/<version>.json`, the new
   `history.md` row, and `samples/<version>/<demo>.md`.
3. Review the diff. A changed `orderedUrls`/`count` or a failed property is a regression: explain it or
   fix it before release. A benchmark that moved sharply is worth a look. A changed sample is expected.
4. Commit the reviewed corpus in the same release. The per-release churn is the progress record, which is
   wanted here.

Adding this to the release sequence in [RELEASING.md](../RELEASING.md) is the last step of building it,
not the first.
