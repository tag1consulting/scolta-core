# The benchmark query set

For anyone reading what Scolta is measured on, and why each query is there. This documents the query
set: which queries each demo carries, which reading each one is for, and why. It carries no live values.
`baselines.json` in the private `scolta-fleet` repo is the record for every count, tolerance and
blessing date, and for whether a row is armed today. Read it there.

Two files in scolta-fleet hold the live values, and they hold different things on purpose. `fleet.json`
carries one `probeQuery` per demo, which drives the AI endpoint checks (expand-query, summarize,
followup) and never a count. `baselines.json` carries the count rows, which are the regression
tripwires. This file reconciles both into one list and marks which is which.

Terms used here (armed row, capture row, track, base-only, settled-expanded) are defined in
[MAINTAINING.md](../MAINTAINING.md), "Terms".

## Drupal

**drupal-git** (git corpus; the only multilingual demo, en/es/fr/de/it)

| query | track | mode | why it is here |
|---|---|---|---|
| merge conflict resolution | base-only | armed | Also the AI probe, and the cross-stack parity query the four port demos share. |
| interactive rebase | base-only | armed | Narrow two-word technical phrase. |
| branch | base-only | armed | Single common word, so it reads the broad end of the corpus. |

**drupal-legal**

| query | track | mode | why it is here |
|---|---|---|---|
| GDPR data subject rights | base-only | armed | Also the AI probe. |
| data processing agreement | base-only | armed | Small result set on a small corpus. |

**drupal-edu**

| query | track | mode | why it is here |
|---|---|---|---|
| computer science degree | settled-expanded | capture | The AI probe. On the expanded track, which the harness does not measure, so it is not gated. |
| artificial intelligence ethics | base-only | armed | The gated row for this demo. |

**drupal-wikipedia** (largest Drupal corpus)

| query | track | mode | why it is here |
|---|---|---|---|
| quantum entanglement | base-only | armed | Also the AI probe. Two rare terms whose intersection is much smaller than either alone. |
| French Revolution causes | base-only | armed | Three terms where the third is a common word, so it tests that adding a term narrows. |
| machine learning neural network | base-only | armed | Broad multi-word query that exercises the expansion path. |
| photosynthesis | base-only | armed | Single-word topical query. |

## WordPress

**wp-apollo**

| query | track | mode | why it is here |
|---|---|---|---|
| Apollo mission timeline | base-only | armed | Also the AI probe. |
| lunar module | base-only | armed | Named-entity phrase on a small corpus. |

**wp-commerce**

| query | track | mode | why it is here |
|---|---|---|---|
| sustainable packaging | settled-expanded | capture | The AI probe. On the expanded track, so it is not gated. |
| organic cotton | base-only | armed | The gated row for this demo. |

**wp-recipes** (high-overlap corpus, the strongest signal for count and expansion regressions)

| query | track | mode | why it is here |
|---|---|---|---|
| meatless | base-only | armed | Single word that stems to a small set. |
| crispy soft | base-only | armed | Two ordinary adjectives, so the intersection is the signal. |
| chicken garlic onion | base-only | armed | Replaced `quick weeknight dinner`, which measured 0. |

The AI probe for this demo is `quick weeknight dinner`, which is deliberately not a count row. See "Why
the probe and the armed set differ".

## Laravel

**laravel-social** (largest corpus in the fleet)

| query | track | mode | why it is here |
|---|---|---|---|
| morning commute traffic | base-only | armed | Replaced `social media trends`, which measured 0. |

The AI probe for this demo is `social media trends`, deliberately unchanged.

**laravel-med**

| query | track | mode | why it is here |
|---|---|---|---|
| emergency procedures | base-only | armed | Also the AI probe. |

## Node and Python ports (git corpus, cross-stack parity)

**next-git**, **nuxt-git**, **astro-git** and **wagtail-git** each carry the single query
`merge conflict resolution`, as capture rows with a null value. All four are never measured: no run has
reached them, because they have no overlay and the scolta-node line is split by construction. They share
the git corpus with drupal-git on purpose, since five ports agreeing on one query is a parity assertion
worth having, but only once four of them are measured rather than handed drupal-git's number.

## Index-size baselines

Each demo also carries a blessed fragment count, which catches a truncated build. It is a property of
the demo's corpus rather than of a query, so it is not listed here. The nine Composer demos have one;
the four port demos are `null`, which means never measured rather than zero. The values live in
`baselines.json` and `fleet.json`, and the harness refuses to start when the two disagree.

## Why the probe and the armed set differ

On four demos the AI probe is not an armed count query, and that is a decision rather than drift.
`probeQuery` drives only the AI endpoint checks, never a count, so changing it to match the count rows
would alter a different set of rows for no reason. On drupal-edu and wp-commerce the probe is a count row
on the expanded track, which the harness does not gate. On wp-recipes and laravel-social the probe is not
a count row at all: both measured 0 against the live index, both were decomposed term by term, every
individual term matched something, and the finding was that the queries were marketing language rather
than corpus vocabulary. Their replacements are drawn from each corpus's own words and narrow
monotonically, which is the shape the base path should produce because it intersects rather than unions.
The probes were left alone.

Zero is the one reading that must never be armed without being understood: a row armed at zero passes
forever no matter how badly recall breaks.

## Open

1. Six rows are capture-only: drupal-edu `computer science degree` and wp-commerce
   `sustainable packaging`, both on the `settled-expanded` track, and the four `*-git` port queries. The
   first two are likely to stay capture, because the harness does not measure that track and an
   LLM-volatile number cannot be a pass/fail baseline. The four port queries stay capture until a run
   measures them.
2. Single-source it. If the corpus in this directory is built, the harness should read its query set from
   here (or from a file generated from here), so `fleet.json` and `baselines.json` can't drift from the
   documented list. Until then, this file explains the set and the two config files implement it.
