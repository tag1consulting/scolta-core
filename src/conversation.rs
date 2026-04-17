//! Conversation history trimming for multi-turn AI interactions.
//!
//! Removes the oldest message pairs when total conversation length exceeds a
//! character limit. Always preserves the first N messages (system prompt,
//! initial context) and removes in configurable units (default: pairs).

use serde::{Deserialize, Serialize};

/// A single turn in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Configuration for conversation truncation.
#[derive(Debug, Clone)]
pub struct ConversationConfig {
    /// Maximum total character length across all message contents. Default: 12000.
    pub max_length: u32,
    /// Always preserve the first N messages regardless of length. Default: 2.
    pub preserve_first_n: u32,
    /// Remove messages in groups of this size. Default: 2 (Q+A pairs).
    pub removal_unit: u32,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        ConversationConfig {
            max_length: 12000,
            preserve_first_n: 2,
            removal_unit: 2,
        }
    }
}

/// Trim a conversation to fit within `config.max_length` total characters.
///
/// The first `preserve_first_n` messages are never removed. Beyond that,
/// the oldest `removal_unit` messages are removed per iteration until the
/// conversation fits or no more messages can be removed.
pub fn truncate_conversation(
    mut messages: Vec<Message>,
    config: &ConversationConfig,
) -> Vec<Message> {
    let preserve = config.preserve_first_n as usize;
    let unit = config.removal_unit as usize;
    let max_len = config.max_length as usize;

    loop {
        let total: usize = messages.iter().map(|m| m.content.len()).sum();
        if total <= max_len {
            break;
        }

        // Nothing to remove if the removable section is empty.
        if messages.len() <= preserve {
            break;
        }

        let removable = messages.len() - preserve;
        let to_remove = unit.min(removable);
        if to_remove == 0 {
            break;
        }

        // Remove the oldest `to_remove` messages from the non-preserved section.
        messages.drain(preserve..preserve + to_remove);
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn test_already_under_limit_unchanged() {
        let msgs = vec![msg("user", "hello"), msg("assistant", "hi")];
        let cfg = ConversationConfig::default();
        let result = truncate_conversation(msgs.clone(), &cfg);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_removes_oldest_pair() {
        let msgs = vec![
            msg("system", "You are helpful."),          // preserved (index 0)
            msg("user", "Initial context."),            // preserved (index 1)
            msg("user", "Old question?"),               // oldest removable
            msg("assistant", "Old answer."),            // oldest removable
            msg("user", "New question?"),               // kept
            msg("assistant", "New answer."),            // kept
        ];
        let cfg = ConversationConfig {
            max_length: 60, // force removal
            preserve_first_n: 2,
            removal_unit: 2,
            ..Default::default()
        };
        let result = truncate_conversation(msgs, &cfg);
        // System and initial context always kept
        assert_eq!(result[0].role, "system");
        assert_eq!(result[1].content, "Initial context.");
        // Old pair removed
        assert!(!result.iter().any(|m| m.content == "Old question?"));
    }

    #[test]
    fn test_preserve_first_n_never_removed() {
        let long = "x".repeat(5000);
        let msgs = vec![
            msg("system", &long),
            msg("user", &long),
            msg("user", "short"),
            msg("assistant", "short"),
        ];
        let cfg = ConversationConfig {
            max_length: 100,
            preserve_first_n: 2,
            removal_unit: 2,
        };
        let result = truncate_conversation(msgs, &cfg);
        // First two must survive even though they alone exceed max_length
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "system");
    }

    #[test]
    fn test_removal_unit_one() {
        let msgs = vec![
            msg("system", "sys"),
            msg("user", "q1"),
            msg("assistant", "a1"),
            msg("user", "q2"),
        ];
        let cfg = ConversationConfig {
            max_length: 10,
            preserve_first_n: 1,
            removal_unit: 1,
        };
        let result = truncate_conversation(msgs, &cfg);
        // System message preserved; oldest non-preserved messages removed one by one
        assert_eq!(result[0].role, "system");
    }

    #[test]
    fn test_empty_conversation() {
        let cfg = ConversationConfig::default();
        let result = truncate_conversation(vec![], &cfg);
        assert!(result.is_empty());
    }

    #[test]
    fn test_all_preserved_cannot_be_removed() {
        let msgs = vec![
            msg("system", &"x".repeat(10000)),
            msg("user", &"y".repeat(10000)),
        ];
        let cfg = ConversationConfig {
            max_length: 100,
            preserve_first_n: 2, // both messages are preserved
            removal_unit: 2,
        };
        let result = truncate_conversation(msgs, &cfg);
        // Both preserved even though they exceed the limit
        assert_eq!(result.len(), 2);
    }
}
