//! Typed error handling for scolta-core.
//!
//! Every error includes the originating function name so that when an error
//! propagates to the calling JavaScript and lands in the browser console, the
//! developer can immediately identify which WASM function produced it.

use std::fmt;

/// Structured error type for all scolta-core operations.
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
    UnknownPrompt { name: String },

    /// Failed to parse or process input data.
    ParseError {
        function: &'static str,
        detail: String,
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
            Self::ParseError { function, detail } => {
                write!(f, "{}: {}", function, detail)
            }
        }
    }
}

impl std::error::Error for ScoltaError {}

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
}
