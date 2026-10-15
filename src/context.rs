//! Context extraction for LLM summarization.
//!
//! Extracts the most relevant portions of an article for use as LLM context,
//! combining a fixed intro with keyword-anchored snippets and merging
//! overlapping ranges.

use crate::common;
use std::collections::BTreeMap;

/// Configuration for context extraction.
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// Maximum total character length of the extracted context. Default: 6000.
    pub max_length: u32,
    /// Length of the mandatory intro section in characters. Default: 2000.
    pub intro_length: u32,
    /// Characters to include before and after each keyword match. Default: 500.
    pub snippet_radius: u32,
    /// Separator inserted between extracted sections. Default: `"\n\n[...]\n\n"`.
    pub separator: String,
    /// ISO 639-1 language code for query stop-word filtering. Default: `"en"`.
    pub language: String,
}

impl Default for ContextConfig {
    fn default() -> Self {
        ContextConfig {
            max_length: 6000,
            intro_length: 2000,
            snippet_radius: 500,
            separator: "\n\n[...]\n\n".to_string(),
            language: "en".to_string(),
        }
    }
}

/// An input item for batch context extraction.
#[derive(Debug, Clone)]
pub struct ContextItem {
    pub content: String,
    pub url: String,
    pub title: String,
}

/// The result of extracting context from one item.
#[derive(Debug, Clone)]
pub struct ContextResult {
    pub url: String,
    pub title: String,
    pub context: String,
}

/// Extract the most relevant portion of `content` for the given `query`.
///
/// Algorithm:
/// 1. Return unchanged if content fits within `max_length`.
/// 2. Extract query terms; if none, return truncated-at-sentence content.
/// 3. Take the intro (first `intro_length` chars, truncated at sentence boundary).
/// 4. Find all keyword matches in the remainder; build ±`snippet_radius` ranges.
/// 5. If every merged range fits after the intro, keep them all. Otherwise keep
///    the windows of the rarest terms first (see `select_snippets`).
/// 6. Join intro + separator + snippets in page order; trim to `max_length` at
///    a sentence boundary as a final guard.
pub fn extract_context(content: &str, query: &str, config: &ContextConfig) -> String {
    let max_len = config.max_length as usize;
    let intro_len = config.intro_length as usize;
    let radius = config.snippet_radius as usize;
    let sep = &config.separator;

    if char_len(content) <= max_len {
        return content.to_string();
    }

    let terms = common::extract_terms(query, &config.language);
    if terms.is_empty() {
        return truncate_at_sentence(content, max_len).to_string();
    }

    // Extract intro at sentence boundary. Slice one char past intro_len so
    // truncate_at_sentence sees an over-long input and actually cuts back to
    // the last sentence boundary within intro_len — slicing exactly intro_len
    // chars made the call a no-op and every intro a mid-word hard cut.
    let intro_raw = char_slice(content, 0, (intro_len + 1).min(char_len(content)));
    let intro = truncate_at_sentence(intro_raw, intro_len).to_string();

    // Work on the text after the intro.
    let intro_byte_end = intro.len();
    let remaining = &content[intro_byte_end..];

    let matches = find_matches(remaining, &terms, radius);

    if matches.is_empty() {
        // No matches found in the remainder — return truncated intro + all of remaining.
        let combined = format!("{}{}{}", intro, sep, remaining);
        return truncate_at_sentence(&combined, max_len).to_string();
    }

    let budget = max_len.saturating_sub(char_len(&intro));
    let merged = choose_ranges(remaining, matches, terms.len(), char_len(sep), budget);

    // Build output, snippets in document order.
    let mut result = intro.clone();
    for (s, e) in &merged {
        result.push_str(sep);
        result.push_str(&remaining[*s..*e]);
    }

    if char_len(&result) > max_len {
        return truncate_at_sentence(&result, max_len).to_string();
    }

    result
}

/// One keyword match: its snippet window and position, as byte offsets into
/// the text after the intro, and the index of the query term it matched.
#[derive(Clone)]
struct Match {
    start: usize,
    end: usize,
    pos: usize,
    term: usize,
}

/// Every match of every term in `remaining`, each with its ±`radius` window
/// snapped to word boundaries.
///
/// The search runs over a lowercased copy, but `to_lowercase` is not
/// length-preserving ('İ' U+0130 grows 2 → 3 bytes), so positions found in
/// the lowered text are mapped back to original byte offsets before slicing.
fn find_matches(remaining: &str, terms: &[String], radius: usize) -> Vec<Match> {
    let mut matches: Vec<Match> = Vec::new();
    let (remaining_lower, lower_to_orig) = lowercase_with_offsets(remaining);

    for (term_idx, term) in terms.iter().enumerate() {
        let term_lower = term.to_lowercase();
        let mut search_from = 0usize;
        while search_from < remaining_lower.len() {
            match remaining_lower[search_from..].find(&term_lower) {
                None => break,
                Some(rel_pos) => {
                    let lower_pos = search_from + rel_pos;
                    let orig_pos = lower_to_orig[lower_pos];
                    let orig_end = lower_to_orig[lower_pos + term_lower.len()];
                    // Expand by snippet_radius bytes, then adjust to word/char boundaries.
                    let start =
                        word_boundary_start(remaining, byte_sub(remaining, orig_pos, radius));
                    let end = word_boundary_end(remaining, byte_add(remaining, orig_end, radius));
                    matches.push(Match {
                        start,
                        end,
                        pos: orig_pos,
                        term: term_idx,
                    });
                    // Advance by at least one character to prevent infinite loops.
                    search_from = lower_pos + term_lower.len().max(1);
                }
            }
        }
    }
    matches
}

/// The ranges of `remaining` to emit after the intro, merged, in page order.
///
/// If every merged window fits in `budget` characters (each with its
/// separator), all of them, exactly as before. This check is not just a
/// shortcut: taken one at a time in rarity order, a window can be refused
/// before a later one arrives that would have bridged it to a kept range, so
/// with three or more windows the walk can keep less than everything even
/// when everything fits. If not, the rarest terms' windows
/// ([`select_snippets`]). If not even one window fits, all of them
/// again, for the caller's final truncation, as before.
fn choose_ranges(
    remaining: &str,
    matches: Vec<Match>,
    term_count: usize,
    sep_len: usize,
    budget: usize,
) -> Vec<(usize, usize)> {
    let all = merge_ranges(sorted(matches.iter().map(|m| (m.start, m.end)).collect()));
    let all_len: usize = all
        .iter()
        .map(|(s, e)| sep_len + char_len(&remaining[*s..*e]))
        .sum();
    if all_len <= budget {
        return all;
    }
    let kept = select_snippets(remaining, matches, term_count, sep_len, budget);
    if kept.is_empty() {
        all
    } else {
        kept
    }
}

/// Choose the snippets that fit `budget` characters when not all of them do.
///
/// Snippets used to be kept in page order until the budget ran out, so a term
/// that occurs often near the top of a long page (six "Jeremy" matches before
/// character 2,263 of a team page) filled the budget before a term that occurs
/// once or twice, further down ("sailboat"), was reached. Windows are now
/// taken rarest term first: by how many times their term matched, then by
/// position. A window is kept when the merged result, each range preceded by
/// the separator, still fits; one that does not fit is passed over and the
/// walk goes on, since a later one may fit or may extend a kept range.
///
/// Cost: one sort of the matches and one pass over `remaining` to count the
/// characters before each byte; then, per match, a lookup in the kept ranges
/// (a `BTreeMap` from start to end) and the ranges it absorbs, each absorbed
/// once. A window that is refused also walks the kept ranges it overlaps;
/// since every window has the same radius and kept ranges begin and end at
/// spaces, that is a handful per window. Returns merged ranges in page order.
fn select_snippets(
    remaining: &str,
    mut matches: Vec<Match>,
    term_count: usize,
    sep_len: usize,
    budget: usize,
) -> Vec<(usize, usize)> {
    let mut counts = vec![0usize; term_count];
    for m in &matches {
        counts[m.term] += 1;
    }
    matches.sort_by_key(|m| (counts[m.term], m.pos));

    // Characters before each byte offset, so a range's length is a
    // subtraction (u32: a page of four billion characters is not a page).
    let mut before = vec![0u32; remaining.len() + 1];
    let mut n = 0u32;
    for (i, ch) in remaining.char_indices() {
        before[i..i + ch.len_utf8()].fill(n);
        n += 1;
    }
    before[remaining.len()] = n;
    let chars = |s: usize, e: usize| (before[e] - before[s]) as usize;

    // Disjoint kept ranges, start to end; ranges that touch are merged, as
    // merge_ranges does.
    let mut kept: BTreeMap<usize, usize> = BTreeMap::new();
    let mut used = 0usize;
    let mut absorbed: Vec<(usize, usize)> = Vec::new();
    for m in &matches {
        // The kept ranges this window overlaps or touches, right to left.
        absorbed.clear();
        absorbed.extend(
            kept.range(..=m.end)
                .rev()
                .take_while(|(_, end)| **end >= m.start)
                .map(|(s, e)| (*s, *e)),
        );
        let start = absorbed.last().map_or(m.start, |r| r.0.min(m.start));
        let end = absorbed.first().map_or(m.end, |r| r.1.max(m.end));
        let old: usize = absorbed.iter().map(|&(s, e)| sep_len + chars(s, e)).sum();
        let new = sep_len + chars(start, end);
        if used + new <= budget + old {
            for (s, _) in &absorbed {
                kept.remove(s);
            }
            kept.insert(start, end);
            used = used + new - old;
        }
    }

    kept.into_iter().collect()
}

fn sorted(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_by_key(|r| r.0);
    ranges
}

/// Extract context from multiple items in a single call.
pub fn batch_extract_context(
    items: Vec<ContextItem>,
    query: &str,
    config: &ContextConfig,
) -> Vec<ContextResult> {
    items
        .into_iter()
        .map(|item| ContextResult {
            context: extract_context(&item.content, query, config),
            url: item.url,
            title: item.title,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// String helpers (UTF-8 safe)
// ---------------------------------------------------------------------------

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Lowercase `s` for case-insensitive search, recording for every byte of the
/// lowered string the byte offset of the originating character in `s` (plus a
/// final sentinel entry equal to `s.len()`).
///
/// `to_lowercase` is not length-preserving, so a position found in the lowered
/// text cannot be used to slice `s` directly; `offsets[lower_pos]` gives the
/// corresponding original offset, always on a char boundary.
fn lowercase_with_offsets(s: &str) -> (String, Vec<usize>) {
    let mut lower = String::with_capacity(s.len());
    let mut offsets: Vec<usize> = Vec::with_capacity(s.len() + 1);
    for (orig_idx, ch) in s.char_indices() {
        for lc in ch.to_lowercase() {
            let start = lower.len();
            lower.push(lc);
            for _ in start..lower.len() {
                offsets.push(orig_idx);
            }
        }
    }
    offsets.push(s.len());
    (lower, offsets)
}

/// Return a slice of `s` from character `start` up to character `end`.
fn char_slice(s: &str, start_chars: usize, end_chars: usize) -> &str {
    let start_byte = s
        .char_indices()
        .nth(start_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let end_byte = s
        .char_indices()
        .nth(end_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[start_byte..end_byte]
}

/// Subtract `n` bytes from `pos`, keeping the result on a char boundary.
fn byte_sub(s: &str, pos: usize, n: usize) -> usize {
    let raw = pos.saturating_sub(n);
    align_char_start(s, raw)
}

/// Add `n` bytes to `pos`, clamped to `s.len()` and on a char boundary.
fn byte_add(s: &str, pos: usize, n: usize) -> usize {
    let raw = (pos + n).min(s.len());
    align_char_end(s, raw)
}

/// Walk backward to the nearest char boundary.
fn align_char_start(s: &str, mut pos: usize) -> usize {
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Walk forward to the nearest char boundary.
fn align_char_end(s: &str, mut pos: usize) -> usize {
    while pos < s.len() && !s.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

/// Scan backward from `from` for a space; return the start of the word.
fn word_boundary_start(s: &str, from: usize) -> usize {
    let from = align_char_start(s, from);
    if let Some(sp) = s[..from].rfind(' ') {
        sp + 1
    } else {
        0
    }
}

/// Scan forward from `from` for a space; return its position.
fn word_boundary_end(s: &str, from: usize) -> usize {
    let from = align_char_end(s, from.min(s.len()));
    if from >= s.len() {
        return s.len();
    }
    match s[from..].find(' ') {
        Some(rel) => from + rel,
        None => s.len(),
    }
}

/// Truncate `s` at a sentence boundary no later than `max_chars` characters.
///
/// Scans backward through the last 200 characters looking for `. `, `! `, `? `.
/// Falls back to the last space if no sentence boundary is found within 200 chars.
pub fn truncate_at_sentence(s: &str, max_chars: usize) -> &str {
    if char_len(s) <= max_chars {
        return s;
    }

    // Find byte offset of max_chars.
    let max_byte = s
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());

    let truncated = &s[..max_byte];

    // Scan the last 200 characters of `truncated` for a sentence boundary.
    let scan_chars = 200usize;
    let scan_start_chars = max_chars.saturating_sub(scan_chars);
    let scan_start_byte = s
        .char_indices()
        .nth(scan_start_chars)
        .map(|(i, _)| i)
        .unwrap_or(0);

    let scan_slice = &truncated[scan_start_byte..];
    let mut best_end: Option<usize> = None;

    for (bi, ch) in scan_slice.char_indices() {
        if ch == '.' || ch == '!' || ch == '?' {
            let next = bi + ch.len_utf8();
            if scan_slice[next..].starts_with(' ') || next == scan_slice.len() {
                best_end = Some(scan_start_byte + next);
            }
        }
    }

    if let Some(end) = best_end {
        &s[..end]
    } else if let Some(sp) = truncated.rfind(' ') {
        &s[..sp]
    } else {
        truncated
    }
}

/// Merge overlapping or adjacent byte ranges (must be sorted by start).
fn merge_ranges(sorted: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in sorted {
        match merged.last_mut() {
            Some(last) if s <= last.1 => {
                last.1 = last.1.max(e);
            }
            _ => {
                merged.push((s, e));
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_context_short_content_returned_unchanged() {
        let cfg = ContextConfig::default();
        let content = "Short content.";
        assert_eq!(extract_context(content, "query", &cfg), content);
    }

    #[test]
    fn test_extract_context_no_terms_truncates() {
        let cfg = ContextConfig {
            max_length: 20,
            ..Default::default()
        };
        let content = "The quick brown fox jumps over the lazy dog and more words here.";
        let result = extract_context(content, "the", &cfg); // all stop words → no terms
        assert!(result.chars().count() <= 20);
    }

    #[test]
    fn test_extract_context_includes_intro() {
        let long = "A".repeat(3000) + " drupal " + &"B".repeat(3000);
        let cfg = ContextConfig {
            max_length: 6000,
            intro_length: 100,
            snippet_radius: 50,
            separator: "\n...\n".to_string(),
            ..Default::default()
        };
        let result = extract_context(&long, "drupal", &cfg);
        // Result should include the intro (first 100 chars of A's)
        assert!(result.starts_with('A'));
    }

    #[test]
    fn test_extract_context_snippet_around_keyword() {
        let intro = "X".repeat(100);
        let body = format!("{}before drupal after{}", " ".repeat(600), " ".repeat(600));
        let content = intro + &body;
        let cfg = ContextConfig {
            max_length: 6000,
            intro_length: 50,
            snippet_radius: 30,
            separator: "|".to_string(),
            ..Default::default()
        };
        let result = extract_context(&content, "drupal", &cfg);
        assert!(result.contains("drupal"));
    }

    #[test]
    fn test_truncate_at_sentence_at_period() {
        let s = "Hello world. This is extra content.";
        let result = truncate_at_sentence(s, 15);
        assert_eq!(result, "Hello world.");
    }

    #[test]
    fn test_truncate_at_sentence_fallback_to_space() {
        let s = "Hello worldextra content here yes";
        let result = truncate_at_sentence(s, 12);
        assert!(!result.ends_with(' '));
        assert!(result.len() <= 12);
    }

    #[test]
    fn test_merge_ranges_overlapping() {
        let ranges = vec![(0, 10), (5, 15), (20, 30)];
        let merged = merge_ranges(ranges);
        assert_eq!(merged, vec![(0, 15), (20, 30)]);
    }

    #[test]
    fn test_merge_ranges_adjacent() {
        let ranges = vec![(0, 10), (10, 20)];
        let merged = merge_ranges(ranges);
        assert_eq!(merged, vec![(0, 20)]);
    }

    #[test]
    fn test_batch_extract_context() {
        let cfg = ContextConfig::default();
        let items = vec![
            ContextItem {
                content: "Short content.".to_string(),
                url: "https://a.com".to_string(),
                title: "A".to_string(),
            },
            ContextItem {
                content: "Another short piece.".to_string(),
                url: "https://b.com".to_string(),
                title: "B".to_string(),
            },
        ];
        let results = batch_extract_context(items, "drupal", &cfg);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].url, "https://a.com");
        assert_eq!(results[1].url, "https://b.com");
    }

    #[test]
    fn test_language_plumbs_to_stop_word_filtering() {
        // "der" is a German stop word: with language "de" it yields no terms,
        // so nothing anchors a snippet deep in the content. With "en" it is a
        // meaningful term and must anchor one.
        let content = format!("{}xx der yy {}", "A ".repeat(2500), "B ".repeat(2000));
        let de_cfg = ContextConfig {
            max_length: 3000,
            intro_length: 500,
            snippet_radius: 30,
            separator: "|".to_string(),
            language: "de".to_string(),
        };
        let de_result = extract_context(&content, "der", &de_cfg);
        assert!(
            !de_result.contains("xx der"),
            "German stop word query must not anchor a snippet"
        );

        let en_cfg = ContextConfig {
            language: "en".to_string(),
            ..de_cfg
        };
        let en_result = extract_context(&content, "der", &en_cfg);
        assert!(
            en_result.contains("xx der yy"),
            "with language=en, 'der' is a meaningful term and must anchor a snippet"
        );
    }

    #[test]
    fn test_german_query_anchors_with_german_stop_words() {
        // Query mixes a German stop word with a content word; the content word
        // must still anchor its snippet under language "de".
        let content = format!(
            "{}vorher drupal nachher {}",
            "A ".repeat(2500),
            "B ".repeat(2000)
        );
        let cfg = ContextConfig {
            max_length: 3000,
            intro_length: 500,
            snippet_radius: 30,
            separator: "|".to_string(),
            language: "de".to_string(),
        };
        let result = extract_context(&content, "und drupal", &cfg);
        assert!(
            result.contains("vorher drupal nachher"),
            "content term in a German query must anchor a snippet"
        );
    }

    #[test]
    fn test_intro_truncates_at_sentence_boundary() {
        // intro_length cuts mid-sentence, but a sentence boundary sits shortly
        // before the cut — the intro must end there, not mid-word.
        let content = format!(
            "First sentence ends here. Second sentence is much longer and keeps going. {}",
            "filler ".repeat(2000)
        );
        let cfg = ContextConfig {
            max_length: 200,
            intro_length: 40, // falls inside "Second sentence …"
            snippet_radius: 20,
            separator: "|".to_string(),
            language: "en".to_string(),
        };
        // Query term does not occur → fallback path is intro + separator + remaining.
        let result = extract_context(&content, "zzznotfound", &cfg);
        assert!(
            result.starts_with("First sentence ends here.|"),
            "intro must be cut at the sentence boundary; got: {:?}",
            &result[..60.min(result.len())]
        );
    }

    #[test]
    fn test_case_expanding_chars_do_not_misalign_snippets() {
        // 'İ' (U+0130) is 2 bytes but lowercases to 3 ("i" + combining dot),
        // so positions found in lowercased text drift +1 byte per İ against
        // the original. 100 İ's before the keyword previously pushed the
        // snippet window 100 bytes past the keyword.
        let content = format!(
            "{} {} drupal kernwort {}",
            "A".repeat(2000),
            "İ".repeat(100),
            "B ".repeat(2000)
        );
        let cfg = ContextConfig {
            max_length: 3000,
            intro_length: 1000,
            snippet_radius: 20,
            separator: "|".to_string(),
            language: "en".to_string(),
        };
        let result = extract_context(&content, "drupal", &cfg);
        assert!(
            result.contains("drupal"),
            "snippet must contain the matched keyword despite case-expanding chars"
        );
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    // -----------------------------------------------------------------------
    // Snippet selection when not every snippet fits
    // -----------------------------------------------------------------------

    fn cfg(max: u32, intro: u32, radius: u32, sep: &str) -> ContextConfig {
        ContextConfig {
            max_length: max,
            intro_length: intro,
            snippet_radius: radius,
            separator: sep.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_rare_late_term_beats_common_early_term() {
        // The shape of a long team page: the person's name is everywhere near
        // the top, the fact asked about appears once, far down. Snippets used
        // to be kept in page order, so the name's windows filled the budget
        // and "sailboat" never made it in.
        let mut content = "Our team. ".to_string();
        for i in 0..20 {
            content.push_str(&format!("Jeremy note {i} {} ", "filler ".repeat(40)));
        }
        content.push_str("He once lived aboard a sailboat in the Caribbean. ");
        content.push_str(&"tail ".repeat(400));
        let result = extract_context(&content, "jeremy sailboat", &cfg(1200, 200, 80, "\n\n"));
        assert!(result.contains("aboard a sailboat"), "got: {result}");
        assert!(
            result.contains("Jeremy note"),
            "the common term still gets room"
        );
        assert!(result.chars().count() <= 1200);
    }

    #[test]
    fn test_selected_snippets_keep_page_order_and_earliest_on_a_tie() {
        // Over budget (55 characters after the intro): room for two windows. "beta" (one match) goes first,
        // then the earliest of the three "alpha" windows; they come out in
        // page order, alpha before beta.
        let content = format!(
            "Intro sentence here. {}alpha one {}beta two {}alpha three {}alpha four {}",
            "x ".repeat(40),
            "y ".repeat(40),
            "z ".repeat(40),
            "v ".repeat(40),
            "w ".repeat(40)
        );
        let result = extract_context(&content, "alpha beta", &cfg(75, 25, 10, "|"));
        assert_eq!(
            result,
            "Intro sentence here.|x x x x x alpha one y y y|y y y y y beta two z z z"
        );
    }

    #[test]
    fn test_unchanged_when_every_snippet_fits() {
        // The output main e3bdf44 gives for this input, byte for byte.
        let content = format!(
            "Intro sentence here. {}drupal one {}drupal two {}",
            "a ".repeat(60),
            "b ".repeat(60),
            "c ".repeat(60)
        );
        assert_eq!(
            extract_context(&content, "drupal", &cfg(300, 30, 10, "|")),
            "Intro sentence here.|a a a a a drupal one b b b|b b b b b drupal two c c c"
        );
    }

    #[test]
    fn test_separator_counts_against_the_budget() {
        // The windows are 24, 26 and 26 characters and the separator 9. The
        // budget after the intro (100 - 20 = 80) holds two windows with their
        // separators (68), not three (103). Without the separators the three
        // would come to 76 and all be kept.
        let content = format!(
            "Intro sentence here. {}rare one {}common two {}common three {}",
            "a ".repeat(40),
            "b ".repeat(40),
            "c ".repeat(40),
            "d ".repeat(40)
        );
        let two = "Intro sentence here.\n\n[...]\n\na a a a a rare one b b b\n\n[...]\n\nb b b b b common two c c c";
        let one = "Intro sentence here.\n\n[...]\n\na a a a a rare one b b b";
        let sep = "\n\n[...]\n\n";
        assert_eq!(
            extract_context(&content, "rare common", &cfg(100, 25, 10, sep)),
            two
        );
        // Exactly 68 characters of budget hold the same two; 67 only one.
        assert_eq!(
            extract_context(&content, "rare common", &cfg(88, 25, 10, sep)),
            two
        );
        assert_eq!(
            extract_context(&content, "rare common", &cfg(87, 25, 10, sep)),
            one
        );
        // A separator is counted in characters: "…" is three bytes, one character.
        assert_eq!(
            extract_context(&content, "rare common", &cfg(84, 25, 10, "\n\n[…]\n\n")),
            two.replace("[...]", "[…]")
        );
    }

    #[test]
    fn test_budget_is_what_the_intro_leaves() {
        // A 188 character intro leaves 62 of 250: room for the rare window and
        // one common one, not for all three. The intro is counted in
        // characters, accented ones included.
        for sentence in ["Long intro sentence. ", "Lóng íntró séntéñcé. "] {
            let intro = sentence.repeat(9);
            let content = format!(
                "{intro}{}rare one {}common two {}common three {}",
                "a ".repeat(40),
                "b ".repeat(40),
                "c ".repeat(40),
                "d ".repeat(40)
            );
            let result = extract_context(&content, "rare common", &cfg(250, 190, 10, "|"));
            assert_eq!(
                result,
                format!(
                    "{}|a a a a a rare one b b b|b b b b b common two c c c",
                    intro.trim_end()
                )
            );
        }
    }

    /// The selection the long way: try each window in rarity order, re-merge
    /// everything kept, and recount every character. `choose_ranges` must
    /// agree with it on every input.
    fn naive_choose(
        remaining: &str,
        matches: &[Match],
        term_count: usize,
        sep_len: usize,
        budget: usize,
    ) -> Vec<(usize, usize)> {
        let cost = |ranges: &[(usize, usize)]| -> usize {
            merge_ranges(sorted(ranges.to_vec()))
                .iter()
                .map(|(s, e)| sep_len + char_len(&remaining[*s..*e]))
                .sum()
        };
        let all: Vec<(usize, usize)> = matches.iter().map(|m| (m.start, m.end)).collect();
        if cost(&all) <= budget {
            return merge_ranges(sorted(all));
        }
        let mut counts = vec![0usize; term_count];
        for m in matches {
            counts[m.term] += 1;
        }
        let mut order: Vec<&Match> = matches.iter().collect();
        order.sort_by_key(|m| (counts[m.term], m.pos));
        let mut kept: Vec<(usize, usize)> = Vec::new();
        for m in order {
            let mut trial = kept.clone();
            trial.push((m.start, m.end));
            if cost(&trial) <= budget {
                kept = trial;
            }
        }
        if kept.is_empty() {
            merge_ranges(sorted(all))
        } else {
            merge_ranges(sorted(kept))
        }
    }

    #[test]
    fn test_selection_matches_the_naive_reference() {
        // 5,000 seeded pseudo random pages over a small vocabulary, with multibyte
        // and four byte characters, double spaces (ranges that touch) and
        // long tokens, at many budgets, radii and separator lengths.
        let words = [
            "alpha",
            "beta",
            "gamma",
            "delta",
            "x",
            "yy",
            "caffè",
            "日本",
            "🇮🇹",
            "😀😀",
            "",
            "zzzzzzzzzzzzzzzzzzzz",
        ];
        let terms: Vec<String> = ["alpha", "beta", "gamma", "caffè", "日本", "😀"]
            .iter()
            .map(|t| t.to_string())
            .collect();
        let mut seed: u64 = 0x5eed;
        let mut next = |n: usize| -> usize {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n
        };
        let mut checked = 0;
        while checked < 5000 {
            let len = 5 + next(120);
            let page: Vec<&str> = (0..len).map(|_| words[next(words.len())]).collect();
            let remaining = page.join(" ");
            let used_terms: Vec<String> = terms.iter().filter(|_| next(2) == 0).cloned().collect();
            if used_terms.is_empty() {
                continue;
            }
            let radius = next(40);
            let sep_len = next(12);
            let matches = find_matches(&remaining, &used_terms, radius);
            if matches.is_empty() {
                continue;
            }
            // A random budget, and the exact cost of keeping every window.
            let all: Vec<(usize, usize)> = matches.iter().map(|m| (m.start, m.end)).collect();
            let exact: usize = merge_ranges(sorted(all))
                .iter()
                .map(|(s, e)| sep_len + char_len(&remaining[*s..*e]))
                .sum();
            checked += 1;
            for budget in [next(250), exact] {
                let expected =
                    naive_choose(&remaining, &matches, used_terms.len(), sep_len, budget);
                let got = choose_ranges(
                    &remaining,
                    matches.clone(),
                    used_terms.len(),
                    sep_len,
                    budget,
                );
                assert_eq!(
                    got, expected,
                    "page {remaining:?} terms {used_terms:?} radius {radius} sep {sep_len} budget {budget}"
                );
            }
        }
    }

    #[test]
    fn test_overflow_order_is_rarest_term_then_position() {
        // Four terms matched 1, 2, 2 and 3 times, and room for three windows
        // (85 characters after the intro, each window 26 with its separator).
        // "kiwi" goes first; "pear" and "plum" tie on two matches, so their
        // windows go by position: "pear one", then "plum one", and the budget
        // is spent before "pear two" or any "lime". The kept windows come out
        // in page order. "plum" is named before "pear" in the query, so its
        // matches are found first: ordering by count alone would keep both
        // "plum" windows.
        let content = format!(
            "Intro sentence here. {}lime one {}pear one {}plum one {}lime two {}pear two {}plum two {}lime six {}kiwi one {}",
            "a ".repeat(40),
            "b ".repeat(40),
            "c ".repeat(40),
            "d ".repeat(40),
            "e ".repeat(40),
            "f ".repeat(40),
            "g ".repeat(40),
            "h ".repeat(40),
            "i ".repeat(40)
        );
        let result = extract_context(&content, "lime plum pear kiwi", &cfg(105, 25, 10, "|"));
        assert_eq!(
            result,
            "Intro sentence here.|b b b b b pear one c c c|c c c c c plum one d d d|h h h h h kiwi one i i i"
        );
    }

    #[test]
    fn test_unchanged_when_no_window_fits() {
        // The intro leaves too little room for any window: page order and the
        // final truncation, as on main e3bdf44, byte for byte. The second case
        // has an intro longer than max_length, which nothing clamps (a JSON
        // config of only max_length keeps the default intro of 2000).
        let content = format!(
            "One. Two. Three. Four. {} sailboat story {}",
            "x ".repeat(100),
            "y ".repeat(100)
        );
        assert_eq!(
            extract_context(&content, "sailboat", &cfg(30, 30, 10, "|")),
            "One. Two. Three."
        );
        assert_eq!(
            extract_context(&content, "sailboat", &cfg(15, 100, 10, "|")),
            "One. Two."
        );
    }

    #[test]
    fn test_window_too_big_is_passed_over() {
        // The rarest term's window is stretched by a long token past the 60
        // characters of room (to 415 or 95 characters). Either way it is
        // refused, the walk goes on, and the two "beta" windows, which
        // overlap and merge, are kept.
        for long in [400, 80] {
            let content = format!(
                "Intro sentence here. {}beta one beta two {}alpha{} {}",
                "a ".repeat(40),
                "b ".repeat(40),
                "z".repeat(long),
                "c ".repeat(40)
            );
            assert_eq!(
                extract_context(&content, "alpha beta", &cfg(80, 25, 10, "|")),
                "Intro sentence here.|a a a a a beta one beta two b b b",
                "token of {long}"
            );
        }
    }

    #[test]
    fn test_windows_are_measured_in_characters() {
        // Emoji are four bytes a character, so this window is more than three
        // bytes per character. Counted in characters it fits the room
        // exactly; counted in bytes no window fits, and the output would fall
        // back to page order cut at max_length.
        let content = format!(
            "Intro sentence here. {}rare {}{}common",
            "😀😀😀😀 ".repeat(20),
            "😀😀😀😀 ".repeat(20),
            "o ".repeat(40)
        );
        let probe = extract_context(&content, "rare", &cfg(300, 25, 40, "|"));
        let window = probe.split('|').nth(1).expect("the rare window");
        assert!(window.len() > 3 * window.chars().count());
        let max = 21 + u32::try_from(window.chars().count()).unwrap();
        assert_eq!(
            extract_context(&content, "rare common", &cfg(max, 25, 40, "|")),
            format!("Intro sentence here.|{window}")
        );
    }

    #[test]
    fn test_goose_post_keeps_the_core_count() {
        // The tag1.com post "A Goose In The Clouds" (2021), as Pagefind
        // indexes it. "goose" matches 29 times; the sentence that answers
        // "how many of the 32 cores?" sits at character 6,495. Page order
        // spent the budget on early "goose" windows; this is the chat's
        // per result budget with the upstream intro, radius and separator.
        let content = include_str!("../tests/fixtures/goose_clouds_post.txt");
        let query = "According to this article, how many of the 32 cores did Goose need to avoid being CPU-throttled?";
        let result = extract_context(content, query, &cfg(1200, 200, 80, "\n\n"));
        assert!(result.contains("22 of the 32"), "got: {result}");
        assert!(result.chars().count() <= 1200);
    }

    // -----------------------------------------------------------------------
    // UTF-8 safety — snippet boundaries must never split a multi-byte char
    // -----------------------------------------------------------------------

    mod utf8_safety {
        use super::*;

        fn is_valid_utf8(s: &str) -> bool {
            std::str::from_utf8(s.as_bytes()).is_ok()
        }

        // Content composed entirely of 2-byte chars (é) with a keyword buried
        // inside.  Snippet radius expansion may land on an odd byte offset —
        // byte_sub/byte_add must align to a char boundary.
        #[test]
        fn multibyte_content_contains_keyword() {
            let intro = "é".repeat(2500); // 5000 bytes, 2500 chars
            let body = " drupal ".to_string() + &"è".repeat(1000);
            let content = intro + &body;

            let cfg = ContextConfig {
                max_length: 3000,
                intro_length: 2000,
                snippet_radius: 200,
                separator: "\n...\n".to_string(),
                ..Default::default()
            };

            let result = extract_context(&content, "drupal", &cfg);
            assert!(is_valid_utf8(&result), "result must be valid UTF-8");
            assert!(result.contains("drupal"), "result must contain keyword");
            assert!(
                result.chars().count() <= cfg.max_length as usize,
                "result must fit within max_length"
            );
        }

        // Keyword that itself contains a multi-byte char ("caffè").  The snippet
        // radius may land one byte past an even position — inside an `è` — and
        // byte_sub must back up to the char boundary.
        #[test]
        fn caffe_keyword_at_multibyte_boundary() {
            let filler = "è".repeat(300); // 600 bytes; radius 299 → raw = 301 (odd)
            let intro = "A".repeat(2000);
            let body = filler + "caffè " + &"B".repeat(600);
            let content = intro + &body;

            let cfg = ContextConfig {
                max_length: 4000,
                intro_length: 2000,
                snippet_radius: 299, // raw start = 600 - 299 = 301; not a char boundary
                separator: "|".to_string(),
                ..Default::default()
            };

            let result = extract_context(&content, "caffè", &cfg);
            assert!(is_valid_utf8(&result), "result must be valid UTF-8");
            assert!(result.contains("caffè"), "result must contain keyword");
        }

        // Flag emoji (🇮🇹, 8 bytes each) adjacent to the keyword.  Small radius
        // makes it likely the snippet window boundary falls inside an emoji.
        #[test]
        fn emoji_neighbor_no_panic() {
            let emoji = "🇮🇹"; // 8 bytes, 1 char (well, 2 code points rendered as 1 glyph)
            let intro = "A".repeat(2000);
            let body = format!("{} drupal {}", emoji.repeat(60), emoji.repeat(60));
            let content = intro + &body;

            let cfg = ContextConfig {
                max_length: 3000,
                intro_length: 2000,
                snippet_radius: 9, // 9 < 16 (two emojis); likely to hit inside an emoji
                separator: "|".to_string(),
                ..Default::default()
            };

            let result = extract_context(&content, "drupal", &cfg);
            assert!(is_valid_utf8(&result), "result must be valid UTF-8");
            assert!(result.contains("drupal"), "result must contain keyword");
        }

        // truncate_at_sentence: period followed by a 2-byte char (È).
        // The function must correctly find the sentence boundary even when the
        // next character after `. ` is multi-byte.
        #[test]
        fn truncate_finds_period_before_multibyte() {
            let s = "Hello world. È molto bello e lungo testo extra qui davvero.";
            // max_chars=15 → truncated contains "Hello world. È " (bytes 0-15)
            // Sentence boundary '.' at char 11 / byte 11, followed by ' '.
            let result = truncate_at_sentence(s, 15);
            assert_eq!(
                result, "Hello world.",
                "should truncate at sentence boundary"
            );
            assert!(is_valid_utf8(result), "result must be valid UTF-8");
        }

        // truncate_at_sentence with CJK content (no ASCII sentence terminators,
        // no spaces) — must not panic, and must return valid UTF-8.
        #[test]
        fn truncate_cjk_no_ascii_terminator() {
            // Chinese text with no ASCII periods or spaces.
            let s = "这是很长的中文文本不包含标点符号需要截断处理验证不会崩溃";
            let result = truncate_at_sentence(s, 5);
            assert!(
                result.chars().count() <= 5,
                "result must not exceed max_chars"
            );
            assert!(is_valid_utf8(result), "result must be valid UTF-8");
        }

        // The selection path with multibyte text: a common accented term early,
        // a rare one late, a budget that holds only some windows.
        #[test]
        fn selection_with_multibyte_content() {
            let mut content = "Già detto. ".to_string();
            for _ in 0..30 {
                content.push_str("perché città caffè è qui ");
            }
            content.push_str(&"ò ".repeat(200));
            content.push_str(" İstanbul 日本語 🇮🇹 Ünïcödé finale ");
            content.push_str(&"ù ".repeat(200));
            let cfg = ContextConfig {
                max_length: 400,
                intro_length: 50,
                snippet_radius: 30,
                separator: "\n\n[...]\n\n".to_string(),
                ..Default::default()
            };
            let result = extract_context(&content, "caffè ünïcödé", &cfg);
            assert!(is_valid_utf8(&result));
            assert!(result.contains("Ünïcödé"), "rare term kept: {result}");
            assert!(result.contains("caffè"));
            assert!(result.chars().count() <= 400);
        }

        // A query term that starts with a multibyte character, on text with
        // no spaces, where every window stretches past the budget.
        #[test]
        fn multibyte_initial_term_on_spaceless_text() {
            let content = format!(
                "{}東京タワー{}",
                "日本語のページです。".repeat(900),
                "おわりです。".repeat(900)
            );
            for (max, intro) in [(1200, 200), (3000, 200), (6000, 2000)] {
                let cfg = ContextConfig {
                    max_length: max,
                    intro_length: intro,
                    snippet_radius: 80,
                    separator: "\n\n".to_string(),
                    ..Default::default()
                };
                let result = extract_context(&content, "東京タワー", &cfg);
                assert!(is_valid_utf8(&result));
                assert!(result.chars().count() <= max as usize);
            }
        }

        // Two terms separated by a 100-char run of è (200 bytes).  With a radius
        // large enough for the ranges to overlap, merge_ranges produces one
        // combined range that must be a valid byte slice of `remaining`.
        #[test]
        fn multibyte_filler_ranges_merge_correctly() {
            let filler = "è".repeat(100); // 200 bytes, 100 chars
            let intro = "A".repeat(2000);
            let body = "alpha ".to_string() + &filler + " beta " + &"C".repeat(1000);
            let content = intro + &body;

            let cfg = ContextConfig {
                max_length: 5000,
                intro_length: 2000,
                snippet_radius: 300, // large enough for alpha+beta ranges to overlap
                separator: "|".to_string(),
                ..Default::default()
            };

            let result = extract_context(&content, "alpha beta", &cfg);
            assert!(is_valid_utf8(&result), "result must be valid UTF-8");
            assert!(result.contains("alpha"), "result must contain 'alpha'");
            assert!(result.contains("beta"), "result must contain 'beta'");
        }
    }
}
