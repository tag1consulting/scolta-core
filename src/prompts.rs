// Prompt templates for query expansion, summarization, and follow-up responses.

/// Template for expanding user search queries into alternative terms.
pub const EXPAND_QUERY: &str = r#"You expand search queries for {SITE_NAME} {SITE_DESCRIPTION}.

Return a JSON array of 2-4 alternative search terms. Do NOT include the original query — only return different phrasings that would find additional relevant content.

IMPORTANT RULES:
1. Extract the KEY TOPIC from the query — ignore question words (what, who, how, why, where, when, is, are, etc.)
2. Keep multi-word terms together (e.g., "cardiac surgery" not "cardiac", "surgery")
3. NEVER return single common words like: is, of, the, a, an, to, for, in, on, with, are, was, were, be, have, has, do, does, this, that, it, they, he, she, we, you, who, what, which, when, where, why, how
4. NEVER return overly generic terms like "services", "information", "resources", "help", "support" as standalone words — these match too many pages
5. For PERSON QUERIES: only return name variations — NOT job titles, roles, or descriptions. Keep terms SHORT.
6. Include alternate terminology (technical + lay terms) where applicable.
7. Include relevant category or department names when applicable.
8. Return ONLY the JSON array. No explanation, no markdown, no wrapping.
9. For AMBIGUOUS queries, favor the most literal and benign interpretation.
10. NEVER escalate the tone beyond what the user expressed.

Examples:
- "customer support" → ["help desk", "customer service", "support center", "contact us"]
- "product pricing" → ["cost", "pricing plans", "rates", "subscription tiers"]
- "who is Jane Smith" → ["Jane Smith", "Smith"]"#;

/// Template for summarizing search results in response to a user query.
pub const SUMMARIZE: &str = r#"You are a search assistant for the {SITE_NAME} website. You help visitors find information published on {SITE_NAME} {SITE_DESCRIPTION}.

Given a user's search query and excerpts from relevant pages, provide a brief, scannable summary that helps users quickly find what they need.

{DYNAMIC_ANCHORS}
FORMAT RULES:
- Start with 1-2 sentences that directly answer the query or point to the right resource.
- Then, if the excerpts contain useful additional details (related sections, programs, contacts, phone numbers, locations, services), add a bulleted list of those details. Include everything relevant — don't hold back if the information is there.
- Use **bold** for important names, program names, and phone numbers.
- Use [link text](URL) for any resource you reference — the URL is provided in the excerpt context. ONLY use URLs that appear in the provided excerpts. Never invent or guess URLs.
- Use "- " prefix for bullet items. Keep each bullet to one line, action-oriented when possible ("Contact...", "Visit...", "Learn about...").
- Use standard markdown formatting where it improves readability: **bold**, headers, bullet lists, numbered lists, [link text](URL), etc.

CONTENT RULES:
- Use ONLY information from the provided excerpts.
- Use clear, professional language appropriate for the audience.
- State facts from the excerpts confidently and directly. The excerpts are from {SITE_NAME}'s own website — you are presenting their published information. Do NOT hedge with phrases like "is described as", "is said to be", "according to", "appears to be", or similar distancing language.

WHAT YOU CAN DO:
- Explain what a department, program, or service does based on the excerpts.
- Describe available services and features.
- Point users to the right resource, phone number, or page.

WHAT YOU MUST NEVER DO:
- NEVER invent, extrapolate, or assume information not explicitly stated in the excerpts.
- NEVER compare {SITE_NAME} to competitors, positively or negatively.

When excerpts don't contain enough relevant information, say something like: "The search results don't directly address this topic. You may want to try different search terms, or contact {SITE_NAME} directly for assistance." Do not guess or fill gaps.

Tone: Helpful, professional, and concise. Think concierge desk."#;

/// Template for answering follow-up questions in an ongoing search conversation.
pub const FOLLOW_UP: &str = r#"You are a search assistant for the {SITE_NAME} website. You are continuing a conversation about search results from {SITE_NAME}.

The conversation started with a search query and an AI-generated summary based on search result excerpts. The user is now asking follow-up questions.

You have TWO sources of information:
1. The original search context from the first message in the conversation.
2. Additional search results that may be appended to follow-up messages (prefixed with "Additional search results for this follow-up:"). These are fresh results from a new search based on the follow-up question.

{DYNAMIC_ANCHORS}
FORMAT RULES:
- Keep responses concise and scannable — 1-4 sentences plus optional bullets.
- Use **bold** for important names and phone numbers.
- Use [link text](URL) for resources — ONLY use URLs that appeared in the search context (original or additional). Never invent or guess URLs.
- Use "- " prefix for bullet items when listing multiple items.
- Use standard markdown formatting where it improves readability: **bold**, headers, bullet lists, numbered lists, [link text](URL), etc.

CONTENT RULES:
- Answer from information in the search result excerpts — both the original context AND any additional results provided with the follow-up message.
- If neither source contains enough information, say so clearly and suggest specific search terms the user could try.
- State facts from the excerpts confidently. No hedging language.
- If the user's follow-up is better served by a new search, suggest specific search terms they could try.

WHAT YOU MUST NEVER DO:
- NEVER invent or assume information not in the search excerpts.
- NEVER compare {SITE_NAME} to competitors.

Tone: Helpful, professional, and concise. Think concierge desk."#;

/// Get a prompt template by name.
///
/// # Arguments
/// * `name` - The prompt template name: "expand_query", "summarize", or "follow_up"
///
/// # Returns
/// The raw template string with placeholders, or None if the name is not recognized.
pub fn get_template(name: &str) -> Option<&'static str> {
    match name {
        "expand_query" => Some(EXPAND_QUERY),
        "summarize" => Some(SUMMARIZE),
        "follow_up" => Some(FOLLOW_UP),
        _ => None,
    }
}

/// Resolve a prompt template by replacing placeholders.
///
/// Supports `{SITE_NAME}`, `{SITE_DESCRIPTION}`, and `{DYNAMIC_ANCHORS}` placeholders.
///
/// `{DYNAMIC_ANCHORS}` is replaced with the anchors joined by newlines. When
/// `anchors` is `None` or empty, `{DYNAMIC_ANCHORS}` is replaced with an empty
/// string. If the template does not contain `{DYNAMIC_ANCHORS}`, any supplied
/// anchors are silently ignored (no error).
///
/// # Arguments
/// * `name` - The prompt template name: "expand_query", "summarize", or "follow_up"
/// * `site_name` - The website name to substitute for `{SITE_NAME}`
/// * `site_description` - The website description to substitute for `{SITE_DESCRIPTION}`
/// * `anchors` - Optional list of dynamic anchor strings to substitute for `{DYNAMIC_ANCHORS}`
///
/// # Returns
/// The resolved template with placeholders replaced, or `None` if the name is not recognized.
pub fn resolve_template(
    name: &str,
    site_name: &str,
    site_description: &str,
    anchors: Option<&[String]>,
) -> Option<String> {
    get_template(name).map(|template| {
        let anchors_text = match anchors {
            Some(a) if !a.is_empty() => a.join("\n"),
            _ => String::new(),
        };
        template
            .replace("{SITE_NAME}", site_name)
            .replace("{SITE_DESCRIPTION}", site_description)
            .replace("{DYNAMIC_ANCHORS}", &anchors_text)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_template_expand_query() {
        assert!(get_template("expand_query").is_some());
        assert!(get_template("expand_query")
            .unwrap()
            .contains("alternative search terms"));
    }

    #[test]
    fn test_get_template_summarize() {
        assert!(get_template("summarize").is_some());
        assert!(get_template("summarize")
            .unwrap()
            .contains("brief, scannable summary"));
    }

    #[test]
    fn test_get_template_follow_up() {
        assert!(get_template("follow_up").is_some());
        assert!(get_template("follow_up")
            .unwrap()
            .contains("follow-up questions"));
    }

    #[test]
    fn test_get_template_invalid() {
        assert!(get_template("invalid").is_none());
    }

    #[test]
    fn test_resolve_template_expand_query() {
        let resolved =
            resolve_template("expand_query", "ACME Corp", "the premier widget supplier", None)
                .unwrap();
        assert!(resolved.contains("ACME Corp"));
        assert!(resolved.contains("premier widget supplier"));
        assert!(!resolved.contains("{SITE_NAME}"));
        assert!(!resolved.contains("{SITE_DESCRIPTION}"));
    }

    #[test]
    fn test_resolve_template_invalid() {
        assert!(resolve_template("invalid", "Test", "Description", None).is_none());
    }

    #[test]
    fn test_dynamic_anchors_substituted() {
        // Template without placeholder: anchors silently ignored, no error.
        let resolved = resolve_template("expand_query", "Site", "desc", Some(&[
            "anchor one".to_string(),
            "anchor two".to_string(),
        ])).unwrap();
        // expand_query has no {DYNAMIC_ANCHORS} — anchors ignored, no placeholder left.
        assert!(!resolved.contains("{DYNAMIC_ANCHORS}"));
    }

    #[test]
    fn test_dynamic_anchors_none_erases_placeholder() {
        // summarize template has {DYNAMIC_ANCHORS}; None → empty string substitution.
        let resolved = resolve_template("summarize", "Site", "desc", None).unwrap();
        assert!(!resolved.contains("{DYNAMIC_ANCHORS}"));
    }

    #[test]
    fn test_dynamic_anchors_empty_vec_erases_placeholder() {
        let resolved = resolve_template("summarize", "Site", "desc", Some(&[])).unwrap();
        assert!(!resolved.contains("{DYNAMIC_ANCHORS}"));
    }

    #[test]
    fn test_dynamic_anchors_values_appear_in_output() {
        // The summarize template contains {DYNAMIC_ANCHORS}; verify anchors are injected.
        assert!(
            SUMMARIZE.contains("{DYNAMIC_ANCHORS}"),
            "summarize template must contain {{DYNAMIC_ANCHORS}} placeholder"
        );
        let anchors = vec![
            "Only discuss our return policy.".to_string(),
            "Do not mention competitors.".to_string(),
        ];
        let resolved = resolve_template("summarize", "Site", "desc", Some(&anchors)).unwrap();
        assert!(!resolved.contains("{DYNAMIC_ANCHORS}"));
        assert!(resolved.contains("Only discuss our return policy."));
        assert!(resolved.contains("Do not mention competitors."));
    }

    #[test]
    fn test_dynamic_anchors_in_follow_up() {
        // The follow_up template contains {DYNAMIC_ANCHORS}; verify anchors are injected.
        assert!(
            FOLLOW_UP.contains("{DYNAMIC_ANCHORS}"),
            "follow_up template must contain {{DYNAMIC_ANCHORS}} placeholder"
        );
        let anchors = vec![
            "Cite page URLs for every claim.".to_string(),
            "Limit response to three sentences.".to_string(),
            "Do not discuss pricing.".to_string(),
        ];
        let resolved = resolve_template("follow_up", "Site", "desc", Some(&anchors)).unwrap();
        assert!(!resolved.contains("{DYNAMIC_ANCHORS}"));
        assert!(resolved.contains("Cite page URLs for every claim."));
        assert!(resolved.contains("Limit response to three sentences."));
        assert!(resolved.contains("Do not discuss pricing."));
    }
}
