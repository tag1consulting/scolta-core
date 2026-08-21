use scolta_core::inner;
use serde_json::json;

// ── describe ──────────────────────────────────────────────────────────────────

#[test]
fn describe_lists_all_functions() {
    let manifest = inner::describe();
    let functions = manifest["functions"].as_object().unwrap();
    let names: Vec<&str> = functions.keys().map(|k| k.as_str()).collect();

    for expected in &[
        "score_results",
        "merge_results",
        "match_priority_pages",
        "parse_expansion",
        "batch_score_results",
        "resolve_prompt",
        "get_prompt",
        "extract_context",
        "batch_extract_context",
        "sanitize_query",
        "truncate_conversation",
        "version",
        "describe",
    ] {
        assert!(
            names.contains(expected),
            "describe() missing function: {}",
            expected
        );
    }

    assert!(
        !names.contains(&"to_js_scoring_config"),
        "describe() still lists removed function to_js_scoring_config"
    );
}

/// Doc-sync guard: every `#[wasm_bindgen]` export in `browser.rs` must carry a
/// `# Stability` doc block whose `Status`/`Since` match its `describe()` entry,
/// mirroring the API.md guard for scoring defaults.
#[test]
fn stability_doc_blocks_match_describe() {
    let browser_rs =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/browser.rs"))
            .expect("src/browser.rs must be readable");

    // Parse `/// Status: X` and `/// Since: Y` doc lines preceding `pub fn NAME`.
    let mut documented: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    let (mut status, mut since): (Option<String>, Option<String>) = (None, None);
    for line in browser_rs.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("///") {
            let rest = rest.trim();
            if let Some(v) = rest.strip_prefix("Status:") {
                status = Some(v.trim().to_string());
            } else if let Some(v) = rest.strip_prefix("Since:") {
                since = Some(v.trim().to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("pub fn ") {
            let name = rest
                .split(['(', '<'])
                .next()
                .expect("fn line must have a name")
                .trim()
                .to_string();
            if let (Some(st), Some(si)) = (status.take(), since.take()) {
                documented.insert(name, (st, si));
            } else {
                panic!("export `{name}` is missing a `# Stability` doc block (Status/Since)");
            }
        }
    }

    let manifest = inner::describe();
    let functions = manifest["functions"].as_object().unwrap();
    for (name, info) in functions {
        let (doc_status, doc_since) = documented.get(name).unwrap_or_else(|| {
            panic!("describe() lists `{name}` but browser.rs has no documented export for it")
        });
        assert_eq!(
            doc_status,
            info["stability"].as_str().unwrap(),
            "`{name}`: doc-block Status disagrees with describe() stability"
        );
        assert_eq!(
            doc_since,
            info["since"].as_str().unwrap(),
            "`{name}`: doc-block Since disagrees with describe() since"
        );
    }
    for name in documented.keys() {
        assert!(
            functions.contains_key(name),
            "browser.rs exports `{name}` but describe() does not list it"
        );
    }
}

// ── score_results ─────────────────────────────────────────────────────────────

#[test]
fn score_results_ranks_relevant_first() {
    let input = json!({
        "query": "drupal performance",
        "results": [
            {"title": "Drupal Performance Guide", "url": "/a", "excerpt": "drupal performance tips", "date": "2026-01-01"},
            {"title": "About Us", "url": "/b", "excerpt": "company info", "date": "2026-01-01"}
        ]
    });
    let result = inner::score_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr[0]["url"], "/a");
}

#[test]
fn score_results_priority_page_boost() {
    let input = json!({
        "query": "contact us",
        "results": [
            {"title": "Contact", "url": "/contact", "excerpt": "get in touch", "date": "2026-01-01"},
            {"title": "Drupal Guide", "url": "/guide", "excerpt": "drupal performance", "date": "2026-01-01"}
        ],
        "config": {
            "priority_pages": [
                {"url_pattern": "/contact", "keywords": ["contact"], "boost": 100.0}
            ]
        }
    });
    let result = inner::score_results(&input).unwrap();
    assert_eq!(result[0]["url"], "/contact");
}

#[test]
fn score_results_forced_phrase_excludes_scattered_terms() {
    // Quoted query: a result whose terms appear only scattered apart (per its
    // locations) is dropped; the adjacent-phrase result survives. Unquoted,
    // both survive.
    let results = json!([
        {"title": "Unspeakable: The Tulsa Race Massacre", "url": "/tulsa",
         "excerpt": "written by Carole Weatherford", "locations": [3, 40]},
        {"title": "Revolutionary Era Sources", "url": "/boston",
         "excerpt": "primary sources", "locations": [7, 8]}
    ]);
    let quoted = inner::score_results(&json!({
        "query": "\"boston massacre\"",
        "results": results
    }))
    .unwrap();
    let arr = quoted.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["url"], "/boston");

    let unquoted = inner::score_results(&json!({
        "query": "boston massacre",
        "results": results
    }))
    .unwrap();
    assert_eq!(unquoted.as_array().unwrap().len(), 2);
}

// ── score_results: sort_override ──────────────────────────────────────────────

#[test]
fn score_results_sort_override_asc() {
    let input = json!({
        "query": "product",
        "results": [
            {"title": "Expensive", "url": "/expensive", "excerpt": "product", "date": "2026-01-01", "price": "99.99"},
            {"title": "Cheap",     "url": "/cheap",     "excerpt": "product", "date": "2026-01-01", "price": "9.99"},
            {"title": "Medium",    "url": "/medium",    "excerpt": "product", "date": "2026-01-01", "price": "49.99"}
        ],
        "sort_override": {"field": "price", "direction": "asc"}
    });
    let result = inner::score_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr[0]["url"], "/cheap");
    assert_eq!(arr[1]["url"], "/medium");
    assert_eq!(arr[2]["url"], "/expensive");
}

#[test]
fn score_results_sort_override_desc() {
    let input = json!({
        "query": "product",
        "results": [
            {"title": "Cheap",     "url": "/cheap",     "excerpt": "product", "date": "2026-01-01", "price": "9.99"},
            {"title": "Expensive", "url": "/expensive", "excerpt": "product", "date": "2026-01-01", "price": "99.99"},
            {"title": "Medium",    "url": "/medium",    "excerpt": "product", "date": "2026-01-01", "price": "49.99"}
        ],
        "sort_override": {"field": "price", "direction": "desc"}
    });
    let result = inner::score_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr[0]["url"], "/expensive");
    assert_eq!(arr[1]["url"], "/medium");
    assert_eq!(arr[2]["url"], "/cheap");
}

#[test]
fn score_results_sort_override_excludes_missing_field() {
    let input = json!({
        "query": "product",
        "results": [
            {"title": "Has Price", "url": "/has-price", "excerpt": "product", "date": "2026-01-01", "price": "50.00"},
            {"title": "No Price",  "url": "/no-price",  "excerpt": "product", "date": "2026-01-01"}
        ],
        "sort_override": {"field": "price", "direction": "asc"}
    });
    let result = inner::score_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["url"], "/has-price");
}

#[test]
fn score_results_sort_override_absent_unchanged() {
    // Without sort_override, results are ranked by relevance score (same behavior as before).
    let input = json!({
        "query": "drupal performance",
        "results": [
            {"title": "Drupal Performance Guide", "url": "/a", "excerpt": "drupal performance tips", "date": "2026-01-01"},
            {"title": "About Us",                 "url": "/b", "excerpt": "company info",            "date": "2026-01-01"}
        ]
    });
    let result = inner::score_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr[0]["url"], "/a");
}

#[test]
fn score_results_sort_override_tiebreaker_by_relevance() {
    // Both results have the same price; the one with better query relevance should rank first.
    let input = json!({
        "query": "drupal product",
        "results": [
            {"title": "Product Page",   "url": "/a", "excerpt": "product",        "date": "2026-01-01", "price": "50.00"},
            {"title": "Drupal Product", "url": "/b", "excerpt": "drupal product", "date": "2026-01-01", "price": "50.00"}
        ],
        "sort_override": {"field": "price", "direction": "asc"}
    });
    let result = inner::score_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    // /b matches both query terms in title and excerpt → higher relevance score → first after tie.
    assert_eq!(arr[0]["url"], "/b");
}

#[test]
fn score_results_sort_override_string_sort() {
    // Non-numeric values fall back to lexicographic ordering.
    let input = json!({
        "query": "post",
        "results": [
            {"title": "Post C", "url": "/c", "excerpt": "post", "date": "2026-01-01", "category": "zebra"},
            {"title": "Post A", "url": "/a", "excerpt": "post", "date": "2026-01-01", "category": "apple"},
            {"title": "Post B", "url": "/b", "excerpt": "post", "date": "2026-01-01", "category": "mango"}
        ],
        "sort_override": {"field": "category", "direction": "asc"}
    });
    let result = inner::score_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr[0]["url"], "/a");
    assert_eq!(arr[1]["url"], "/b");
    assert_eq!(arr[2]["url"], "/c");
}

// ── merge_results ─────────────────────────────────────────────────────────────

#[test]
fn merge_results_two_sets_dedup() {
    let input = json!({
        "sets": [
            {
                "results": [{"title": "Page A", "url": "/a", "score": 0.9, "excerpt": "a", "date": "2025-01-01"}],
                "weight": 1.0
            },
            {
                "results": [
                    {"title": "Page A", "url": "/a", "score": 0.8, "excerpt": "a", "date": "2025-01-01"},
                    {"title": "Page B", "url": "/b", "score": 0.7, "excerpt": "b", "date": "2025-01-01"}
                ],
                "weight": 0.8
            }
        ],
        "deduplicate_by": "url"
    });
    let result = inner::merge_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    let urls: Vec<&str> = arr.iter().map(|r| r["url"].as_str().unwrap()).collect();
    assert_eq!(
        urls.iter().filter(|&&u| u == "/a").count(),
        1,
        "URL /a should appear once"
    );
    assert!(urls.contains(&"/b"));
}

#[test]
fn merge_results_old_format_returns_error() {
    let input = json!({
        "original": [{"title": "A", "url": "/a", "score": 1.0, "excerpt": "a", "date": "2025-01-01"}],
        "expanded": []
    });
    assert!(
        inner::merge_results(&input).is_err(),
        "Old original/expanded format should return an error"
    );
}

// ── match_priority_pages ──────────────────────────────────────────────────────

#[test]
fn match_priority_pages_keyword_match() {
    let input = json!({
        "query": "contact the team",
        "priority_pages": [
            {"url_pattern": "/contact", "keywords": ["contact"], "boost": 50.0},
            {"url_pattern": "/blog", "keywords": ["news"], "boost": 10.0}
        ]
    });
    let result = inner::match_priority_pages(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["url_pattern"], "/contact");
}

// ── parse_expansion ───────────────────────────────────────────────────────────

#[test]
fn parse_expansion_generic_terms_filtered() {
    let terms = inner::parse_expansion(
        r#"{"text": "[\"drupal\", \"team\", \"platform\"]", "language": "en", "generic_terms": ["team", "platform"]}"#,
    );
    assert!(terms.contains(&"drupal".to_string()));
    assert!(!terms.contains(&"team".to_string()));
    assert!(!terms.contains(&"platform".to_string()));
}

#[test]
fn parse_expansion_existing_terms_merged() {
    let terms = inner::parse_expansion(
        r#"{"text": "[\"performance\"]", "language": "en", "existing_terms": ["migration"]}"#,
    );
    assert!(terms.contains(&"performance".to_string()));
    assert!(terms.contains(&"migration".to_string()));
}

// ── extract_context ───────────────────────────────────────────────────────────

#[test]
fn extract_context_short_content_unchanged() {
    let input = json!({
        "content": "Short content about drupal.",
        "query": "drupal"
    });
    let result = inner::extract_context(&input).unwrap();
    assert_eq!(result, "Short content about drupal.");
}

#[test]
fn batch_extract_context_returns_all_items() {
    let input = json!({
        "items": [
            {"content": "First doc.", "url": "/a", "title": "A"},
            {"content": "Second doc.", "url": "/b", "title": "B"}
        ],
        "query": "drupal"
    });
    let result = inner::batch_extract_context(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["url"], "/a");
    assert_eq!(arr[1]["url"], "/b");
}

// ── sanitize_query ────────────────────────────────────────────────────────────

#[test]
fn sanitize_query_redacts_email() {
    let input = json!({"query": "contact user@example.com"});
    let result = inner::sanitize_query(&input).unwrap();
    assert!(!result.contains('@'));
    assert!(result.contains("[EMAIL]"));
}

#[test]
fn sanitize_query_passes_clean_query() {
    let input = json!({"query": "drupal performance optimization"});
    let result = inner::sanitize_query(&input).unwrap();
    assert_eq!(result, "drupal performance optimization");
}

/// The four classes reported unredacted in issue #53, checked through the same
/// entry point the WASM export uses.
#[test]
fn sanitize_query_redacts_ipv6_and_unhyphenated_ssn() {
    let cases = [
        (
            "host 2001:0db8:85a3:0000:0000:8a2e:0370:7334 down",
            "host [IP] down",
        ),
        ("host fe80::1 down", "host [IP] down"),
        ("ssn 123 45 6789", "ssn [SSN]"),
        ("ssn 123456789", "ssn [SSN]"),
    ];
    for (query, expected) in cases {
        let result = inner::sanitize_query(&json!({ "query": query })).unwrap();
        assert_eq!(result, expected, "query: {query}");
    }
}

/// The classes that already worked must keep working, and a clean query must
/// not acquire a redaction from the widened patterns.
#[test]
fn sanitize_query_existing_classes_unchanged() {
    let cases = [
        ("contact user@example.com", "contact [EMAIL]"),
        ("call 555-867-5309", "call [PHONE]"),
        ("my ssn is 123-45-6789", "my ssn is [SSN]"),
        ("card 4111 1111 1111 1111", "card [CC]"),
        ("server at 192.168.1.1", "server at [IP]"),
        ("clip at 12:34:56 mark", "clip at 12:34:56 mark"),
    ];
    for (query, expected) in cases {
        let result = inner::sanitize_query(&json!({ "query": query })).unwrap();
        assert_eq!(result, expected, "query: {query}");
    }
}

/// Over-redaction the widened patterns introduced and no longer commit: an
/// address format and a documentation-corpus identifier both read as PII.
#[test]
fn sanitize_query_does_not_over_redact() {
    let unchanged = [
        // 5+4 groupings are not SSNs.
        "zip 12345-6789",
        "ship to 90210-1234",
        "part no 12345-6789",
        // Hex-only namespace syntax. Valid IPv6 by grammar, not an address.
        "db::add docs",
        "abc::def namespace",
        "ec::add curve",
        "cafe::babe example",
    ];
    for query in unchanged {
        let result = inner::sanitize_query(&json!({ "query": query })).unwrap();
        assert_eq!(result, query, "query: {query}");
    }

    // A valid address must be consumed whole, leaving no fragment behind.
    let result = inner::sanitize_query(&json!({ "query": "host 1::2:3:4:5:6:7 down" })).unwrap();
    assert_eq!(result, "host [IP] down");
}

// ── truncate_conversation ─────────────────────────────────────────────────────

#[test]
fn truncate_conversation_preserves_first_n() {
    let input = json!({
        "messages": [
            {"role": "system", "content": "system prompt"},
            {"role": "user", "content": "initial context"},
            {"role": "user", "content": "q1"},
            {"role": "assistant", "content": "a1"}
        ],
        "config": {"max_length": 20, "preserve_first_n": 2, "removal_unit": 2}
    });
    let result = inner::truncate_conversation(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr[0]["content"], "system prompt");
    assert_eq!(arr[1]["content"], "initial context");
}

#[test]
fn truncate_conversation_removes_oldest_pair() {
    let input = json!({
        "messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "initial"},
            {"role": "user", "content": "q1"},
            {"role": "assistant", "content": "a1"},
            {"role": "user", "content": "q2"},
            {"role": "assistant", "content": "a2"}
        ],
        "config": {"max_length": 15, "preserve_first_n": 2, "removal_unit": 2}
    });
    let result = inner::truncate_conversation(&input).unwrap();
    let arr = result.as_array().unwrap();
    let contents: Vec<&str> = arr.iter().map(|m| m["content"].as_str().unwrap()).collect();
    assert!(contents.contains(&"sys"));
    assert!(contents.contains(&"initial"));
    assert!(
        !contents.contains(&"q1"),
        "oldest pair q1 should be removed"
    );
    assert!(
        !contents.contains(&"a1"),
        "oldest pair a1 should be removed"
    );
    assert!(contents.contains(&"q2"));
}

// ── batch_score_results ──────────────────────────────────────────────────────

#[test]
fn batch_score_results_returns_per_query_results() {
    let input = json!({
        "queries": [
            {
                "query": "drupal",
                "results": [
                    {"url": "/a", "title": "Drupal Guide", "excerpt": "drupal info", "date": "2026-01-01"}
                ]
            },
            {
                "query": "wordpress",
                "results": [
                    {"url": "/b", "title": "WordPress Tips", "excerpt": "wp tips", "date": "2026-01-01"}
                ]
            }
        ]
    });
    let result = inner::batch_score_results(&input).unwrap();
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].as_array().unwrap().len(), 1);
    assert_eq!(arr[1].as_array().unwrap().len(), 1);
}

// ── resolve_prompt ───────────────────────────────────────────────────────────

#[test]
fn resolve_prompt_substitutes_variables() {
    let input = json!({
        "prompt_name": "expand_query",
        "site_name": "Acme Corp",
        "site_description": "a widget store"
    });
    let result = inner::resolve_prompt(&input).unwrap();
    assert!(result.contains("Acme Corp"));
    assert!(result.contains("a widget store"));
}

#[test]
fn resolve_prompt_unknown_name_is_err() {
    let input = json!({"prompt_name": "nonexistent_xyz"});
    assert!(inner::resolve_prompt(&input).is_err());
}

// ── get_prompt ───────────────────────────────────────────────────────────────

#[test]
fn get_prompt_returns_raw_template() {
    let result = inner::get_prompt("expand_query").unwrap();
    assert!(result.contains("{SITE_NAME}"));
    assert!(result.contains("{SITE_DESCRIPTION}"));
}

#[test]
fn get_prompt_unknown_name_is_err() {
    assert!(inner::get_prompt("nonexistent_xyz").is_err());
}

// ── version ──────────────────────────────────────────────────────────────────

#[test]
fn version_returns_cargo_version() {
    let v = inner::version();
    assert!(!v.is_empty());
    // Must match Cargo.toml version
    assert_eq!(v, env!("CARGO_PKG_VERSION"));
}

// ── docs ──────────────────────────────────────────────────────────────────────

/// Doc-sync guard: every `ScoringConfig` default documented in `API.md` must
/// equal the corresponding field of `ScoringConfig::default()`. This is what
/// keeps the docs from silently drifting from the code (as they did before the
/// scoring-default alignment).
///
/// The test parses the `| Field | Type | Default | Description |` table and
/// checks every row against the serialized default config, dispatching on the
/// field's actual JSON type so strings and arrays are verified too, not just
/// numbers. A row that cannot be parsed is a hard failure, so a future reformat
/// of the table surfaces here instead of silently skipping the check.
#[test]
fn api_md_documents_actual_scoring_defaults() {
    use scolta_core::scoring::ScoringConfig;

    let api_md = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/API.md"))
        .expect("API.md must be readable");

    let defaults =
        serde_json::to_value(ScoringConfig::default()).expect("ScoringConfig must serialize");
    let defaults = defaults
        .as_object()
        .expect("ScoringConfig must serialize to a JSON object");

    // Locate the ScoringConfig defaults table by its exact header, then walk the
    // contiguous block of rows after the markdown separator.
    let header = "| Field | Type | Default | Description |";
    let table_start = api_md
        .find(header)
        .expect("API.md must contain the ScoringConfig defaults table header");
    let mut lines = api_md[table_start..].lines();
    lines.next(); // header row
    let separator = lines.next().expect("table must have a separator row");
    assert!(
        separator.trim_start().starts_with("|---"),
        "expected a markdown separator row after the header, got: {separator:?}"
    );

    let mut checked = 0;
    for line in lines {
        let line = line.trim();
        if !line.starts_with('|') {
            break; // end of the table
        }
        // Split "| a | b | c | d |" into ["a", "b", "c", "d"].
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        assert!(
            cells.len() >= 3,
            "doc-sync guard: cannot parse defaults-table row: {line:?}"
        );
        let field = cells[0].trim_matches('`').trim();
        let documented = cells[2].trim_matches('`').trim();
        assert!(
            !field.is_empty() && !documented.is_empty(),
            "doc-sync guard: row has empty field or default: {line:?}"
        );

        let actual = defaults.get(field).unwrap_or_else(|| {
            panic!("doc-sync guard: API.md documents unknown ScoringConfig field `{field}`")
        });

        match actual {
            serde_json::Value::Number(n) => {
                let want = n.as_f64().unwrap();
                let got: f64 = documented.parse().unwrap_or_else(|_| {
                    panic!("doc-sync guard: `{field}` default `{documented}` is not numeric, but the field is")
                });
                assert!(
                    (want - got).abs() < 1e-9,
                    "doc-sync guard: `{field}` default mismatch — API.md says {got}, code default is {want}"
                );
            }
            serde_json::Value::String(s) => {
                let doc = documented.trim_matches('"');
                assert_eq!(
                    doc, s,
                    "doc-sync guard: `{field}` default mismatch — API.md says {doc:?}, code default is {s:?}"
                );
            }
            serde_json::Value::Array(a) => {
                assert_eq!(
                    documented, "[]",
                    "doc-sync guard: `{field}` documented default {documented:?} but code default is an array"
                );
                assert!(
                    a.is_empty(),
                    "doc-sync guard: `{field}` documented as [] but code default is non-empty"
                );
            }
            serde_json::Value::Bool(b) => {
                let got: bool = documented.parse().unwrap_or_else(|_| {
                    panic!("doc-sync guard: `{field}` default `{documented}` is not a bool, but the field is")
                });
                assert_eq!(got, *b, "doc-sync guard: `{field}` bool default mismatch");
            }
            other => panic!("doc-sync guard: unhandled default type for `{field}`: {other:?}"),
        }
        checked += 1;
    }

    // Sanity check that we actually parsed the whole table, not an empty/truncated one.
    assert!(
        checked >= 15,
        "doc-sync guard parsed only {checked} rows — the ScoringConfig table looks truncated"
    );
}
