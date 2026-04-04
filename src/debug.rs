//! Debug and logging utilities for tracking plugin performance.
//!
//! The [`measure_call`] function wraps any operation with timing and size
//! metrics. Used by the `debug_call` plugin export to profile individual
//! functions, and available for platform adapters implementing their own
//! debug logging.
//!
//! # WASI compatibility
//!
//! [`std::time::Instant`] works on `wasm32-wasip1` because WASI provides
//! `clock_time_get`. The `eprintln!` logging is gated behind
//! `#[cfg(not(target_arch = "wasm32"))]` since stderr behavior varies
//! across WASM runtimes.

use std::time::Instant;

/// Result of a measured function call with timing and size metrics.
#[derive(Debug, Clone)]
pub struct DebugResult {
    /// Function output (on success) or `None` (on error).
    pub output: Option<String>,
    /// Error message, if the function failed.
    pub error: Option<String>,
    /// Elapsed time in microseconds.
    pub time_us: u128,
    /// Input size in bytes.
    pub input_size: usize,
    /// Output size in bytes (0 if error).
    pub output_size: usize,
}

/// Measure a function call that can succeed or fail.
///
/// Returns a [`DebugResult`] with separate `output` and `error` fields
/// so callers can distinguish success from failure without parsing the
/// output string.
///
/// # Arguments
/// * `name` - Function name for logging
/// * `input` - Input text (for size measurement)
/// * `f` - Closure that performs the work
///
/// # Returns
/// [`DebugResult`] with timing, sizes, and either output or error
pub fn measure_call<F>(name: &str, input: &str, f: F) -> DebugResult
where
    F: FnOnce() -> Result<String, String>,
{
    let input_size = input.len();
    let start = Instant::now();

    let result = f();

    let elapsed = start.elapsed();
    let time_us = elapsed.as_micros();

    let (output, error, output_size) = match result {
        Ok(out) => {
            let size = out.len();
            (Some(out), None, size)
        }
        Err(err) => (None, Some(err), 0),
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        let status = if error.is_some() { "ERROR" } else { "OK" };
        eprintln!(
            "[DEBUG] {} [{}]: {:.3}ms (input: {} bytes, output: {} bytes)",
            name,
            status,
            time_us as f64 / 1000.0,
            input_size,
            output_size
        );
    }

    DebugResult {
        output,
        error,
        time_us,
        input_size,
        output_size,
    }
}

/// Format a DebugResult as a JSON value.
///
/// Output shape:
/// ```json
/// {
///   "output": "..." | null,
///   "error": "..." | null,
///   "time_us": 1234,
///   "input_size": 56,
///   "output_size": 78
/// }
/// ```
pub fn debug_result_to_json(result: &DebugResult) -> serde_json::Value {
    serde_json::json!({
        "output": result.output,
        "error": result.error,
        "time_us": result.time_us,
        "input_size": result.input_size,
        "output_size": result.output_size
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measure_call_success() {
        let input = "test input";
        let result = measure_call("test_func", input, || Ok("test output".to_string()));

        assert_eq!(result.output, Some("test output".to_string()));
        assert!(result.error.is_none());
        assert_eq!(result.input_size, input.len());
        assert_eq!(result.output_size, "test output".len());
    }

    #[test]
    fn test_measure_call_error() {
        let result = measure_call("test_func", "input", || {
            Err("something went wrong".to_string())
        });

        assert!(result.output.is_none());
        assert_eq!(result.error, Some("something went wrong".to_string()));
        assert_eq!(result.output_size, 0);
    }

    #[test]
    fn test_debug_result_to_json_success() {
        let result = DebugResult {
            output: Some("test".to_string()),
            error: None,
            time_us: 1000,
            input_size: 10,
            output_size: 4,
        };

        let json = debug_result_to_json(&result);
        assert_eq!(json["output"], "test");
        assert!(json["error"].is_null());
        assert_eq!(json["time_us"], 1000);
    }

    #[test]
    fn test_debug_result_to_json_error() {
        let result = DebugResult {
            output: None,
            error: Some("bad input".to_string()),
            time_us: 500,
            input_size: 5,
            output_size: 0,
        };

        let json = debug_result_to_json(&result);
        assert!(json["output"].is_null());
        assert_eq!(json["error"], "bad input");
    }
}
