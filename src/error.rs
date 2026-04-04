//! Typed error handling for scolta-core-wasm.
//!
//! Every error includes the originating function name so that when an error
//! propagates through an Extism host (PHP, Python, JS, Go, etc.) and lands
//! in a log file, the developer can immediately identify which WASM function
//! produced it without tracing the call stack.
//!
//! The [`ScoltaError`] enum covers all error categories the crate can produce.
//! Its [`Display`] implementation generates human-readable messages suitable
//! for both log files and Extism host error handling.

use std::fmt;

/// Structured error type for all scolta-core-wasm operations.
///
/// Each variant includes enough context for a platform plugin maintainer
/// to diagnose the problem without reading the Rust source. The `function`
/// field (where present) names the WASM export that failed.
#[derive(Debug, Clone)]
pub enum ScoltaError {
    /// Input could not be parsed as valid JSON.
    InvalidJson {
        function: &'static str,
        detail: String,
    },

    /// A required input field was not present.
    MissingField {
        function: &'static str,
        field: &'static str,
    },

    /// A field was present but had the wrong type.
    InvalidFieldType {
        function: &'static str,
        field: &'static str,
        expected: &'static str,
    },

    /// The requested prompt template name does not exist.
    UnknownPrompt {
        name: String,
    },

    /// The function name passed to `debug_call` is not recognized.
    UnknownFunction {
        name: String,
    },

    /// Failed to parse or process input data.
    ParseError {
        function: &'static str,
        detail: String,
    },

    /// A configuration value is out of its valid range.
    /// This is a warning-level issue: the operation proceeds with a default.
    ConfigWarning {
        field: &'static str,
        message: String,
    },
}

impl fmt::Display for ScoltaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson { function, detail } => {
                write!(f, "{}: invalid JSON input: {}", function, detail)
            }
            Self::MissingField { function, field } => {
                write!(f, "{}: missing required field '{}'", function, field)
            }
            Self::InvalidFieldType {
                function,
                field,
                expected,
            } => {
                write!(f, "{}: field '{}' must be {}", function, field, expected)
            }
            Self::UnknownPrompt { name } => {
                write!(f, "resolve_prompt: unknown prompt template '{}'", name)
            }
            Self::UnknownFunction { name } => {
                write!(f, "debug_call: unknown function '{}'", name)
            }
            Self::ParseError { function, detail } => {
                write!(f, "{}: {}", function, detail)
            }
            Self::ConfigWarning { field, message } => {
                write!(f, "config warning: field '{}': {}", field, message)
            }
        }
    }
}

impl std::error::Error for ScoltaError {}

/// Convenience constructor for JSON parse errors at the plugin_fn boundary.
impl ScoltaError {
    pub fn invalid_json(function: &'static str, err: impl fmt::Display) -> Self {
        Self::InvalidJson {
            function,
            detail: err.to_string(),
        }
    }

    pub fn missing_field(function: &'static str, field: &'static str) -> Self {
        Self::MissingField { function, field }
    }

    pub fn parse_error(function: &'static str, detail: impl fmt::Display) -> Self {
        Self::ParseError {
            function,
            detail: detail.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_function_name() {
        let err = ScoltaError::missing_field("score_results", "query");
        let msg = err.to_string();
        assert!(msg.contains("score_results"));
        assert!(msg.contains("query"));
    }

    #[test]
    fn test_error_display_invalid_json() {
        let err = ScoltaError::invalid_json("clean_html", "expected object");
        assert_eq!(
            err.to_string(),
            "clean_html: invalid JSON input: expected object"
        );
    }

    #[test]
    fn test_error_display_unknown_prompt() {
        let err = ScoltaError::UnknownPrompt {
            name: "nonexistent".to_string(),
        };
        assert!(err.to_string().contains("nonexistent"));
        assert!(err.to_string().contains("resolve_prompt"));
    }

    #[test]
    fn test_error_display_config_warning() {
        let err = ScoltaError::ConfigWarning {
            field: "recency_boost_max",
            message: "value 5.0 exceeds reasonable range (0.0-2.0), using default 0.5".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("recency_boost_max"));
        assert!(msg.contains("config warning"));
    }
}
