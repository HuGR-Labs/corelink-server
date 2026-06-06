//! Shared output formatter for `corelink-cli` (WI-S15-001).
//!
//! All subcommand handlers receive an [`OutputFormat`] and call
//! [`Formatter::emit`] to produce either a human-readable table or
//! machine-parseable JSON.

use std::fmt;

use serde::Serialize;

/// Output format discriminant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
#[non_exhaustive]
pub enum OutputFormat {
    /// Human-readable text table (default).
    #[default]
    Text,
    /// Machine-parseable JSON array / object.
    Json,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

/// Shared formatter: routes `emit` to text or JSON depending on format.
#[derive(Debug)]
pub struct Formatter {
    format: OutputFormat,
}

impl Formatter {
    /// Create a new formatter with the given output format.
    #[must_use]
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// Emit a serialisable value as JSON or as a pretty debug dump.
    ///
    /// JSON: prints the serialised form followed by a newline.
    /// Text: prints `display` followed by a newline.
    pub fn emit<T: Serialize + fmt::Display>(&self, value: &T) -> Result<(), serde_json::Error> {
        match self.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(value)?;
                println!("{json}");
            }
            OutputFormat::Text => {
                println!("{value}");
            }
        }
        Ok(())
    }

    /// Emit raw JSON value directly (already serialised).
    pub fn emit_json_value(&self, value: &serde_json::Value) -> Result<(), serde_json::Error> {
        match self.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(value)?;
                println!("{json}");
            }
            OutputFormat::Text => {
                // For text mode, print a simplified view.
                println!("{value}");
            }
        }
        Ok(())
    }

    /// Returns the current format.
    #[must_use]
    #[allow(dead_code)]
    pub fn format(&self) -> OutputFormat {
        self.format
    }
}

#[cfg(test)]
#[allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Sample {
        key: String,
        value: u32,
    }

    impl fmt::Display for Sample {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}: {}", self.key, self.value)
        }
    }

    #[test]
    fn json_format_serialises() {
        let s = Sample {
            key: "x".into(),
            value: 42,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"key\""));
        assert!(json.contains("42"));
    }

    #[test]
    fn text_format_display() {
        let s = Sample {
            key: "hello".into(),
            value: 7,
        };
        let display = format!("{s}");
        assert_eq!(display, "hello: 7");
    }

    #[test]
    fn output_format_default_is_text() {
        assert_eq!(OutputFormat::default(), OutputFormat::Text);
    }
}
