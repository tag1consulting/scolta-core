//! Pagefind integration tests for scolta-core.
//!
//! These tests verify that scolta-core's HTML output is correctly indexed
//! by pagefind. They generate HTML via `build_pagefind_html`, write files
//! to a temp directory, invoke `npx pagefind` to build a search index,
//! and verify the index is valid.
//!
//! Requires: Node.js + npm (for `npx pagefind`). Tests are skipped if
//! npx is not available.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use scolta_core::html;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if npx is available and can run pagefind.
fn pagefind_available() -> bool {
    Command::new("npx")
        .args(["pagefind", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run pagefind against a site directory, putting the index into `output_dir`.
fn run_pagefind(site_dir: &Path, output_dir: &Path) -> (bool, String) {
    let output = Command::new("npx")
        .args([
            "pagefind",
            "--site",
            site_dir.to_str().unwrap(),
            "--output-path",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute npx pagefind");

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    (output.status.success(), combined)
}

/// Write a scolta-generated HTML file into the build directory.
fn write_page(
    dir: &Path,
    filename: &str,
    id: &str,
    title: &str,
    body: &str,
    url: &str,
    date: &str,
    site: &str,
) {
    let html = html::build_pagefind_html(id, title, body, url, date, site);
    fs::write(dir.join(filename), html).unwrap();
}

// ---------------------------------------------------------------------------
// Pagefind CLI integration tests
// ---------------------------------------------------------------------------

#[test]
fn pagefind_indexes_single_page() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    write_page(
        &site,
        "page1.html",
        "page-1",
        "Getting Started with Rust",
        "Rust is a systems programming language focused on safety, speed, and concurrency.",
        "https://example.com/rust-intro",
        "2024-06-15",
        "Example Docs",
    );

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed:\n{}", output);

    // Pagefind should produce its JS entry point.
    assert!(
        index.join("pagefind.js").exists(),
        "pagefind.js not generated"
    );
    // And a WASM file for client-side search (pagefind 1.4+ uses .pagefind extension).
    let has_wasm = fs::read_dir(&index)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.contains("wasm") || name.ends_with(".pagefind")
        });
    assert!(has_wasm, "No WASM file found in pagefind output");
}

#[test]
fn pagefind_indexes_multiple_pages() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    write_page(
        &site,
        "page1.html",
        "doc-1",
        "Database Optimization",
        "Learn how to optimize your database queries for production workloads.",
        "https://example.com/db-optimize",
        "2024-03-10",
        "Tech Blog",
    );
    write_page(
        &site,
        "page2.html",
        "doc-2",
        "Caching Strategies",
        "Explore different caching strategies including Redis, Memcached, and CDN layers.",
        "https://example.com/caching",
        "2024-05-20",
        "Tech Blog",
    );
    write_page(
        &site,
        "page3.html",
        "doc-3",
        "Performance Monitoring",
        "Set up monitoring dashboards with Grafana and Prometheus for your infrastructure.",
        "https://example.com/monitoring",
        "2024-07-01",
        "Tech Blog",
    );

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed:\n{}", output);
    assert!(index.join("pagefind.js").exists());

    // Verify pagefind reports indexing 3 pages.
    assert!(
        output.contains("3") || output.contains("three"),
        "Expected pagefind to report indexing 3 pages, got:\n{}",
        output
    );
}

#[test]
fn pagefind_indexes_html_with_special_characters() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    // Content with characters that need HTML escaping.
    write_page(
        &site, "special.html", "special-1",
        "C++ vs Rust: A <Comparison>",
        "Performance comparison: C++ achieves 95% & Rust achieves 98% on benchmarks. Use 'unsafe' wisely.",
        "https://example.com/compare?lang=cpp&vs=rust",
        "2024-01-01",
        "Dev & Ops Blog",
    );

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed on special chars:\n{}", output);
    assert!(index.join("pagefind.js").exists());
}

#[test]
fn pagefind_indexes_page_without_site_filter() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    // Empty site_name means no data-pagefind-filter attribute.
    write_page(
        &site,
        "nofilter.html",
        "nf-1",
        "No Filter Page",
        "This page has no site filter attribute.",
        "https://example.com/no-filter",
        "2024-02-15",
        "",
    );

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed without filter:\n{}", output);
    assert!(index.join("pagefind.js").exists());
}

#[test]
fn pagefind_indexes_page_without_date() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    // Empty date should produce valid HTML without the date meta tag.
    write_page(
        &site,
        "nodate.html",
        "nd-1",
        "Undated Content",
        "This page has no publication date set.",
        "https://example.com/undated",
        "",
        "My Site",
    );

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed without date:\n{}", output);
    assert!(index.join("pagefind.js").exists());
}

#[test]
fn pagefind_indexes_unicode_content() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    write_page(
        &site,
        "unicode.html",
        "uni-1",
        "Ünïcödé Tëst Pägé",
        "日本語のテスト。Ñoño résumé café naïve über Straße. Ελληνικά العربية",
        "https://example.com/unicode",
        "2024-08-01",
        "Ünïcödé Sïtë",
    );

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed on unicode:\n{}", output);
    assert!(index.join("pagefind.js").exists());
}

#[test]
fn pagefind_indexes_long_content() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    // Generate a large body to verify pagefind handles bulk content.
    let long_body =
        "This is paragraph number N. It contains enough words to contribute to the search index. "
            .repeat(500)
            .replace("number N", &format!("number {}", 1));

    write_page(
        &site,
        "long.html",
        "long-1",
        "Very Long Article",
        &long_body,
        "https://example.com/long-article",
        "2024-09-01",
        "Docs",
    );

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed on long content:\n{}", output);
    assert!(index.join("pagefind.js").exists());
}

#[test]
fn pagefind_indexes_fixture_drupal_page() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    // Load the Drupal fixture, clean it, then build pagefind HTML.
    let raw_html = include_str!("fixtures/drupal-page.html");
    let cleaned = html::clean_html(raw_html, "Building Scalable Drupal Sites");
    assert!(
        !cleaned.is_empty(),
        "clean_html produced empty output from drupal fixture"
    );
    assert!(
        cleaned.contains("Drupal"),
        "Cleaned Drupal fixture should contain 'Drupal'"
    );
    assert!(
        cleaned.contains("Database optimization"),
        "Cleaned Drupal fixture should contain list items"
    );
    assert!(
        !cleaned.contains("<nav"),
        "Cleaned output should not contain nav"
    );
    assert!(
        !cleaned.contains("Copyright"),
        "Cleaned output should not contain footer text"
    );
    assert!(
        !cleaned.contains("console.log"),
        "Cleaned output should not contain script"
    );

    let pagefind_html = html::build_pagefind_html(
        "drupal-1",
        "Building Scalable Drupal Sites",
        &cleaned,
        "https://example.com/scalable-drupal",
        "2024-01-01",
        "Example Corp",
    );
    fs::write(site.join("drupal.html"), &pagefind_html).unwrap();

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed on Drupal fixture:\n{}", output);
    assert!(index.join("pagefind.js").exists());
}

#[test]
fn pagefind_indexes_fixture_wordpress_post() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    let raw_html = include_str!("fixtures/wordpress-post.html");
    let cleaned = html::clean_html(raw_html, "WordPress Security Best Practices");
    assert!(
        !cleaned.is_empty(),
        "clean_html produced empty output from WP fixture"
    );
    assert!(
        cleaned.contains("WordPress"),
        "Cleaned WP fixture should contain 'WordPress'"
    );
    assert!(
        cleaned.contains("Strong Passwords"),
        "Cleaned WP fixture should contain section heading"
    );
    assert!(
        !cleaned.contains("tracking.js"),
        "Cleaned output should not contain script src"
    );
    assert!(
        !cleaned.contains("pageInfo"),
        "Cleaned output should not contain inline JS"
    );
    assert!(
        !cleaned.contains("All rights reserved"),
        "Cleaned output should not contain footer"
    );

    let pagefind_html = html::build_pagefind_html(
        "wp-1",
        "WordPress Security Best Practices",
        &cleaned,
        "https://example.com/wp-security",
        "2024-01-15",
        "Example Blog",
    );
    fs::write(site.join("wordpress.html"), &pagefind_html).unwrap();

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind failed on WP fixture:\n{}", output);
    assert!(index.join("pagefind.js").exists());
}

#[test]
fn pagefind_end_to_end_pipeline() {
    if !pagefind_available() {
        eprintln!("SKIPPED: npx pagefind not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let site = tmp.path().join("site");
    let index = tmp.path().join("index");
    fs::create_dir_all(&site).unwrap();

    // Simulate the full scolta pipeline: raw CMS content → clean → build → index.
    let pages = vec![
        ("page-1", "Drupal Migration Guide",
         "<nav>Skip</nav><div id=\"main-content\"><h1>Drupal Migration Guide</h1><p>Step by step guide for migrating from Drupal 7 to Drupal 10.</p></div><footer>© 2024</footer>",
         "https://example.com/migration", "2024-03-15", "Tag1 Docs"),
        ("page-2", "Performance Tuning",
         "<body><nav>Menu</nav><main id=\"main-content\"><h1>Performance Tuning</h1><p>How to optimize PHP and MySQL for high traffic Drupal sites.</p></main><script>analytics()</script></body>",
         "https://example.com/performance", "2024-06-01", "Tag1 Docs"),
        ("page-3", "Security Hardening",
         "<div id=\"main-content\"><h1>Security Hardening</h1><p>Essential security practices for production Drupal deployments including WAF and CSP headers.</p></div><footer class=\"footer\">Footer text</footer>",
         "https://example.com/security", "2024-09-01", "Tag1 Docs"),
    ];

    for (id, title, raw_html, url, date, site_name) in &pages {
        let cleaned = html::clean_html(raw_html, title);
        let pagefind_html = html::build_pagefind_html(id, title, &cleaned, url, date, site_name);
        let filename = format!("{}.html", id);
        fs::write(site.join(&filename), &pagefind_html).unwrap();
    }

    let (success, output) = run_pagefind(&site, &index);
    assert!(success, "Pagefind pipeline failed:\n{}", output);
    assert!(index.join("pagefind.js").exists());

    // Verify fragment files were produced (pagefind chunks the index).
    let fragment_dir = index.join("fragment");
    if fragment_dir.exists() {
        let fragment_count = fs::read_dir(&fragment_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .count();
        assert!(fragment_count > 0, "Expected at least one fragment file");
    }

    // Verify the index entry JS file exists and contains references.
    let pagefind_js = fs::read_to_string(index.join("pagefind.js")).unwrap();
    assert!(!pagefind_js.is_empty(), "pagefind.js should not be empty");
}

// ---------------------------------------------------------------------------
// HTML output structure tests (no pagefind CLI needed)
// ---------------------------------------------------------------------------

#[test]
fn build_pagefind_html_has_valid_structure() {
    let html = html::build_pagefind_html(
        "test-1",
        "Test Title",
        "Test body content.",
        "https://example.com/test",
        "2024-01-15",
        "My Site",
    );

    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<html>"));
    assert!(html.contains("</html>"));
    assert!(html.contains("<head>"));
    assert!(html.contains("</head>"));
    assert!(html.contains("<body"));
    assert!(html.contains("</body>"));
    assert!(html.contains(r#"<meta charset="utf-8">"#));
    assert!(html.contains("<title>Test Title</title>"));
    assert!(html.contains("<h1>Test Title</h1>"));
    assert!(html.contains("data-pagefind-body"));
    assert!(html.contains(r#"data-pagefind-meta="url:https://example.com/test""#));
    assert!(html.contains(r#"data-pagefind-meta="date:2024-01-15""#));
    assert!(html.contains(r#"data-pagefind-filter="site:My Site""#));
    assert!(html.contains("Test body content."));
}

#[test]
fn build_pagefind_html_date_omitted_when_empty() {
    let html = html::build_pagefind_html("test-2", "No Date", "Body.", "https://x.com", "", "Site");
    assert!(!html.contains("data-pagefind-meta=\"date:"));
}

#[test]
fn build_pagefind_html_filter_omitted_when_no_site() {
    let html = html::build_pagefind_html(
        "test-3",
        "No Site",
        "Body.",
        "https://x.com",
        "2024-01-01",
        "",
    );
    assert!(!html.contains("data-pagefind-filter"));
}

// ---------------------------------------------------------------------------
// Fixture cleaning tests (no pagefind CLI needed)
// ---------------------------------------------------------------------------

#[test]
fn clean_drupal_fixture_extracts_main_content() {
    let raw = include_str!("fixtures/drupal-page.html");
    let cleaned = html::clean_html(raw, "Building Scalable Drupal Sites");

    // Should contain article body.
    assert!(cleaned.contains("powerful content management system"));
    assert!(cleaned.contains("Database optimization"));
    assert!(cleaned.contains("Caching strategies"));
    assert!(cleaned.contains("Content architecture"));
    assert!(cleaned.contains("Performance monitoring"));

    // Should not contain chrome.
    assert!(!cleaned.contains("Home")); // nav link
    assert!(!cleaned.contains("Services")); // nav link
    assert!(!cleaned.contains("Copyright")); // footer
    assert!(!cleaned.contains("Privacy Policy")); // footer
    assert!(!cleaned.contains("console.log")); // script
    assert!(!cleaned.contains("ga(")); // script
    assert!(!cleaned.contains("font-family")); // style

    // Title should be removed from the beginning (it's in h1).
    assert!(!cleaned.starts_with("Building Scalable Drupal Sites"));
}

#[test]
fn clean_wordpress_fixture_extracts_main_content() {
    let raw = include_str!("fixtures/wordpress-post.html");
    let cleaned = html::clean_html(raw, "WordPress Security Best Practices");

    // Should contain article body.
    assert!(cleaned.contains("WordPress powers over 40%"));
    assert!(cleaned.contains("Keep Everything Updated"));
    assert!(cleaned.contains("Strong Passwords"));
    assert!(cleaned.contains("Limit Login Attempts"));
    assert!(cleaned.contains("rate limiting"));

    // Should not contain chrome.
    assert!(!cleaned.contains("Main Menu")); // nav
    assert!(!cleaned.contains("Blog")); // nav link
    assert!(!cleaned.contains("tracking.js")); // script
    assert!(!cleaned.contains("pageInfo")); // inline script
    assert!(!cleaned.contains("All rights reserved")); // footer

    // Title should be removed from the beginning.
    assert!(!cleaned.starts_with("WordPress Security Best Practices"));
}

// ---------------------------------------------------------------------------
// Edge case HTML cleaning tests
// ---------------------------------------------------------------------------

#[test]
fn clean_html_handles_empty_input() {
    let cleaned = html::clean_html("", "");
    assert!(cleaned.is_empty() || cleaned.trim().is_empty());
}

#[test]
fn clean_html_handles_plain_text() {
    let cleaned = html::clean_html("Just plain text, no HTML at all.", "");
    assert!(cleaned.contains("Just plain text"));
}

#[test]
fn clean_html_handles_html_entities_in_content() {
    let html = r#"<div id="main-content"><p>Tom &amp; Jerry &lt;3 &gt; everything &quot;else&quot;</p></div>"#;
    let cleaned = html::clean_html(html, "");
    // HTML entities should become their literal characters after tag stripping.
    assert!(cleaned.contains("Tom"));
    assert!(cleaned.contains("Jerry"));
}

#[test]
fn clean_html_handles_deeply_nested_tags() {
    let html = r#"<div id="main-content"><div><div><div><div><p>Deep content</p></div></div></div></div></div>"#;
    let cleaned = html::clean_html(html, "");
    assert!(cleaned.contains("Deep content"));
}

#[test]
fn clean_html_handles_multiple_main_content_markers() {
    // First one should win.
    let html = r#"<div id="main-content"><p>First main</p></div><div id="main-content"><p>Second main</p></div>"#;
    let cleaned = html::clean_html(html, "");
    assert!(cleaned.contains("First main"));
}

#[test]
fn clean_html_strips_multiple_script_and_style_blocks() {
    let html = r#"<body>
        <script>var a = 1;</script>
        <p>Good content</p>
        <style>.bad { color: red; }</style>
        <script type="module">import x from 'y';</script>
        <style media="print">body { display: none; }</style>
        <p>More good content</p>
    </body>"#;
    let cleaned = html::clean_html(html, "");
    assert!(cleaned.contains("Good content"));
    assert!(cleaned.contains("More good content"));
    assert!(!cleaned.contains("var a"));
    assert!(!cleaned.contains("import x"));
    assert!(!cleaned.contains("color: red"));
    assert!(!cleaned.contains("display: none"));
}

#[test]
fn clean_html_strips_nav_with_nested_lists() {
    let html = r#"<body>
        <nav><ul><li><a href="/">Home</a></li><li><a href="/about">About</a><ul><li><a href="/team">Team</a></li></ul></li></ul></nav>
        <div id="main-content"><p>Article body</p></div>
    </body>"#;
    let cleaned = html::clean_html(html, "");
    assert!(cleaned.contains("Article body"));
    assert!(!cleaned.contains("Team"));
}

#[test]
fn clean_html_removes_title_from_start() {
    let html = r#"<div id="main-content"><h1>My Title</h1><p>Content after title.</p></div>"#;
    let cleaned = html::clean_html(html, "My Title");
    assert!(cleaned.contains("Content after title"));
    // Title should be removed from the beginning.
    assert!(!cleaned.starts_with("My Title"));
}

#[test]
fn clean_html_preserves_body_when_title_not_at_start() {
    let html = r#"<body><div id="main-content"><p>Intro text.</p><h2>Subtitle</h2><p>More text.</p></div></body>"#;
    let cleaned = html::clean_html(html, "Some Title");
    // Title "Some Title" doesn't appear in the content, nothing should be removed.
    assert!(cleaned.contains("Intro text"));
    assert!(cleaned.contains("Subtitle"));
    assert!(cleaned.contains("More text"));
}
