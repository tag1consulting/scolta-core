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
