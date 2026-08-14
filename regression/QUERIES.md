# regression queries — the list of record

This is the fixed set of queries Scolta is benchmarked against, per demo. It is the query list the corpus
(`corpus/<demo>.json`) and the benchmarks (`benchmarks/`) are built from. Keep this file as the single
documented source of the query set; everything else derives from it.

Two files in the private `scolta-fleet` repo hold the live values, and they hold different things on
purpose. `fleet.json` carries one `probeQuery` per demo, which drives the AI endpoint checks
(expand-query, summarize, followup) and never a count. `baselines.json` carries the count rows, which are
the regression tripwires. This file reconciles both into one list and marks which is which.

Counts below are the base result counts blessed in `baselines.json` (blessings dated between 2026-08-04
and 2026-08-07), each with a 25% tolerance band. "Armed" means a miss fails the run; "capture" means the
value is recorded but not gated. Read `baselines.json` for the live value: the number here is a snapshot,
and the file is the record.

## Drupal

**drupal-git** (git corpus, the only multilingual demo: en/es/fr/de/it)
| query | track | armed | count | note |
|---|---|---|---|---|
| merge conflict resolution | base-only | yes | 37 | also the AI probe; the cross-stack parity query |
| interactive rebase | base-only | yes | 76 | |
| branch | base-only | yes | 778 | |

**drupal-legal**
| query | track | armed | count | note |
|---|---|---|---|---|
| GDPR data subject rights | base-only | yes | 22 | also the AI probe |
| data processing agreement | base-only | yes | 8 | |

**drupal-edu**
| query | track | armed | count | note |
|---|---|---|---|---|
| computer science degree | settled-expanded | no (capture) | band 20 to 31 | the AI probe; not gated |
| artificial intelligence ethics | base-only | yes | 5 | |

**drupal-wikipedia** (largest Drupal corpus)
| query | track | armed | count | note |
|---|---|---|---|---|
| quantum entanglement | base-only | yes | 8 | also the AI probe |
| French Revolution causes | base-only | yes | 531 | |
| machine learning neural network | base-only | yes | 14 | |
| photosynthesis | base-only | yes | 40 | |

## WordPress

**wp-apollo**
| query | track | armed | count | note |
|---|---|---|---|---|
| Apollo mission timeline | base-only | yes | 4 | also the AI probe |
| lunar module | base-only | yes | 52 | |

**wp-commerce**
| query | track | armed | count | note |
|---|---|---|---|---|
| sustainable packaging | settled-expanded | no (capture) | 15 | the AI probe; not gated |
| organic cotton | base-only | yes | 36 | |

**wp-recipes** (high-overlap corpus, the strongest signal for count and expansion regressions)
| query | track | armed | count | note |
|---|---|---|---|---|
| meatless | base-only | yes | 6 | |
| crispy soft | base-only | yes | 32 | |
| chicken garlic onion | base-only | yes | 211 | replaced `quick weeknight dinner`, which measured 0 |

> The AI probe for this demo is `quick weeknight dinner`, which is deliberately not a count row: see
> "Why the probe and the armed set differ" below.

## Laravel

**laravel-social** (largest corpus in the fleet)
| query | track | armed | count | note |
|---|---|---|---|---|
| morning commute traffic | base-only | yes | 54 | replaced `social media trends`, which measured 0 |

> The AI probe for this demo is `social media trends`, deliberately unchanged.

**laravel-med**
| query | track | armed | count | note |
|---|---|---|---|---|
| emergency procedures | base-only | yes | 54 | also the AI probe |

## Node and Python ports (git corpus, cross-stack parity)

**next-git**, **nuxt-git**, **astro-git** and **wagtail-git** each carry the single query
`merge conflict resolution`, as capture rows with a null value. All four are **never measured**: no run
has reached them, because they have no overlay and the scolta-node line is split by construction. They
share the git corpus with drupal-git on purpose, since five ports agreeing on one query is a parity
assertion worth having, but only once four of them are measured rather than handed drupal-git's number.

## Index-size baselines (not query benchmarks, but tracked alongside)

Each demo also has a blessed fragment count that catches a truncated build: drupal-git 1426, drupal-legal
143, drupal-edu 162, drupal-wikipedia 6926, wp-apollo 204, wp-commerce 1000, wp-recipes 3743,
laravel-social 12541, laravel-med 3650. The four node/python demos are `null`, which means never measured
rather than zero. These live in `baselines.json`; the corpus benchmarks carry them too.

## Why the probe and the armed set differ

On three demos the AI probe is not one of the armed count queries, and that is a decision rather than
drift. `probeQuery` drives only the AI endpoint checks, never a count, so changing it to match the count
rows would alter a different set of rows for no reason. Two of the armed queries exist precisely because
their demo's probe measured 0 against the live index: `quick weeknight dinner` (wp-recipes) and
`social media trends` (laravel-social) were both decomposed term by term, every individual term matched
something, and the finding was that the queries were marketing language rather than corpus vocabulary.
Their replacements are drawn from each corpus's own words and narrow monotonically, which is the shape
the base path should produce because it intersects rather than unions. The probes were left alone.

Zero is the one reading that must never be armed without being understood: a row armed at zero passes
forever no matter how badly recall breaks.

## Open

1. **Five queries are capture-only:** drupal-edu `computer science degree`, wp-commerce
   `sustainable packaging`, and the four `*-git` port queries. The first two are on the
   `settled-expanded` track, which the harness does not measure at all and which cannot be a pass/fail
   baseline, so they are likely to stay capture. The four port queries stay capture until a run measures
   them.
2. **Single-source it.** Once the corpus is populated, the harness should read its query set from here
   (or from a file generated from here), so `fleet.json` and `baselines.json` can't drift from the
   documented list. Until then, this file is the reference and the two config files are the
   implementation.
