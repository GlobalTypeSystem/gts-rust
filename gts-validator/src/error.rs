//! Error types for GTS validation.

use std::path::PathBuf;

use serde::Serialize;

/// The kind of scan-level failure that prevented a file from being validated.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScanErrorKind {
    /// An I/O error occurred while reading the file.
    IoError,
    /// The file exceeded the configured maximum size limit.
    FileTooLarge,
    /// The file content could not be parsed as valid JSON.
    JsonParseError,
    /// The file content could not be parsed as valid YAML.
    YamlParseError,
    /// The file content is not valid UTF-8.
    InvalidEncoding,
    /// The resolved path is outside the repository root (symlink escape).
    OutsideRepository,
    /// A resource limit (`max_files` or `max_total_bytes`) was reached, truncating the scan.
    LimitExceeded,
    /// A directory traversal error (permission denied, loop detected, etc.).
    WalkError,
    /// An exclude glob pattern could not be parsed.
    InvalidExcludePattern,
}

/// A scan-level error: a file that could not be validated at all.
///
/// These are distinct from `ValidationError` (which represents a GTS ID that
/// was found and failed validation). A `ScanError` means the file could not
/// even be read or parsed — CI must treat these as failures.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScanError {
    /// The file path that could not be scanned.
    pub file: PathBuf,
    /// The kind of failure.
    pub kind: ScanErrorKind,
    /// Human-readable description of the failure.
    pub message: String,
}

impl ScanError {
    /// Format the error for human-readable output.
    #[must_use]
    pub fn format_human_readable(&self) -> String {
        format!("{}: [scan error] {}", self.file.display(), self.message)
    }
}

/// A single validation error found in a documentation/config file.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValidationError {
    /// File path where the error was found
    pub file: PathBuf,
    /// Line number (1-indexed) — for .md files; 0 for structured files
    pub line: usize,
    /// Column number (1-indexed) — for .md files; 0 for structured files
    pub column: usize,
    /// JSON path (e.g., "$.properties.type.x-gts-ref") — for .json/.yaml files; empty for .md
    pub json_path: String,
    /// The original raw string that was found
    pub raw_value: String,
    /// The normalized GTS identifier (after stripping gts://, etc.)
    pub normalized_id: String,
    /// Human-readable error description
    pub error: String,
    /// Surrounding context (for .md: the line content; for .json/.yaml: the parent key)
    pub context: String,
}

impl ValidationError {
    /// Format the error for human-readable output.
    ///
    /// For markdown errors: `{file}:{line}:{column}: {error} [{raw_value}]`
    /// For JSON/YAML errors: `{file}: {error} [{raw_value}] (at {json_path})`
    #[must_use]
    pub fn format_human_readable(&self) -> String {
        if self.line > 0 && self.column > 0 {
            // Markdown error with line/column
            format!(
                "{}:{}:{}: {} [{}]",
                self.file.display(),
                self.line,
                self.column,
                self.error,
                self.raw_value
            )
        } else if !self.json_path.is_empty() {
            // JSON/YAML error with json_path
            format!(
                "{}: {} [{}] (at {})",
                self.file.display(),
                self.error,
                self.raw_value,
                self.json_path
            )
        } else {
            // Fallback: just file and error
            format!(
                "{}: {} [{}]",
                self.file.display(),
                self.error,
                self.raw_value
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_format_markdown_error() {
        let err = ValidationError {
            file: PathBuf::from("docs/test.md"),
            line: 42,
            column: 10,
            json_path: String::new(),
            raw_value: "gts.invalid".to_owned(),
            normalized_id: "gts.invalid".to_owned(),
            error: "Invalid GTS ID".to_owned(),
            context: "Some context".to_owned(),
        };

        let formatted = err.format_human_readable();
        assert!(formatted.contains("docs/test.md:42:10"));
        assert!(formatted.contains("Invalid GTS ID"));
        assert!(formatted.contains("[gts.invalid]"));
        assert!(!formatted.contains("(at"));
    }

    #[test]
    fn test_format_json_error() {
        let err = ValidationError {
            file: PathBuf::from("config/test.json"),
            line: 0,
            column: 0,
            json_path: "$.properties.type.x-gts-ref".to_owned(),
            raw_value: "gts.invalid".to_owned(),
            normalized_id: "gts.invalid".to_owned(),
            error: "Invalid GTS ID".to_owned(),
            context: "x-gts-ref".to_owned(),
        };

        let formatted = err.format_human_readable();
        assert!(formatted.contains("config/test.json"));
        assert!(formatted.contains("Invalid GTS ID"));
        assert!(formatted.contains("[gts.invalid]"));
        assert!(formatted.contains("(at $.properties.type.x-gts-ref)"));
        assert!(!formatted.contains(":0:0"));
    }
}
