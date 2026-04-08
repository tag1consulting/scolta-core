//! HTML processing utilities for cleaning content and generating Pagefind-compatible HTML.
//!
//! Two main functions:
//! - [`clean_html`] strips page chrome (nav, footer, scripts, styles) and extracts
//!   the main content region as plain text suitable for search indexing.
//! - [`build_pagefind_html`] generates minimal HTML with `data-pagefind-*` attributes
//!   for Pagefind static search indexing.
//!
//! # Performance
//!
//! All regex patterns are compiled once via [`OnceLock`] and reused across calls.
//! This matters in WASM where regex compilation is 5-10x slower than native due
//! to the absence of SIMD optimizations. A Pagefind indexing run over thousands
//! of pages calls `clean_html` per page.
//!
//! # HTML parsing approach
//!
//! The HTML cleaner uses regex-based tag matching rather than a full DOM parser.
//! This keeps the WASM binary small (~500KB vs ~700KB+ with a parser like lol_html)
//! and handles the specific CMS output patterns from Drupal, WordPress, and Laravel.
//! For arbitrary untrusted HTML, a proper parser would be more robust — but for
//! CMS page output fed into a search index, regex is sufficient and faster.

use regex::Regex;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Compiled regex patterns (compiled once, reused across all calls)
// ---------------------------------------------------------------------------

/// Match `<script>...</script>` including multiline content.
fn re_script() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>").expect("static regex pattern is valid"))
}

/// Match `<style>...</style>` including multiline content.
fn re_style() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style\s*>").expect("static regex pattern is valid"))
}

/// Match `<nav>...</nav>` including multiline content.
fn re_nav() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<nav\b[^>]*>.*?</nav\s*>").expect("static regex pattern is valid"))
}

/// Match `<footer>...</footer>` including multiline content.
fn re_footer() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<footer\b[^>]*>.*?</footer\s*>").expect("static regex pattern is valid"))
}

/// Match elements with footer-related IDs (e.g., `id="footer"`, `id="site-footer"`).
fn re_footer_id() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<[^>]*\sid\s*=\s*["'][^"']*footer[^"']*["'][^>]*>.*?</[^>]*>"#)
            .expect("static regex pattern is valid")
    })
}

/// Match elements with footer-related classes (e.g., `class="footer"`, `class="site-footer"`).
fn re_footer_class() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<[^>]*\sclass\s*=\s*["'][^"']*footer[^"']*["'][^>]*>.*?</[^>]*>"#)
            .expect("static regex pattern is valid")
    })
}

/// Match elements with region-footer class.
fn re_region_footer() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?is)<[^>]*\sclass\s*=\s*["'][^"']*region-footer[^"']*["'][^>]*>.*?</[^>]*>"#,
        )
        .expect("static regex pattern is valid")
    })
}

/// Match any HTML tag.
fn re_tags() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("static regex pattern is valid"))
}

/// Match whitespace runs (for normalization).
fn re_whitespace() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").expect("static regex pattern is valid"))
}

/// Match HTML comments.
fn re_comments() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)<!--.*?-->").expect("static regex pattern is valid"))
}

/// Match an element with `id="main-content"` (case-insensitive, handles
/// single/double quotes and spaces around `=`).
fn re_main_content_open() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)<(div|main|article|section)\b[^>]*\sid\s*=\s*["']main-content["'][^>]*>"#)
            .expect("static regex pattern is valid")
    })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Remove page chrome and extract main content from HTML.
///
/// Processing pipeline:
/// 1. Remove HTML comments
/// 2. Extract the main content region (looks for `id="main-content"`, falls back to `<body>`)
/// 3. Remove footer regions (footer tags, footer IDs/classes)
/// 4. Remove script, style, and nav elements
/// 5. Strip all HTML tags
/// 6. Normalize whitespace
/// 7. Remove duplicate title at the beginning
///
/// # Arguments
/// * `html` - The raw HTML document
/// * `title` - The page title (used to remove duplicate at start of cleaned text)
///
/// # Returns
/// Cleaned plain text suitable for search indexing
pub fn clean_html(html: &str, title: &str) -> String {
    // Remove HTML comments first (they can contain tag-like strings)
    let no_comments = re_comments().replace_all(html, "");

    // Extract main content region
    let content = extract_main_content(&no_comments);

    // Remove footer regions
    let without_footer = remove_footer(&content);

    // Remove script, style, nav elements
    let without_chrome = remove_chrome_elements(&without_footer);

    // Strip HTML tags
    let text = strip_tags(&without_chrome);

    // Normalize whitespace
    let normalized = normalize_whitespace(&text);

    // Remove title from beginning if present
    remove_leading_title(&normalized, title)
}

/// Build a Pagefind-compatible HTML document.
///
/// Generates minimal HTML with `data-pagefind-*` attributes suitable for
/// Pagefind static search indexing. All field values are HTML-escaped.
///
/// # Arguments
/// * `id` - Unique document ID
/// * `title` - Page title
/// * `body` - Cleaned/processed text content
/// * `url` - Full page URL
/// * `date` - Publication date (ISO 8601 recommended, empty string if unknown)
/// * `site_name` - Site name for filtering (empty string to omit filter)
///
/// # Returns
/// Complete HTML document string
pub fn build_pagefind_html(
    id: &str,
    title: &str,
    body: &str,
    url: &str,
    date: &str,
    site_name: &str,
) -> String {
    let escaped_title = escape_html(title);
    let escaped_body = escape_html(body);
    let escaped_url = escape_html(url);
    let escaped_date = escape_html(date);
    let escaped_site = escape_html(site_name);

    let site_filter = if !site_name.is_empty() {
        format!(r#" data-pagefind-filter="site:{}""#, escaped_site)
    } else {
        String::new()
    };

    let date_meta = if !date.is_empty() {
        format!(
            r#"<p data-pagefind-meta="date:{}" hidden></p>
"#,
            escaped_date
        )
    } else {
        String::new()
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{}</title>
</head>
<body data-pagefind-body id="{}"{}>
<h1>{}</h1>
<p data-pagefind-meta="url:{}" hidden></p>
{}{}
</body>
</html>"#,
        escaped_title, id, site_filter, escaped_title, escaped_url, date_meta, escaped_body
    )
}

// ---------------------------------------------------------------------------
// Internal functions
// ---------------------------------------------------------------------------

/// Extract the main content region from HTML.
///
/// Looks for an element with `id="main-content"` (case-insensitive, handles
/// both quote styles and spaces around `=`). Falls back to `<body>` content.
/// Falls back to the full input if neither is found.
fn extract_main_content(html: &str) -> String {
    // Try to find main-content region using regex (handles case, quotes, spacing)
    if let Some(m) = re_main_content_open().find(html) {
        let tag_end = m.end();
        // Determine the tag name to find its closing tag
        let cap_text = m.as_str();
        let tag_name = extract_tag_name(cap_text).unwrap_or("div");

        if let Some(close_pos) = find_matching_close(html, tag_end, tag_name) {
            return html[tag_end..close_pos].to_string();
        }
    }

    // Fall back to <body> content
    if let Some(body_match) = find_body_content(html) {
        return body_match;
    }

    html.to_string()
}

/// Extract the tag name from an opening tag string like `<div ...>`.
fn extract_tag_name(tag: &str) -> Option<&str> {
    let after_lt = tag.strip_prefix('<')?;
    let end = after_lt.find(|c: char| c.is_whitespace() || c == '>' || c == '/')?;
    Some(&after_lt[..end])
}

/// Find the matching closing tag, handling nesting of the same tag type.
///
/// Starts searching from `start_pos` (which should be right after the opening
/// tag's `>`). Returns the position of the `<` in the matching `</tag>`.
fn find_matching_close(html: &str, start_pos: usize, tag_name: &str) -> Option<usize> {
    let search = &html[start_pos..];
    let open_pattern = format!("<{}", tag_name);
    let close_pattern = format!("</{}", tag_name);
    let mut depth: i32 = 1;
    let mut pos = 0;

    while pos < search.len() {
        let remaining = &search[pos..];

        // Find next opening or closing tag of this type
        let next_open = remaining
            .find(&open_pattern)
            .filter(|&p| {
                // Ensure it's actually a tag start, not middle of text
                let after = remaining.as_bytes().get(p + open_pattern.len());
                matches!(after, Some(b' ') | Some(b'>') | Some(b'/') | Some(b'\t') | Some(b'\n'))
            });
        let next_close = remaining.find(&close_pattern);

        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                pos += o + open_pattern.len();
            }
            (_, Some(c)) => {
                depth -= 1;
                if depth == 0 {
                    return Some(start_pos + pos + c);
                }
                pos += c + close_pattern.len();
            }
            (Some(o), None) => {
                depth += 1;
                pos += o + open_pattern.len();
            }
            (None, None) => break,
        }
    }

    None
}

/// Extract content between `<body ...>` and `</body>`.
fn find_body_content(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let body_start = lower.find("<body")?;
    let body_tag_end = html[body_start..].find('>')? + body_start + 1;
    let body_close = lower[body_tag_end..].find("</body>")?;
    Some(html[body_tag_end..body_tag_end + body_close].to_string())
}

/// Remove footer regions from HTML.
fn remove_footer(html: &str) -> String {
    let mut result = re_footer().replace_all(html, "").to_string();
    result = re_footer_id().replace_all(&result, "").to_string();
    result = re_footer_class().replace_all(&result, "").to_string();
    result = re_region_footer().replace_all(&result, "").to_string();
    result
}

/// Remove script, style, and nav elements.
fn remove_chrome_elements(html: &str) -> String {
    let result = re_script().replace_all(html, "");
    let result = re_style().replace_all(&result, "");
    let result = re_nav().replace_all(&result, "");
    result.to_string()
}

/// Remove all HTML tags from text.
fn strip_tags(html: &str) -> String {
    re_tags().replace_all(html, "").to_string()
}

/// Normalize whitespace: collapse multiple spaces/newlines to single space.
fn normalize_whitespace(text: &str) -> String {
    re_whitespace().replace_all(text, " ").trim().to_string()
}

/// Remove leading occurrence of title from text.
fn remove_leading_title(text: &str, title: &str) -> String {
    if title.is_empty() {
        return text.to_string();
    }

    if let Some(pos) = text.find(title) {
        if pos < 50 {
            let remaining = &text[pos + title.len()..];
            return remaining.trim_start().to_string();
        }
    }

    text.to_string()
}

/// Escape HTML special characters.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;");
        assert_eq!(escape_html("normal text"), "normal text");
    }

    #[test]
    fn test_strip_tags() {
        assert_eq!(strip_tags("<p>Hello <b>world</b></p>"), "Hello world");
        assert_eq!(strip_tags("<div>test</div>"), "test");
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(
            normalize_whitespace("hello    world\n\ntest"),
            "hello world test"
        );
        assert_eq!(normalize_whitespace("  spaced  "), "spaced");
    }

    #[test]
    fn test_clean_html_basic() {
        let html = "<html><body><p>Hello World</p></body></html>";
        let cleaned = clean_html(html, "");
        assert!(cleaned.contains("Hello World"));
    }

    #[test]
    fn test_clean_html_removes_script() {
        let html = "<body><p>Content</p><script>alert('test')</script></body>";
        let cleaned = clean_html(html, "");
        assert!(cleaned.contains("Content"));
        assert!(!cleaned.contains("alert"));
    }

    #[test]
    fn test_clean_html_removes_multiline_script() {
        let html = "<body><p>Content</p><script>\nvar x = 1;\nvar y = 2;\n</script></body>";
        let cleaned = clean_html(html, "");
        assert!(cleaned.contains("Content"));
        assert!(!cleaned.contains("var x"));
        assert!(!cleaned.contains("var y"));
    }

    #[test]
    fn test_clean_html_removes_multiline_style() {
        let html = "<body><p>Content</p><style>\n.test {\n  color: red;\n}\n</style></body>";
        let cleaned = clean_html(html, "");
        assert!(cleaned.contains("Content"));
        assert!(!cleaned.contains("color"));
    }

    #[test]
    fn test_clean_html_removes_html_comments() {
        let html = "<body><!-- <script>evil()</script> --><p>Content</p></body>";
        let cleaned = clean_html(html, "");
        assert!(cleaned.contains("Content"));
        assert!(!cleaned.contains("evil"));
        assert!(!cleaned.contains("<!--"));
    }

    #[test]
    fn test_extract_main_content_standard() {
        let html = r#"<html><body><nav>Skip</nav><div id="main-content"><p>Main</p></div><footer>Skip</footer></body></html>"#;
        let content = extract_main_content(html);
        assert!(content.contains("Main"));
        assert!(!content.contains("Skip"));
    }

    #[test]
    fn test_extract_main_content_case_insensitive() {
        let html = r#"<html><body><DIV ID="main-content"><p>Main</p></DIV></body></html>"#;
        let content = extract_main_content(html);
        assert!(content.contains("Main"));
    }

    #[test]
    fn test_extract_main_content_single_quotes() {
        let html = r#"<html><body><div id='main-content'><p>Main</p></div></body></html>"#;
        let content = extract_main_content(html);
        assert!(content.contains("Main"));
    }

    #[test]
    fn test_extract_main_content_with_spaces() {
        let html = r#"<html><body><div id = "main-content"><p>Main</p></div></body></html>"#;
        let content = extract_main_content(html);
        assert!(content.contains("Main"));
    }

    #[test]
    fn test_extract_main_content_as_main_tag() {
        let html = r#"<html><body><main id="main-content"><p>Main</p></main></body></html>"#;
        let content = extract_main_content(html);
        assert!(content.contains("Main"));
    }

    #[test]
    fn test_extract_main_content_nested_divs() {
        let html = r#"<div id="main-content"><div class="inner"><p>Deep content</p></div></div>"#;
        let content = extract_main_content(html);
        assert!(content.contains("Deep content"));
    }

    #[test]
    fn test_extract_main_content_fallback_to_body() {
        let html = "<html><body><p>Body content</p></body></html>";
        let content = extract_main_content(html);
        assert!(content.contains("Body content"));
    }

    #[test]
    fn test_build_pagefind_html() {
        let html = build_pagefind_html(
            "doc-123",
            "Test Title",
            "Test content",
            "https://example.com/page",
            "2024-01-01",
            "Test Site",
        );

        assert!(html.contains("doc-123"));
        assert!(html.contains("Test Title"));
        assert!(html.contains("Test content"));
        assert!(html.contains("https://example.com/page"));
        assert!(html.contains("2024-01-01"));
        assert!(html.contains("data-pagefind-filter=\"site:Test Site\""));
        assert!(html.contains("data-pagefind-body"));
    }

    #[test]
    fn test_build_pagefind_html_escapes_content() {
        let html = build_pagefind_html(
            "doc-1",
            "<Title>",
            "Content & more",
            "https://example.com?a=1&b=2",
            "2024-01-01",
            "",
        );

        assert!(html.contains("&lt;Title&gt;"));
        assert!(html.contains("Content &amp; more"));
        assert!(html.contains("a=1&amp;b=2"));
    }

    #[test]
    fn test_build_pagefind_html_omits_empty_site() {
        let html = build_pagefind_html("doc-1", "Title", "Body", "https://x.com", "2024-01-01", "");
        assert!(!html.contains("data-pagefind-filter"));
    }

    #[test]
    fn test_find_matching_close_simple() {
        let html = "<div>content</div>";
        let pos = find_matching_close(html, 5, "div");
        assert_eq!(pos, Some(12)); // position of '<' in '</div>'
    }

    #[test]
    fn test_find_matching_close_nested() {
        let html = "<div><div>inner</div>outer</div>";
        let pos = find_matching_close(html, 5, "div");
        assert_eq!(pos, Some(26)); // position of '<' in outer </div>
    }

    #[test]
    fn test_extract_tag_name() {
        assert_eq!(extract_tag_name("<div class=\"x\">"), Some("div"));
        assert_eq!(extract_tag_name("<main id=\"main-content\">"), Some("main"));
        assert_eq!(extract_tag_name("<article>"), Some("article"));
    }
}
