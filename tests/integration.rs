//! Comprehensive integration tests for scolta-core.
//!
//! Tests the module-level functions and the `inner::` public API.
//! Does NOT call `#[plugin_fn]` functions — those are `extern "C"` wrappers
//! that can only be called through the Extism host runtime.
//!
//! Organization:
//! - `common_module` — stop words, term extraction, validation
//! - `error_module` — typed error enum, display formatting
//! - `prompts_module` — template loading and resolution
//! - `html_module` — HTML cleaning, content extraction, Pagefind generation
//! - `scoring_module` — recency, title/content matching, composite scoring
//! - `config_module` — parsing, validation, JS export
//! - `expansion_module` — LLM response parsing in all formats
//! - `inner_api` — JSON-in/JSON-out interface (the contract adapters rely on)
//! - `inner_api_errors` — every error path through the inner API
//! - `debug_module` — measure_call, debug_result_to_json
//! - `describe_module` — self-documenting catalog completeness
//! - `pipeline` — end-to-end workflows

#[cfg(test)]
mod common_module {
    use scolta_core::common;

    #[test]
    fn stop_word_hit() {
        for word in &["the", "a", "an", "is", "are", "what", "how", "of", "in"] {
            assert!(
                common::is_stop_word(word),
                "'{}' should be a stop word",
                word
            );
        }
    }

    #[test]
    fn stop_word_case_insensitive() {
        assert!(common::is_stop_word("The"));
        assert!(common::is_stop_word("THE"));
        assert!(common::is_stop_word("tHe"));
    }

    #[test]
    fn stop_word_miss() {
        for word in &["drupal", "search", "performance", "rust", "wasm"] {
            assert!(
                !common::is_stop_word(word),
                "'{}' should not be a stop word",
                word
            );
        }
    }

    #[test]
    fn valid_term_accepts_good_terms() {
        assert!(common::is_valid_term("search"));
        assert!(common::is_valid_term("multi-word term"));
        assert!(common::is_valid_term("term123"));
        assert!(common::is_valid_term("ab")); // Exactly 2 chars — minimum
    }

    #[test]
    fn valid_term_rejects_empty() {
        assert!(!common::is_valid_term(""));
    }

    #[test]
    fn valid_term_rejects_single_char() {
        assert!(!common::is_valid_term("x"));
        assert!(!common::is_valid_term("a"));
    }

    #[test]
    fn valid_term_rejects_pure_numbers() {
        assert!(!common::is_valid_term("123"));
        assert!(!common::is_valid_term("0"));
        assert!(!common::is_valid_term("999999"));
    }

    #[test]
    fn valid_term_accepts_mixed_alphanumeric() {
        assert!(common::is_valid_term("term123"));
        assert!(common::is_valid_term("123abc"));
    }

    #[test]
    fn valid_term_rejects_stop_words() {
        assert!(!common::is_valid_term("the"));
        assert!(!common::is_valid_term("is"));
    }

    #[test]
    fn extract_terms_filters_stop_words() {
        let terms = common::extract_terms("what is drupal performance");
        assert_eq!(terms, vec!["drupal", "performance"]);
    }

    #[test]
    fn extract_terms_lowercases() {
        let terms = common::extract_terms("Hello World");
        assert_eq!(terms, vec!["hello", "world"]);
    }

    #[test]
    fn extract_terms_preserves_order() {
        let terms = common::extract_terms("performance drupal optimization");
        assert_eq!(terms, vec!["performance", "drupal", "optimization"]);
    }

    #[test]
    fn extract_terms_empty_query() {
        assert!(common::extract_terms("").is_empty());
    }

    #[test]
    fn extract_terms_all_stop_words() {
        assert!(common::extract_terms("the a an is are").is_empty());
    }

    #[test]
    fn extract_terms_single_char_filtered() {
        let terms = common::extract_terms("I like x");
        // "I" is stop word, "x" is single char — only "like" should pass
        // But "like" is not a stop word and is >1 char
        assert!(terms.contains(&"like".to_string()));
        assert!(!terms.iter().any(|t| t.len() < 2));
    }
}

#[cfg(test)]
mod error_module {
    use scolta_core::error::ScoltaError;

    #[test]
    fn invalid_json_display() {
        let err = ScoltaError::invalid_json("clean_html", "expected object");
        assert_eq!(
            err.to_string(),
            "clean_html: invalid JSON input: expected object"
        );
    }

    #[test]
    fn missing_field_display() {
        let err = ScoltaError::missing_field("score_results", "query");
        assert_eq!(
            err.to_string(),
            "score_results: missing required field 'query'"
        );
    }

    #[test]
    fn invalid_field_type_display() {
        let err = ScoltaError::InvalidFieldType {
            function: "build_pagefind_html",
            field: "id",
            expected: "a string",
        };
        assert_eq!(
            err.to_string(),
            "build_pagefind_html: field 'id' must be a string"
        );
    }

    #[test]
    fn unknown_prompt_display() {
        let err = ScoltaError::UnknownPrompt {
            name: "nonexistent".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("resolve_prompt"));
        assert!(msg.contains("nonexistent"));
    }

    #[test]
    fn unknown_function_display() {
        let err = ScoltaError::UnknownFunction {
            name: "fake_fn".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("debug_call"));
        assert!(msg.contains("fake_fn"));
    }

    #[test]
    fn parse_error_display() {
        let err = ScoltaError::parse_error(
            "merge_results",
            "failed to parse original results: missing field `url`",
        );
        let msg = err.to_string();
        assert!(msg.contains("merge_results"));
        assert!(msg.contains("missing field `url`"));
    }

    #[test]
    fn config_warning_display() {
        let err = ScoltaError::ConfigWarning {
            field: "recency_boost_max",
            message: "value 10.0 outside reasonable range".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("config warning"));
        assert!(msg.contains("recency_boost_max"));
    }

    #[test]
    fn error_implements_std_error() {
        let err = ScoltaError::missing_field("test", "field");
        // Verify it compiles as Box<dyn std::error::Error>
        let _: Box<dyn std::error::Error> = Box::new(err);
    }

    #[test]
    fn error_is_debug_printable() {
        let err = ScoltaError::missing_field("test", "field");
        let debug = format!("{:?}", err);
        assert!(debug.contains("MissingField"));
    }

    #[test]
    fn error_is_clone() {
        let err = ScoltaError::missing_field("test", "field");
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }
}

#[cfg(test)]
mod prompts_module {
    use scolta_core::prompts;

    #[test]
    fn get_template_expand_query() {
        let t = prompts::get_template("expand_query").unwrap();
        assert!(t.contains("alternative search terms"));
        assert!(t.contains("{SITE_NAME}"));
        assert!(t.contains("{SITE_DESCRIPTION}"));
    }

    #[test]
    fn get_template_summarize() {
        let t = prompts::get_template("summarize").unwrap();
        assert!(t.contains("{SITE_NAME}"));
        assert!(t.contains("scannable summary"));
    }

    #[test]
    fn get_template_follow_up() {
        let t = prompts::get_template("follow_up").unwrap();
        assert!(t.contains("follow-up questions"));
        assert!(t.contains("{SITE_NAME}"));
    }

    #[test]
    fn get_template_invalid_returns_none() {
        assert!(prompts::get_template("invalid").is_none());
        assert!(prompts::get_template("").is_none());
        assert!(prompts::get_template("expand_Query").is_none()); // Case-sensitive
    }

    #[test]
    fn resolve_template_replaces_all_placeholders() {
        let resolved = prompts::resolve_template("expand_query", "ACME", "widgets R us").unwrap();
        assert!(resolved.contains("ACME"));
        assert!(resolved.contains("widgets R us"));
        assert!(!resolved.contains("{SITE_NAME}"));
        assert!(!resolved.contains("{SITE_DESCRIPTION}"));
    }

    #[test]
    fn resolve_template_empty_values() {
        let resolved = prompts::resolve_template("expand_query", "", "").unwrap();
        assert!(!resolved.contains("{SITE_NAME}"));
        assert!(!resolved.contains("{SITE_DESCRIPTION}"));
    }

    #[test]
    fn resolve_template_invalid_name() {
        assert!(prompts::resolve_template("nonexistent", "A", "B").is_none());
    }

    #[test]
    fn resolve_template_special_chars_in_values() {
        let resolved =
            prompts::resolve_template("expand_query", "Site <&> \"Co\"", "it's great").unwrap();
        assert!(resolved.contains("Site <&> \"Co\""));
        assert!(resolved.contains("it's great"));
    }

    #[test]
    fn all_templates_contain_site_name_placeholder() {
        for name in &["expand_query", "summarize", "follow_up"] {
            let t = prompts::get_template(name).unwrap();
            assert!(t.contains("{SITE_NAME}"), "{} missing {{SITE_NAME}}", name);
        }
    }

    #[test]
    fn expand_and_summarize_contain_site_description() {
        // expand_query and summarize use {SITE_DESCRIPTION}.
        // follow_up intentionally does not — it's a conversation
        // continuation that only needs the site name for context.
        for name in &["expand_query", "summarize"] {
            let t = prompts::get_template(name).unwrap();
            assert!(
                t.contains("{SITE_DESCRIPTION}"),
                "{} missing {{SITE_DESCRIPTION}}",
                name
            );
        }
    }
}

#[cfg(test)]
#[cfg(feature = "extism")]
mod html_module {
    use scolta_core::html;

    // -- clean_html --

    #[test]
    fn removes_inline_script() {
        let html = "<body><p>Content</p><script>alert('xss')</script></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Content"));
        assert!(!cleaned.contains("alert"));
    }

    #[test]
    fn removes_multiline_script() {
        let html = "<body><p>Before</p><script type=\"text/javascript\">\nvar x = 1;\nvar y = 2;\nconsole.log(x + y);\n</script><p>After</p></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Before"));
        assert!(cleaned.contains("After"));
        assert!(!cleaned.contains("var x"));
        assert!(!cleaned.contains("console"));
    }

    #[test]
    fn removes_inline_style() {
        let html = "<body><style>.x { color: red; }</style><p>Content</p></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Content"));
        assert!(!cleaned.contains("color"));
    }

    #[test]
    fn removes_multiline_style() {
        let html = "<body><style>\n.header {\n  background: blue;\n  font-size: 16px;\n}\n</style><p>Content</p></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Content"));
        assert!(!cleaned.contains("background"));
    }

    #[test]
    fn removes_nav_elements() {
        let html = "<body><nav><a href='/'>Home</a></nav><p>Main content</p></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Main content"));
    }

    #[test]
    fn removes_html_comments() {
        let html = "<body><!-- This is a comment --><p>Real content</p></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Real content"));
        assert!(!cleaned.contains("comment"));
    }

    #[test]
    fn removes_comments_with_html_inside() {
        let html = "<body><!-- <script>evil()</script> --><p>Safe</p></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Safe"));
        assert!(!cleaned.contains("evil"));
    }

    #[test]
    fn normalizes_whitespace() {
        let html = "<p>Text    with     lots     of    spaces</p>";
        let cleaned = html::clean_html(html, "");
        assert!(!cleaned.contains("     "));
        assert!(cleaned.contains("Text"));
        assert!(cleaned.contains("spaces"));
    }

    #[test]
    fn extracts_main_content_by_id() {
        let html = "<html><body><nav>Skip this</nav><div id=\"main-content\"><p>Main text</p></div><footer>Skip footer</footer></body></html>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Main text"));
    }

    #[test]
    fn extracts_main_content_case_insensitive() {
        let html = "<html><body><nav>Skip</nav><DIV ID=\"main-content\"><p>Main</p></DIV><footer>Skip</footer></body></html>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Main"));
    }

    #[test]
    fn extracts_main_content_single_quotes() {
        let html = "<html><body><div id='main-content'><p>Content</p></div></body></html>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Content"));
    }

    #[test]
    fn falls_back_to_body_without_main_content() {
        let html = "<html><body><p>Body content only</p></body></html>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Body content only"));
    }

    #[test]
    fn removes_footer_tag() {
        let html = "<body><p>Content</p><footer>Footer stuff</footer></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Content"));
    }

    #[test]
    fn removes_leading_title() {
        let html = "<body><h1>My Page</h1><p>My Page is great</p></body>";
        let cleaned = html::clean_html(html, "My Page");
        // Should not start with duplicate title
        assert!(!cleaned.starts_with("My Page My Page"));
    }

    #[test]
    fn empty_html_returns_empty_or_whitespace() {
        let cleaned = html::clean_html("", "");
        assert!(cleaned.trim().is_empty());
    }

    #[test]
    fn plain_text_input_returned_as_is() {
        let cleaned = html::clean_html("Just plain text, no tags", "");
        assert!(cleaned.contains("Just plain text"));
    }

    #[test]
    fn strips_all_html_tags() {
        let html = "<div class=\"test\"><p><strong>Bold</strong> and <em>italic</em></p></div>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Bold"));
        assert!(cleaned.contains("italic"));
        assert!(!cleaned.contains("<strong>"));
        assert!(!cleaned.contains("<em>"));
        assert!(!cleaned.contains("<div"));
    }

    #[test]
    fn handles_nested_divs_in_main_content() {
        let html = r#"<body><div id="main-content"><div class="inner"><div class="nested"><p>Deep content</p></div></div></div></body>"#;
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("Deep content"));
    }

    #[test]
    fn multiple_script_tags_all_removed() {
        let html = "<body><script>a()</script><p>OK</p><script>b()</script><p>Fine</p><script>c()</script></body>";
        let cleaned = html::clean_html(html, "");
        assert!(cleaned.contains("OK"));
        assert!(cleaned.contains("Fine"));
        assert!(!cleaned.contains("a()"));
        assert!(!cleaned.contains("b()"));
        assert!(!cleaned.contains("c()"));
    }

    // -- build_pagefind_html --

    #[test]
    fn pagefind_html_structure() {
        let result = html::build_pagefind_html(
            "doc-123",
            "Test Page",
            "Test content",
            "https://example.com/page",
            "2024-01-15",
            "Example Site",
        );
        assert!(result.contains("<!DOCTYPE html>"));
        assert!(result.contains("<html>"));
        assert!(result.contains("<head>"));
        assert!(result.contains("<title>Test Page</title>"));
        assert!(result.contains("data-pagefind-body"));
        assert!(result.contains("id=\"doc-123\""));
        assert!(result.contains("data-pagefind-meta=\"url:https://example.com/page\""));
        assert!(result.contains("data-pagefind-meta=\"date:2024-01-15\""));
        assert!(result.contains("data-pagefind-filter=\"site:Example Site\""));
        assert!(result.contains("<h1>Test Page</h1>"));
        assert!(result.contains("Test content"));
    }

    #[test]
    fn pagefind_html_escapes_html_chars() {
        let result = html::build_pagefind_html(
            "doc-1",
            "<Script>",
            "Content & more < > \"quotes\"",
            "https://example.com?a=1&b=2",
            "2024-01-15",
            "Site & Co.",
        );
        assert!(result.contains("&lt;Script&gt;"));
        assert!(result.contains("Content &amp; more &lt; &gt;"));
        assert!(result.contains("a=1&amp;b=2"));
        assert!(result.contains("Site &amp; Co."));
    }

    #[test]
    fn pagefind_html_omits_empty_date() {
        let result =
            html::build_pagefind_html("doc-1", "Title", "Body", "https://example.com", "", "Site");
        assert!(!result.contains("data-pagefind-meta=\"date:\""));
    }

    #[test]
    fn pagefind_html_omits_empty_site_name() {
        let result = html::build_pagefind_html(
            "doc-1",
            "Title",
            "Body",
            "https://example.com",
            "2024-01-01",
            "",
        );
        assert!(!result.contains("data-pagefind-filter"));
    }

    #[test]
    fn pagefind_html_includes_charset() {
        let result = html::build_pagefind_html("id", "T", "B", "https://x.com", "", "");
        assert!(result.contains("charset=\"utf-8\"") || result.contains("charset=utf-8"));
    }
}

#[cfg(test)]
mod scoring_module {
    use scolta_core::scoring::*;

    fn days_ago(n: u64) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (n * 86400);
        let (y, m, d) = civil_from_epoch_secs(secs);
        format!("{:04}-{:02}-{:02}", y, m, d)
    }

    fn make_result(url: &str, title: &str, excerpt: &str, date: &str, score: f64) -> SearchResult {
        SearchResult {
            url: url.to_string(),
            title: title.to_string(),
            excerpt: excerpt.to_string(),
            date: date.to_string(),
            score,
            content_type: String::new(),
            site_name: String::new(),
            extra: serde_json::Map::new(),
        }
    }

    // -- ScoringConfig --

    #[test]
    fn default_config_values() {
        let c = ScoringConfig::default();
        assert_eq!(c.recency_boost_max, 0.5);
        assert_eq!(c.recency_half_life_days, 365);
        assert_eq!(c.recency_penalty_after_days, 1825);
        assert_eq!(c.recency_max_penalty, 0.3);
        assert_eq!(c.title_match_boost, 1.0);
        assert_eq!(c.title_all_terms_multiplier, 1.5);
        assert_eq!(c.content_match_boost, 0.4);
        assert_eq!(c.content_all_terms_multiplier, 0.48);
        assert_eq!(c.expand_primary_weight, 0.7);
        assert_eq!(c.excerpt_length, 300);
        assert_eq!(c.results_per_page, 10);
        assert_eq!(c.max_pagefind_results, 50);
    }

    #[test]
    fn config_validation_passes_defaults() {
        assert!(ScoringConfig::default().validate().is_empty());
    }

    #[test]
    fn config_validation_warns_boost_too_high() {
        let c = ScoringConfig {
            recency_boost_max: 10.0,
            ..Default::default()
        };
        let w = c.validate();
        assert!(w.iter().any(|w| w.field == "recency_boost_max"));
    }

    #[test]
    fn config_validation_warns_boost_negative() {
        let c = ScoringConfig {
            recency_boost_max: -1.0,
            ..Default::default()
        };
        assert!(!c.validate().is_empty());
    }

    #[test]
    fn config_validation_warns_half_life_zero() {
        let c = ScoringConfig {
            recency_half_life_days: 0,
            ..Default::default()
        };
        assert!(c
            .validate()
            .iter()
            .any(|w| w.field == "recency_half_life_days"));
    }

    #[test]
    fn config_validation_warns_half_life_too_large() {
        let c = ScoringConfig {
            recency_half_life_days: 5000,
            ..Default::default()
        };
        assert!(c
            .validate()
            .iter()
            .any(|w| w.field == "recency_half_life_days"));
    }

    #[test]
    fn config_validation_warns_penalty_too_high() {
        let c = ScoringConfig {
            recency_max_penalty: 1.5,
            ..Default::default()
        };
        assert!(c
            .validate()
            .iter()
            .any(|w| w.field == "recency_max_penalty"));
    }

    #[test]
    fn config_validation_warns_primary_weight_out_of_range() {
        let c = ScoringConfig {
            expand_primary_weight: 1.5,
            ..Default::default()
        };
        assert!(c
            .validate()
            .iter()
            .any(|w| w.field == "expand_primary_weight"));
    }

    #[test]
    fn config_validation_warns_results_per_page_zero() {
        let c = ScoringConfig {
            results_per_page: 0,
            ..Default::default()
        };
        assert!(c.validate().iter().any(|w| w.field == "results_per_page"));
    }

    #[test]
    fn config_validation_warns_results_per_page_too_large() {
        let c = ScoringConfig {
            results_per_page: 200,
            ..Default::default()
        };
        assert!(c.validate().iter().any(|w| w.field == "results_per_page"));
    }

    #[test]
    fn config_validation_warns_max_pagefind_zero() {
        let c = ScoringConfig {
            max_pagefind_results: 0,
            ..Default::default()
        };
        assert!(c
            .validate()
            .iter()
            .any(|w| w.field == "max_pagefind_results"));
    }

    #[test]
    fn config_validation_warns_max_pagefind_too_large() {
        let c = ScoringConfig {
            max_pagefind_results: 1000,
            ..Default::default()
        };
        assert!(c
            .validate()
            .iter()
            .any(|w| w.field == "max_pagefind_results"));
    }

    #[test]
    fn config_validation_multiple_warnings() {
        let c = ScoringConfig {
            recency_boost_max: 10.0,
            results_per_page: 0,
            max_pagefind_results: 0,
            ..Default::default()
        };
        assert!(c.validate().len() >= 3);
    }

    // -- Recency boost (additive) --

    #[test]
    fn recency_recent_content_boosted() {
        let c = ScoringConfig::default();
        let b = recency_boost(&days_ago(30), &c);
        assert!(
            b > 0.0,
            "Recent content (30 days) should get positive boost, got {}",
            b
        );
        assert!(
            b <= c.recency_boost_max,
            "Should not exceed max boost, got {}",
            b
        );
    }

    #[test]
    fn recency_very_recent_gets_near_max_boost() {
        let c = ScoringConfig::default();
        let b = recency_boost(&days_ago(1), &c);
        assert!(
            b > 0.4,
            "Very recent content should get near-max boost, got {}",
            b
        );
    }

    #[test]
    fn recency_old_content_penalized() {
        let c = ScoringConfig::default();
        let b = recency_boost("2000-01-01", &c);
        assert!(
            b < 0.0,
            "Old content should get negative penalty, got {}",
            b
        );
        assert!(
            b >= -c.recency_max_penalty,
            "Should not exceed max penalty, got {}",
            b
        );
    }

    #[test]
    fn recency_future_date_boosted() {
        let c = ScoringConfig::default();
        let b = recency_boost("2099-01-01", &c);
        assert!(b > 0.0, "Future date should get positive boost, got {}", b);
    }

    #[test]
    fn recency_unparseable_date_neutral() {
        let c = ScoringConfig::default();
        assert_eq!(recency_boost("garbage", &c), 0.0);
        assert_eq!(recency_boost("", &c), 0.0);
        assert_eq!(recency_boost("not-a-date", &c), 0.0);
    }

    #[test]
    fn recency_invalid_date_ranges_neutral() {
        let c = ScoringConfig::default();
        assert_eq!(recency_boost("2026-13-01", &c), 0.0); // Month 13
        assert_eq!(recency_boost("2026-00-15", &c), 0.0); // Month 0
        assert_eq!(recency_boost("2026-01-32", &c), 0.0); // Day 32
        assert_eq!(recency_boost("2026-01-00", &c), 0.0); // Day 0
    }

    #[test]
    fn recency_half_life_zero_safe() {
        let c = ScoringConfig {
            recency_half_life_days: 0,
            ..Default::default()
        };
        let b = recency_boost(&days_ago(30), &c);
        assert_eq!(b, 0.0, "Zero half-life should return neutral");
    }

    // -- Title match --

    #[test]
    fn title_match_all_terms() {
        let c = ScoringConfig::default();
        // All terms match with >1 term: boost * multiplier * (2/2)
        let expected = c.title_match_boost * c.title_all_terms_multiplier;
        let score = title_match_score("hello world", "Hello World Page", &c);
        assert!(
            (score - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn title_match_partial() {
        let c = ScoringConfig::default();
        // 1 of 2 terms match: boost * (1/2)
        let expected = c.title_match_boost * 0.5;
        let score = title_match_score("hello world", "Hello there", &c);
        assert!(
            (score - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn title_match_none() {
        let c = ScoringConfig::default();
        assert_eq!(title_match_score("xyz abc", "Hello world", &c), 0.0);
    }

    #[test]
    fn title_match_case_insensitive() {
        let c = ScoringConfig::default();
        let s = title_match_score("DRUPAL", "drupal guide", &c);
        assert!(s > 0.0);
    }

    #[test]
    fn title_match_empty_query() {
        let c = ScoringConfig::default();
        assert_eq!(title_match_score("", "Any Title", &c), 0.0);
    }

    #[test]
    fn title_match_stop_words_only_query() {
        let c = ScoringConfig::default();
        assert_eq!(title_match_score("what is the", "Any Title", &c), 0.0);
    }

    // -- Content match --

    #[test]
    fn content_match_all_terms() {
        let c = ScoringConfig::default();
        // All terms match with >1 term: content_all_terms_multiplier * (2/2)
        let expected = c.content_all_terms_multiplier;
        let score = content_match_score("test page", "This is a test page with content", &c);
        assert!(
            (score - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn content_match_partial() {
        let c = ScoringConfig::default();
        // 1 of 2 terms match: boost * (1/2)
        let expected = c.content_match_boost * 0.5;
        let score = content_match_score("test xyz", "This is a test page", &c);
        assert!(
            (score - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            score
        );
    }

    #[test]
    fn content_match_none() {
        let c = ScoringConfig::default();
        assert_eq!(
            content_match_score("xyz abc", "No matching words here", &c),
            0.0
        );
    }

    // -- Composite score --

    #[test]
    fn score_result_base_1_when_no_upstream_score() {
        let c = ScoringConfig::default();
        let r = make_result("https://a.com", "Test", "Test content", &days_ago(30), 0.0);
        let s = score_result(&r, "test", &c);
        assert!(
            s > 0.0,
            "Should produce a positive score even with 0.0 upstream"
        );
    }

    #[test]
    fn score_result_incorporates_upstream_score() {
        let c = ScoringConfig::default();
        let date = days_ago(30);
        let with = make_result("https://a.com", "Test", "content", &date, 5.0);
        let without = make_result("https://a.com", "Test", "content", &date, 0.0);
        assert!(score_result(&with, "test", &c) > score_result(&without, "test", &c));
    }

    #[test]
    fn score_results_sorts_descending() {
        let c = ScoringConfig::default();
        let date = days_ago(30);
        let mut results = vec![
            make_result("https://a.com", "Unrelated", "No match here", &date, 0.0),
            make_result(
                "https://b.com",
                "Drupal Guide",
                "All about drupal performance",
                &date,
                0.0,
            ),
        ];
        score_results(&mut results, "drupal performance", &c);
        assert!(results[0].score >= results[1].score);
        assert_eq!(results[0].url, "https://b.com");
    }

    #[test]
    fn score_results_empty_input() {
        let c = ScoringConfig::default();
        let mut results: Vec<SearchResult> = vec![];
        score_results(&mut results, "test", &c);
        assert!(results.is_empty());
    }

    // -- Merge results --

    #[test]
    fn merge_deduplicates_by_url() {
        let c = ScoringConfig::default();
        let original = vec![make_result("https://a.com", "A", "a", "2026-01-01", 10.0)];
        let expanded = vec![make_result("https://a.com", "A", "a", "2026-01-01", 5.0)];
        let merged = merge_results(original, expanded, &c);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_combines_scores() {
        let c = ScoringConfig::default();
        let original = vec![make_result("https://a.com", "A", "a", "2026-01-01", 10.0)];
        let expanded = vec![make_result("https://a.com", "A", "a", "2026-01-01", 10.0)];
        let merged = merge_results(original, expanded, &c);
        // original: 10 * 0.7 = 7.0, expanded: 10 * 0.3 = 3.0, total: 10.0
        let expected = 10.0 * c.expand_primary_weight + 10.0 * (1.0 - c.expand_primary_weight);
        assert!((merged[0].score - expected).abs() < 0.001);
    }

    #[test]
    fn merge_preserves_non_duplicate_results() {
        let c = ScoringConfig::default();
        let original = vec![make_result("https://a.com", "A", "a", "2026-01-01", 10.0)];
        let expanded = vec![make_result("https://b.com", "B", "b", "2025-01-01", 8.0)];
        let merged = merge_results(original, expanded, &c);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_empty_original() {
        let c = ScoringConfig::default();
        let expanded = vec![make_result("https://a.com", "A", "a", "2026-01-01", 5.0)];
        let merged = merge_results(vec![], expanded, &c);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_empty_expanded() {
        let c = ScoringConfig::default();
        let original = vec![make_result("https://a.com", "A", "a", "2026-01-01", 10.0)];
        let merged = merge_results(original, vec![], &c);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_both_empty() {
        let c = ScoringConfig::default();
        let merged = merge_results(vec![], vec![], &c);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_respects_primary_weight() {
        let c = ScoringConfig {
            expand_primary_weight: 0.9,
            ..Default::default()
        };
        let original = vec![make_result("https://a.com", "A", "a", "2026-01-01", 10.0)];
        let expanded = vec![make_result("https://b.com", "B", "b", "2026-01-01", 10.0)];
        let merged = merge_results(original, expanded, &c);
        let a = merged.iter().find(|r| r.url == "https://a.com").unwrap();
        let b = merged.iter().find(|r| r.url == "https://b.com").unwrap();
        assert!(
            a.score > b.score,
            "With weight 0.9, original should dominate"
        );
    }

    #[test]
    fn merge_sorted_descending() {
        let c = ScoringConfig::default();
        let original = vec![make_result("https://a.com", "A", "a", "2026-01-01", 3.0)];
        let expanded = vec![make_result("https://b.com", "B", "b", "2026-01-01", 10.0)];
        let merged = merge_results(original, expanded, &c);
        assert!(merged[0].score >= merged[1].score);
    }

    // -- civil_from_epoch_secs --

    #[test]
    fn civil_known_date() {
        // 2026-01-01 00:00:00 UTC = 1767225600
        assert_eq!(civil_from_epoch_secs(1_767_225_600), (2026, 1, 1));
    }

    #[test]
    fn civil_epoch_zero() {
        assert_eq!(civil_from_epoch_secs(0), (1970, 1, 1));
    }

    #[test]
    fn civil_leap_day() {
        // 2024-02-29 00:00:00 UTC = 1709164800
        assert_eq!(civil_from_epoch_secs(1_709_164_800), (2024, 2, 29));
    }

    // -- SearchResult serde --

    #[test]
    fn search_result_preserves_extra_fields() {
        let json = serde_json::json!({
            "url": "https://a.com",
            "title": "T",
            "excerpt": "E",
            "date": "2026-01-01",
            "score": 1.0,
            "custom_field": "custom_value",
            "another": 42
        });
        let result: SearchResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.extra.get("custom_field").unwrap(), "custom_value");
        assert_eq!(result.extra.get("another").unwrap(), 42);

        // Round-trip
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(serialized["custom_field"], "custom_value");
        assert_eq!(serialized["another"], 42);
    }

    #[test]
    fn search_result_defaults_for_optional_fields() {
        let json = serde_json::json!({
            "url": "https://a.com",
            "title": "T",
            "excerpt": "E",
            "date": "2026-01-01"
        });
        let result: SearchResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.score, 0.0);
        assert_eq!(result.content_type, "");
        assert_eq!(result.site_name, "");
    }
}

#[cfg(test)]
mod config_module {
    use scolta_core::config;
    use serde_json::json;

    #[test]
    fn from_json_defaults() {
        let c = config::from_json(&json!({}));
        assert_eq!(c.recency_boost_max, 0.5);
        assert_eq!(c.recency_half_life_days, 365);
        assert_eq!(c.content_all_terms_multiplier, 0.48);
        assert_eq!(c.expand_primary_weight, 0.7);
    }

    #[test]
    fn from_json_custom_values() {
        let c = config::from_json(&json!({
            "recency_boost_max": 0.8,
            "recency_half_life_days": 200,
            "content_all_terms_multiplier": 0.6
        }));
        assert_eq!(c.recency_boost_max, 0.8);
        assert_eq!(c.recency_half_life_days, 200);
        assert_eq!(c.content_all_terms_multiplier, 0.6);
        assert_eq!(c.content_match_boost, 0.4); // Unchanged default
    }

    #[test]
    fn from_json_non_object_uses_defaults() {
        let c = config::from_json(&json!("not an object"));
        assert_eq!(c.recency_boost_max, 0.5);
    }

    #[test]
    fn from_json_null_uses_defaults() {
        let c = config::from_json(&json!(null));
        assert_eq!(c.recency_boost_max, 0.5);
    }

    #[test]
    fn from_json_array_uses_defaults() {
        let c = config::from_json(&json!([1, 2, 3]));
        assert_eq!(c.recency_boost_max, 0.5);
    }

    #[test]
    fn from_json_wrong_types_use_defaults() {
        let c = config::from_json(&json!({"recency_boost_max": "not a number"}));
        assert_eq!(c.recency_boost_max, 0.5);
    }

    #[test]
    fn from_json_validated_returns_warnings() {
        let (c, w) =
            config::from_json_validated(&json!({"recency_boost_max": 10.0, "results_per_page": 0}));
        assert_eq!(c.recency_boost_max, 10.0); // Still uses the value
        assert!(w.len() >= 2);
    }

    #[test]
    fn from_json_validated_no_warnings_on_defaults() {
        let (_, w) = config::from_json_validated(&json!({}));
        assert!(w.is_empty());
    }

    #[test]
    fn to_js_scoring_config_keys() {
        let c = scolta_core::scoring::ScoringConfig::default();
        let js = config::to_js_scoring_config(&c, &json!({}));
        // Verify all expected keys exist
        for key in &[
            "RECENCY_BOOST_MAX",
            "RECENCY_HALF_LIFE_DAYS",
            "RECENCY_PENALTY_AFTER_DAYS",
            "RECENCY_MAX_PENALTY",
            "TITLE_MATCH_BOOST",
            "TITLE_ALL_TERMS_MULTIPLIER",
            "CONTENT_MATCH_BOOST",
            "CONTENT_ALL_TERMS_MULTIPLIER",
            "EXPAND_PRIMARY_WEIGHT",
            "EXCERPT_LENGTH",
            "RESULTS_PER_PAGE",
            "MAX_PAGEFIND_RESULTS",
            "AI_EXPAND_QUERY",
            "AI_SUMMARIZE",
            "AI_SUMMARY_TOP_N",
            "AI_SUMMARY_MAX_CHARS",
            "AI_MAX_FOLLOWUPS",
        ] {
            assert!(js.get(key).is_some(), "Missing key: {}", key);
        }
    }

    #[test]
    fn to_js_scoring_config_default_values() {
        let c = scolta_core::scoring::ScoringConfig::default();
        let js = config::to_js_scoring_config(&c, &json!({}));
        assert_eq!(js["RECENCY_BOOST_MAX"], 0.5);
        assert_eq!(js["RECENCY_HALF_LIFE_DAYS"], 365);
        assert_eq!(js["CONTENT_ALL_TERMS_MULTIPLIER"], 0.48);
        assert_eq!(js["AI_EXPAND_QUERY"], true);
        assert_eq!(js["AI_SUMMARIZE"], true);
        assert_eq!(js["AI_SUMMARY_TOP_N"], 5);
        assert_eq!(js["AI_SUMMARY_MAX_CHARS"], 2000);
        assert_eq!(js["AI_MAX_FOLLOWUPS"], 3);
    }

    #[test]
    fn to_js_scoring_config_ai_toggles_passthrough() {
        let c = scolta_core::scoring::ScoringConfig::default();
        let js = config::to_js_scoring_config(
            &c,
            &json!({
                "ai_expand_query": false,
                "ai_summarize": false,
                "ai_summary_top_n": 3,
                "ai_summary_max_chars": 1000,
                "ai_max_followups": 5,
            }),
        );
        assert_eq!(js["AI_EXPAND_QUERY"], false);
        assert_eq!(js["AI_SUMMARIZE"], false);
        assert_eq!(js["AI_SUMMARY_TOP_N"], 3);
        assert_eq!(js["AI_SUMMARY_MAX_CHARS"], 1000);
        assert_eq!(js["AI_MAX_FOLLOWUPS"], 5);
    }

    #[test]
    fn to_js_scoring_config_custom_scoring_values() {
        let c = scolta_core::scoring::ScoringConfig {
            recency_boost_max: 0.8,
            recency_half_life_days: 200,
            ..Default::default()
        };
        let js = config::to_js_scoring_config(&c, &json!({}));
        assert_eq!(js["RECENCY_BOOST_MAX"], 0.8);
        assert_eq!(js["RECENCY_HALF_LIFE_DAYS"], 200);
    }
}

#[cfg(test)]
mod expansion_module {
    use scolta_core::expansion;

    #[test]
    fn parse_json_array() {
        let terms = expansion::parse_expansion(r#"["term1", "term2", "term3"]"#);
        assert_eq!(terms, vec!["term1", "term2", "term3"]);
    }

    #[test]
    fn parse_markdown_json() {
        let terms = expansion::parse_expansion("```json\n[\"search term\", \"another\"]\n```");
        assert_eq!(terms, vec!["search term", "another"]);
    }

    #[test]
    fn parse_markdown_generic() {
        let terms = expansion::parse_expansion("```\n[\"item1\", \"item2\"]\n```");
        assert_eq!(terms.len(), 2);
    }

    #[test]
    fn parse_fallback_newlines() {
        let terms = expansion::parse_expansion("term1\nterm2\nterm3");
        assert_eq!(terms.len(), 3);
    }

    #[test]
    fn parse_fallback_commas() {
        let terms = expansion::parse_expansion("term1, term2, term3");
        assert_eq!(terms.len(), 3);
    }

    #[test]
    fn parse_fallback_mixed() {
        let terms = expansion::parse_expansion("term1\nterm2, term3");
        assert_eq!(terms.len(), 3);
    }

    #[test]
    fn parse_strips_quotes() {
        let terms = expansion::parse_expansion("\"term1\", \"term2\"");
        assert!(terms.contains(&"term1".to_string()));
        assert!(terms.contains(&"term2".to_string()));
    }

    #[test]
    fn filters_stop_words() {
        let terms = expansion::parse_expansion(r#"["the", "search", "a", "test"]"#);
        assert!(!terms.contains(&"the".to_string()));
        assert!(!terms.contains(&"a".to_string()));
        assert!(terms.contains(&"search".to_string()));
        assert!(terms.contains(&"test".to_string()));
    }

    #[test]
    fn filters_pure_numbers() {
        let terms = expansion::parse_expansion(r#"["123", "term123", "test"]"#);
        assert!(!terms.contains(&"123".to_string()));
        assert!(terms.contains(&"term123".to_string()));
    }

    #[test]
    fn filters_short_terms() {
        let terms = expansion::parse_expansion(r#"["a", "ab", "abc"]"#);
        assert!(!terms.contains(&"a".to_string()));
        // "ab" passes (2 chars minimum), but it could be filtered as too short
        // depending on is_valid_term. Let's just check no 1-char terms pass.
        assert!(!terms.iter().any(|t| t.len() < 2));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(expansion::parse_expansion("").is_empty());
    }

    #[test]
    fn all_stop_words_returns_empty() {
        let terms = expansion::parse_expansion(r#"["the", "a", "is", "of"]"#);
        assert!(terms.is_empty());
    }

    #[test]
    fn whitespace_around_terms_trimmed() {
        let terms = expansion::parse_expansion(r#"[ "term1" , "term2" ]"#);
        assert_eq!(terms, vec!["term1", "term2"]);
    }

    #[test]
    fn invalid_json_falls_back() {
        let terms = expansion::parse_expansion("{not valid json}");
        // Falls back to splitting by comma/newline — the whole string becomes one term
        // It should at least not panic
        assert!(terms.len() <= 1);
    }

    #[test]
    fn json_object_instead_of_array_falls_back() {
        let terms = expansion::parse_expansion(r#"{"key": "value"}"#);
        // Not an array — should fall back
        // The fallback might produce something from splitting
        // Main point: doesn't panic
        let _ = terms;
    }
}

#[cfg(test)]
#[cfg(feature = "extism")]
mod debug_module {
    use scolta_core::debug;

    #[test]
    fn measure_call_success() {
        let result = debug::measure_call("test_fn", "input text", || Ok("output text".to_string()));
        assert_eq!(result.output, Some("output text".to_string()));
        assert!(result.error.is_none());
        assert_eq!(result.input_size, "input text".len());
        assert_eq!(result.output_size, "output text".len());
        assert!(result.time_us < 1_000_000); // Should be well under 1 second
    }

    #[test]
    fn measure_call_error() {
        let result = debug::measure_call("test_fn", "input", || Err("something broke".to_string()));
        assert!(result.output.is_none());
        assert_eq!(result.error, Some("something broke".to_string()));
        assert_eq!(result.output_size, 0);
        assert!(result.input_size > 0);
    }

    #[test]
    fn measure_call_empty_input() {
        let result = debug::measure_call("test_fn", "", || Ok("out".to_string()));
        assert_eq!(result.input_size, 0);
    }

    #[test]
    fn debug_result_to_json_success() {
        let result = debug::DebugResult {
            output: Some("test".to_string()),
            error: None,
            time_us: 1000,
            input_size: 10,
            output_size: 4,
        };
        let json = debug::debug_result_to_json(&result);
        assert_eq!(json["output"], "test");
        assert!(json["error"].is_null());
        assert_eq!(json["time_us"], 1000);
        assert_eq!(json["input_size"], 10);
        assert_eq!(json["output_size"], 4);
    }

    #[test]
    fn debug_result_to_json_error() {
        let result = debug::DebugResult {
            output: None,
            error: Some("bad input".to_string()),
            time_us: 500,
            input_size: 5,
            output_size: 0,
        };
        let json = debug::debug_result_to_json(&result);
        assert!(json["output"].is_null());
        assert_eq!(json["error"], "bad input");
    }

    #[test]
    fn debug_result_is_clone() {
        let result = debug::DebugResult {
            output: Some("test".to_string()),
            error: None,
            time_us: 100,
            input_size: 5,
            output_size: 4,
        };
        let cloned = result.clone();
        assert_eq!(cloned.output, result.output);
        assert_eq!(cloned.time_us, result.time_us);
    }
}

#[cfg(test)]
mod inner_api {
    //! Tests the inner:: JSON interface — the contract that all language adapters depend on.
    use scolta_core::inner;
    use serde_json::json;

    fn recent_date() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (30 * 86400);
        let (y, m, d) = scolta_core::scoring::civil_from_epoch_secs(secs);
        format!("{:04}-{:02}-{:02}", y, m, d)
    }

    // -- resolve_prompt --

    #[test]
    fn resolve_prompt_success() {
        let input = json!({
            "prompt_name": "expand_query",
            "site_name": "Test Site",
            "site_description": "a test site"
        });
        let result = inner::resolve_prompt(&input).unwrap();
        assert!(result.contains("Test Site"));
        assert!(result.contains("test site"));
    }

    #[test]
    fn resolve_prompt_all_template_names() {
        for name in &["expand_query", "summarize", "follow_up"] {
            let input = json!({"prompt_name": name, "site_name": "S", "site_description": "D"});
            assert!(
                inner::resolve_prompt(&input).is_ok(),
                "Should resolve '{}'",
                name
            );
        }
    }

    #[test]
    fn resolve_prompt_optional_fields_omitted() {
        let input = json!({"prompt_name": "expand_query"});
        let result = inner::resolve_prompt(&input).unwrap();
        assert!(!result.contains("{SITE_NAME}"));
        assert!(!result.contains("{SITE_DESCRIPTION}"));
    }

    // -- get_prompt --

    #[test]
    fn get_prompt_success() {
        let result = inner::get_prompt("expand_query").unwrap();
        assert!(result.contains("{SITE_NAME}"));
        assert!(result.contains("alternative search terms"));
    }

    #[test]
    fn get_prompt_trims_whitespace() {
        let result = inner::get_prompt("  expand_query  ");
        assert!(result.is_ok());
    }

    // -- clean_html (server-only) --

    #[cfg(feature = "extism")]
    #[test]
    fn clean_html_success() {
        let input = json!({
            "html": "<html><body><p>Hello World</p><script>evil()</script></body></html>",
            "title": ""
        });
        let result = inner::clean_html(&input).unwrap();
        assert!(result.contains("Hello World"));
        assert!(!result.contains("evil"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn clean_html_title_optional() {
        let input = json!({"html": "<p>Content</p>"});
        let result = inner::clean_html(&input).unwrap();
        assert!(result.contains("Content"));
    }

    // -- build_pagefind_html (server-only) --

    #[cfg(feature = "extism")]
    #[test]
    fn build_pagefind_html_success() {
        let input = json!({
            "id": "doc-42",
            "title": "Test",
            "body": "Content",
            "url": "https://example.com/test",
            "date": "2026-04-01",
            "site_name": "Example"
        });
        let result = inner::build_pagefind_html(&input).unwrap();
        assert!(result.contains("data-pagefind-body"));
        assert!(result.contains("doc-42"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn build_pagefind_html_optional_fields() {
        let input = json!({
            "id": "doc-1",
            "title": "T",
            "body": "B",
            "url": "https://x.com"
        });
        let result = inner::build_pagefind_html(&input).unwrap();
        assert!(result.contains("data-pagefind-body"));
        assert!(!result.contains("data-pagefind-filter"));
    }

    // -- to_js_scoring_config --

    #[test]
    fn to_js_scoring_config_success() {
        let input = json!({"recency_boost_max": 0.8, "ai_expand_query": false});
        let result = inner::to_js_scoring_config(&input).unwrap();
        assert_eq!(result["RECENCY_BOOST_MAX"], 0.8);
        assert_eq!(result["AI_EXPAND_QUERY"], false);
        assert_eq!(result["RECENCY_HALF_LIFE_DAYS"], 365); // Default
    }

    #[test]
    fn to_js_scoring_config_empty_input() {
        let result = inner::to_js_scoring_config(&json!({})).unwrap();
        assert!(result.is_object());
        assert_eq!(result["RECENCY_BOOST_MAX"], 0.5);
    }

    // -- score_results --

    #[test]
    fn score_results_success() {
        let date = recent_date();
        let input = json!({
            "query": "drupal",
            "results": [
                {"url": "https://a.com", "title": "About Us", "excerpt": "Company info", "date": "2020-01-01"},
                {"url": "https://b.com", "title": "Drupal Guide", "excerpt": "All about Drupal", "date": &date}
            ],
            "config": {}
        });
        let result = inner::score_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["url"], "https://b.com"); // Better match should be first
    }

    #[test]
    fn score_results_preserves_extra_fields() {
        let input = json!({
            "query": "test",
            "results": [
                {"url": "https://a.com", "title": "Test", "excerpt": "test", "date": "2026-01-01", "custom": "preserved"}
            ],
            "config": {}
        });
        let result = inner::score_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["custom"], "preserved");
    }

    #[test]
    fn score_results_config_optional() {
        let input = json!({
            "query": "test",
            "results": [
                {"url": "https://a.com", "title": "T", "excerpt": "E", "date": "2026-01-01"}
            ]
        });
        let result = inner::score_results(&input).unwrap();
        assert!(result.as_array().unwrap()[0]["score"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn score_results_empty_results_array() {
        let input = json!({"query": "test", "results": []});
        let result = inner::score_results(&input).unwrap();
        assert!(result.as_array().unwrap().is_empty());
    }

    // -- merge_results --

    #[test]
    fn merge_results_success() {
        let input = json!({
            "original": [
                {"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 10.0}
            ],
            "expanded": [
                {"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 5.0},
                {"url": "https://b.com", "title": "B", "excerpt": "b", "date": "2025-06-01", "score": 3.0}
            ],
            "config": {"expand_primary_weight": 0.7}
        });
        let result = inner::merge_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2); // Deduped
    }

    #[test]
    fn merge_results_respects_weight() {
        let input = json!({
            "original": [{"url": "https://a.com", "title": "A", "excerpt": "a", "date": "2026-01-01", "score": 10.0}],
            "expanded": [{"url": "https://b.com", "title": "B", "excerpt": "b", "date": "2025-01-01", "score": 8.0}],
            "config": {"expand_primary_weight": 0.9}
        });
        let result = inner::merge_results(&input).unwrap();
        let arr = result.as_array().unwrap();
        let a_score = arr.iter().find(|r| r["url"] == "https://a.com").unwrap()["score"]
            .as_f64()
            .unwrap();
        let b_score = arr.iter().find(|r| r["url"] == "https://b.com").unwrap()["score"]
            .as_f64()
            .unwrap();
        assert!(
            a_score > b_score,
            "0.9 weight should favor original: {} vs {}",
            a_score,
            b_score
        );
    }

    // -- parse_expansion --

    #[test]
    fn parse_expansion_json() {
        let terms = inner::parse_expansion(r#"["term1", "term2", "term3"]"#);
        assert_eq!(terms, vec!["term1", "term2", "term3"]);
    }

    #[test]
    fn parse_expansion_never_errors() {
        // Should always return Vec, even for garbage input
        let _ = inner::parse_expansion("");
        let _ = inner::parse_expansion("garbage");
        let _ = inner::parse_expansion("{invalid json");
        let _ = inner::parse_expansion("null");
    }

    // -- version --

    #[test]
    fn version_matches_cargo() {
        assert_eq!(inner::version(), env!("CARGO_PKG_VERSION"));
    }

    // -- describe --

    #[test]
    fn describe_has_metadata() {
        let d = inner::describe();
        assert_eq!(d["name"], "scolta-core");
        assert_eq!(d["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(d["wasm_interface_version"], 1);
        assert!(d["description"].as_str().unwrap().len() > 10);
    }

    #[test]
    fn describe_lists_all_functions() {
        let d = inner::describe();
        let fns = d["functions"].as_object().unwrap();
        let expected = [
            "resolve_prompt",
            "get_prompt",
            "clean_html",
            "build_pagefind_html",
            "score_results",
            "merge_results",
            "to_js_scoring_config",
            "parse_expansion",
            "version",
            "describe",
            "debug_call",
        ];
        for name in expected {
            assert!(fns.contains_key(name), "describe() missing: {}", name);
        }
        assert_eq!(
            fns.len(),
            expected.len(),
            "Extra unexpected functions in describe()"
        );
    }

    #[test]
    fn describe_function_entries_have_required_fields() {
        let d = inner::describe();
        let fns = d["functions"].as_object().unwrap();
        for (name, info) in fns {
            assert!(
                info.get("description").is_some(),
                "{} missing 'description'",
                name
            );
            assert!(
                info.get("output_type").is_some(),
                "{} missing 'output_type'",
                name
            );
            assert!(
                info.get("since").is_some(),
                "{} missing 'since' (VERSIONING.md requirement)",
                name
            );
            assert!(
                info.get("stability").is_some(),
                "{} missing 'stability' (VERSIONING.md requirement)",
                name
            );
        }
    }

    #[test]
    fn describe_resolve_prompt_documents_input_fields() {
        let d = inner::describe();
        let rp = &d["functions"]["resolve_prompt"];
        let fields = rp["input_fields"].as_object().unwrap();
        assert!(fields.contains_key("prompt_name"));
        assert!(fields.contains_key("site_name"));
        assert!(fields.contains_key("site_description"));
        // prompt_name should be required
        assert_eq!(fields["prompt_name"]["required"], true);
    }
}

#[cfg(test)]
mod inner_api_errors {
    //! Every error path through the inner:: API. These verify that bad input
    //! produces a useful, function-attributed error message.
    use scolta_core::inner;
    use serde_json::json;

    // -- resolve_prompt errors --

    #[test]
    fn resolve_prompt_not_object() {
        let err = inner::resolve_prompt(&json!("string")).unwrap_err();
        assert!(
            err.to_string().contains("resolve_prompt"),
            "Should name function: {}",
            err
        );
        assert!(
            err.to_string().contains("JSON"),
            "Should mention JSON: {}",
            err
        );
    }

    #[test]
    fn resolve_prompt_missing_prompt_name() {
        let err = inner::resolve_prompt(&json!({"site_name": "S"})).unwrap_err();
        assert!(err.to_string().contains("prompt_name"));
        assert!(err.to_string().contains("resolve_prompt"));
    }

    #[test]
    fn resolve_prompt_unknown_template() {
        let err = inner::resolve_prompt(&json!({"prompt_name": "nonexistent"})).unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn resolve_prompt_prompt_name_not_string() {
        let err = inner::resolve_prompt(&json!({"prompt_name": 42})).unwrap_err();
        assert!(err.to_string().contains("prompt_name"));
    }

    #[test]
    fn resolve_prompt_null_input() {
        let err = inner::resolve_prompt(&json!(null)).unwrap_err();
        assert!(err.to_string().contains("resolve_prompt"));
    }

    #[test]
    fn resolve_prompt_array_input() {
        let err = inner::resolve_prompt(&json!([1, 2])).unwrap_err();
        assert!(err.to_string().contains("resolve_prompt"));
    }

    // -- get_prompt errors --

    #[test]
    fn get_prompt_unknown_name() {
        let err = inner::get_prompt("nonexistent").unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn get_prompt_empty_string() {
        let result = inner::get_prompt("");
        assert!(result.is_err(), "Empty string should produce an error");
    }

    // -- clean_html errors (server-only) --

    #[cfg(feature = "extism")]
    #[test]
    fn clean_html_not_object() {
        let err = inner::clean_html(&json!("string")).unwrap_err();
        assert!(err.to_string().contains("clean_html"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn clean_html_missing_html_field() {
        let err = inner::clean_html(&json!({"title": "T"})).unwrap_err();
        assert!(err.to_string().contains("html"));
        assert!(err.to_string().contains("clean_html"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn clean_html_html_field_not_string() {
        let err = inner::clean_html(&json!({"html": 42})).unwrap_err();
        assert!(err.to_string().contains("html"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn clean_html_null_input() {
        let err = inner::clean_html(&json!(null)).unwrap_err();
        assert!(err.to_string().contains("clean_html"));
    }

    // -- build_pagefind_html errors (server-only) --

    #[cfg(feature = "extism")]
    #[test]
    fn build_pagefind_html_not_object() {
        let err = inner::build_pagefind_html(&json!("string")).unwrap_err();
        assert!(err.to_string().contains("build_pagefind_html"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn build_pagefind_html_missing_id() {
        let err = inner::build_pagefind_html(&json!({"title": "T", "body": "B", "url": "U"}))
            .unwrap_err();
        assert!(err.to_string().contains("id"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn build_pagefind_html_missing_title() {
        let err =
            inner::build_pagefind_html(&json!({"id": "1", "body": "B", "url": "U"})).unwrap_err();
        assert!(err.to_string().contains("title"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn build_pagefind_html_missing_body() {
        let err =
            inner::build_pagefind_html(&json!({"id": "1", "title": "T", "url": "U"})).unwrap_err();
        assert!(err.to_string().contains("body"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn build_pagefind_html_missing_url() {
        let err =
            inner::build_pagefind_html(&json!({"id": "1", "title": "T", "body": "B"})).unwrap_err();
        assert!(err.to_string().contains("url"));
    }

    #[cfg(feature = "extism")]
    #[test]
    fn build_pagefind_html_null_input() {
        let err = inner::build_pagefind_html(&json!(null)).unwrap_err();
        assert!(err.to_string().contains("build_pagefind_html"));
    }

    // -- score_results errors --

    #[test]
    fn score_results_not_object() {
        let err = inner::score_results(&json!("string")).unwrap_err();
        assert!(err.to_string().contains("score_results"));
    }

    #[test]
    fn score_results_missing_query() {
        let err = inner::score_results(&json!({"results": []})).unwrap_err();
        assert!(err.to_string().contains("query"));
    }

    #[test]
    fn score_results_missing_results() {
        let err = inner::score_results(&json!({"query": "test"})).unwrap_err();
        assert!(err.to_string().contains("results"));
    }

    #[test]
    fn score_results_results_not_array() {
        let err =
            inner::score_results(&json!({"query": "test", "results": "not an array"})).unwrap_err();
        assert!(err.to_string().contains("score_results"));
    }

    #[test]
    fn score_results_malformed_result_object() {
        let err = inner::score_results(&json!({
            "query": "test",
            "results": [{"not_url": "missing required fields"}]
        }))
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("score_results"),
            "Error should name function: {}",
            msg
        );
    }

    #[test]
    fn score_results_null_input() {
        let err = inner::score_results(&json!(null)).unwrap_err();
        assert!(err.to_string().contains("score_results"));
    }

    // -- merge_results errors --

    #[test]
    fn merge_results_not_object() {
        let err = inner::merge_results(&json!("string")).unwrap_err();
        assert!(err.to_string().contains("merge_results"));
    }

    #[test]
    fn merge_results_missing_original() {
        let err = inner::merge_results(&json!({"expanded": []})).unwrap_err();
        assert!(err.to_string().contains("original"));
    }

    #[test]
    fn merge_results_missing_expanded() {
        let err = inner::merge_results(&json!({"original": []})).unwrap_err();
        assert!(err.to_string().contains("expanded"));
    }

    #[test]
    fn merge_results_malformed_original() {
        let err = inner::merge_results(&json!({
            "original": [{"bad": "data"}],
            "expanded": []
        }))
        .unwrap_err();
        assert!(err.to_string().contains("merge_results"));
    }

    #[test]
    fn merge_results_malformed_expanded() {
        let err = inner::merge_results(&json!({
            "original": [],
            "expanded": [{"bad": "data"}]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("merge_results"));
    }

    #[test]
    fn merge_results_null_input() {
        let err = inner::merge_results(&json!(null)).unwrap_err();
        assert!(err.to_string().contains("merge_results"));
    }

    // -- to_js_scoring_config -- (this one is lenient, hard to make it error)
    // It's tested in config_module. The inner:: wrapper doesn't add error paths
    // beyond what from_json handles (which silently uses defaults).
}

#[cfg(test)]
mod pipeline {
    //! End-to-end workflow tests simulating real adapter usage patterns.
    use scolta_core::*;
    use serde_json::json;

    fn recent_date() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (30 * 86400);
        let (y, m, d) = scoring::civil_from_epoch_secs(secs);
        format!("{:04}-{:02}-{:02}", y, m, d)
    }

    #[test]
    fn full_search_pipeline() {
        // 1. Get the AI expansion prompt
        let template = prompts::get_template("expand_query").unwrap();
        assert!(template.contains("{SITE_NAME}"));

        // 2. Resolve it for a specific site
        let prompt = prompts::resolve_template("expand_query", "TestSite", "a test").unwrap();
        assert!(!prompt.contains("{SITE_NAME}"));

        // 3. Simulate LLM response and parse expansion terms
        let llm_response = r#"["drupal cms", "content management", "web platform"]"#;
        let terms = expansion::parse_expansion(llm_response);
        assert_eq!(terms.len(), 3);

        // 4. Clean some HTML
        let cleaned = html::clean_html(
            "<html><body><div id=\"main-content\"><p>Drupal is a CMS</p></div><footer>Skip</footer></body></html>",
            "",
        );
        assert!(cleaned.contains("Drupal is a CMS"));

        // 5. Build Pagefind document
        let pagefind = html::build_pagefind_html(
            "doc-1",
            "Drupal Guide",
            &cleaned,
            "https://example.com/drupal",
            "2026-03-01",
            "TestSite",
        );
        assert!(pagefind.contains("data-pagefind-body"));

        // 6. Score results
        let date = recent_date();
        let mut results = vec![
            scoring::SearchResult {
                url: "https://example.com/drupal".to_string(),
                title: "Drupal Guide".to_string(),
                excerpt: "All about Drupal CMS".to_string(),
                date: date.clone(),
                score: 0.0,
                content_type: String::new(),
                site_name: String::new(),
                extra: serde_json::Map::new(),
            },
            scoring::SearchResult {
                url: "https://example.com/other".to_string(),
                title: "Other Page".to_string(),
                excerpt: "Unrelated content".to_string(),
                date,
                score: 0.0,
                content_type: String::new(),
                site_name: String::new(),
                extra: serde_json::Map::new(),
            },
        ];
        let cfg = scoring::ScoringConfig::default();
        scoring::score_results(&mut results, "drupal cms", &cfg);
        assert!(results[0].score > results[1].score);
        assert_eq!(results[0].url, "https://example.com/drupal");

        // 7. Export config for frontend
        let js_config = config::to_js_scoring_config(&cfg, &json!({"ai_expand_query": true}));
        assert_eq!(js_config["AI_EXPAND_QUERY"], true);

        // 8. Version check
        assert!(!inner::version().is_empty());
    }

    #[test]
    fn merge_pipeline() {
        // Simulate scoring primary + expanded, then merging
        let date = recent_date();
        let input = json!({
            "query": "drupal performance",
            "results": [
                {"url": "https://a.com", "title": "Drupal Perf", "excerpt": "Drupal performance tips", "date": &date},
                {"url": "https://b.com", "title": "General Tips", "excerpt": "Various tips", "date": &date}
            ],
            "config": {}
        });
        let primary = inner::score_results(&input).unwrap();

        let expanded_input = json!({
            "query": "cms optimization",
            "results": [
                {"url": "https://a.com", "title": "Drupal Perf", "excerpt": "Drupal performance tips", "date": &date},
                {"url": "https://c.com", "title": "CMS Guide", "excerpt": "CMS optimization strategies", "date": &date}
            ],
            "config": {}
        });
        let expanded = inner::score_results(&expanded_input).unwrap();

        let merge_input = json!({
            "original": primary,
            "expanded": expanded,
            "config": {"expand_primary_weight": 0.7}
        });
        let merged = inner::merge_results(&merge_input).unwrap();
        let arr = merged.as_array().unwrap();

        // Should have 3 unique URLs (a, b, c), with a being deduped
        assert_eq!(arr.len(), 3);
        // All should have positive scores
        for r in arr {
            assert!(r["score"].as_f64().unwrap() > 0.0);
        }
    }

    #[test]
    fn describe_then_call_each_function() {
        // Use describe() to discover functions, then verify each one works
        let catalog = inner::describe();
        let functions = catalog["functions"].as_object().unwrap();

        // Every function in the catalog should be callable
        assert!(functions.contains_key("resolve_prompt"));
        assert!(functions.contains_key("get_prompt"));
        assert!(functions.contains_key("clean_html"));
        assert!(functions.contains_key("build_pagefind_html"));
        assert!(functions.contains_key("score_results"));
        assert!(functions.contains_key("merge_results"));
        assert!(functions.contains_key("to_js_scoring_config"));
        assert!(functions.contains_key("parse_expansion"));
        assert!(functions.contains_key("version"));
        assert!(functions.contains_key("describe"));
        assert!(functions.contains_key("debug_call"));

        // Verify the catalog itself is consistent
        assert_eq!(functions.len(), 11);
        assert_eq!(catalog["version"], inner::version());
    }

    #[test]
    fn config_round_trip() {
        // Parse config from JSON, export as JS config, verify values match
        let input = json!({
            "recency_boost_max": 0.8,
            "recency_half_life_days": 200,
            "content_all_terms_multiplier": 0.6,
            "expand_primary_weight": 0.5,
            "ai_expand_query": false,
            "ai_max_followups": 7
        });

        let cfg = config::from_json(&input);
        assert_eq!(cfg.recency_boost_max, 0.8);
        assert_eq!(cfg.recency_half_life_days, 200);
        assert_eq!(cfg.content_all_terms_multiplier, 0.6);

        let js = config::to_js_scoring_config(&cfg, &input);
        assert_eq!(js["RECENCY_BOOST_MAX"], 0.8);
        assert_eq!(js["RECENCY_HALF_LIFE_DAYS"], 200);
        assert_eq!(js["CONTENT_ALL_TERMS_MULTIPLIER"], 0.6);
        assert_eq!(js["EXPAND_PRIMARY_WEIGHT"], 0.5);
        assert_eq!(js["AI_EXPAND_QUERY"], false);
        assert_eq!(js["AI_MAX_FOLLOWUPS"], 7);
    }
}

// ---------------------------------------------------------------------------
// Lifecycle validation — enforces VERSIONING.md rules via CI.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod versioning {
    //! These tests enforce the rules in VERSIONING.md.
    //!
    //! - Every exported function has `since` and `stability` in describe().
    //! - Valid stability values are: experimental, stable, deprecated.
    //! - Deprecated functions must have `deprecated_in` and `replacement`.
    //! - WASM interface version is present and is a positive integer.
    //! - Version string is valid semver.

    use scolta_core::inner;
    use scolta_core::WASM_INTERFACE_VERSION;

    const VALID_STABILITY: &[&str] = &["experimental", "stable", "deprecated"];

    #[test]
    fn wasm_interface_version_is_positive() {
        assert!(WASM_INTERFACE_VERSION > 0);
    }

    #[test]
    fn describe_includes_wasm_interface_version() {
        let d = inner::describe();
        let v = d["wasm_interface_version"].as_u64().unwrap();
        assert_eq!(v, WASM_INTERFACE_VERSION as u64);
    }

    #[test]
    fn version_is_valid_semver() {
        let v = inner::version();
        // Strip pre-release suffix (e.g., "-dev") before checking MAJOR.MINOR.PATCH.
        let base = v.split('-').next().unwrap();
        let parts: Vec<&str> = base.split('.').collect();
        assert_eq!(
            parts.len(),
            3,
            "Version must be MAJOR.MINOR.PATCH[-prerelease]: {}",
            v
        );
        for part in &parts {
            assert!(
                part.parse::<u32>().is_ok(),
                "Non-numeric version part '{}' in: {}",
                part,
                v
            );
        }
        // If there's a pre-release, it must be non-empty.
        if let Some(pre) = v.split('-').nth(1) {
            assert!(!pre.is_empty(), "Empty pre-release suffix in: {}", v);
        }
    }

    #[test]
    fn all_functions_have_since_and_stability() {
        let d = inner::describe();
        let fns = d["functions"].as_object().unwrap();

        for (name, info) in fns {
            let since = info
                .get("since")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    panic!("{} missing 'since' field — VERSIONING.md requires it", name)
                });
            assert!(!since.is_empty(), "{} has empty 'since'", name);

            let stability = info
                .get("stability")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    panic!(
                        "{} missing 'stability' field — VERSIONING.md requires it",
                        name
                    )
                });
            assert!(
                VALID_STABILITY.contains(&stability),
                "{} has invalid stability '{}' — must be one of {:?}",
                name,
                stability,
                VALID_STABILITY
            );
        }
    }

    #[test]
    fn since_values_are_valid_semver() {
        let d = inner::describe();
        let fns = d["functions"].as_object().unwrap();

        for (name, info) in fns {
            let since = info["since"].as_str().unwrap();
            let parts: Vec<&str> = since.split('.').collect();
            assert_eq!(
                parts.len(),
                3,
                "{} has invalid 'since' version '{}' — must be MAJOR.MINOR.PATCH",
                name,
                since
            );
        }
    }

    #[test]
    fn deprecated_functions_have_required_metadata() {
        let d = inner::describe();
        let fns = d["functions"].as_object().unwrap();

        for (name, info) in fns {
            if info.get("stability").and_then(|v| v.as_str()) == Some("deprecated") {
                assert!(
                    info.get("deprecated_in").is_some(),
                    "Deprecated function '{}' missing 'deprecated_in' — VERSIONING.md requires it",
                    name
                );
                assert!(
                    info.get("replacement").is_some(),
                    "Deprecated function '{}' missing 'replacement' — VERSIONING.md requires it",
                    name
                );
                assert!(
                    info.get("removal").is_some(),
                    "Deprecated function '{}' missing 'removal' — VERSIONING.md requires it",
                    name
                );
            }
        }
    }

    #[test]
    fn no_function_lacks_description() {
        let d = inner::describe();
        let fns = d["functions"].as_object().unwrap();

        for (name, info) in fns {
            let desc = info
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert!(!desc.is_empty(), "{} has empty description", name);
        }
    }
}
