//! Validation report types.

use serde::Serialize;

use crate::error::{ScanError, ValidationError};

/// Result of a validation run.
///
/// CI pipelines must check both `validation_errors` and `scan_errors`.
/// A non-empty `scan_errors` means the validator did not fully run —
/// treat this as a build failure regardless of `validation_errors`.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ValidationReport {
    /// Number of files successfully scanned (read + parsed).
    pub scanned_files: usize,
    /// Number of files that could not be scanned (read/parse failures).
    pub failed_files: usize,
    /// Whether all scanned files passed validation AND no scan errors occurred.
    pub ok: bool,
    /// Individual GTS ID validation errors found in scanned files.
    pub validation_errors: Vec<ValidationError>,
    /// Scan-level errors: files that could not be read or parsed.
    /// Non-empty means the validator did not fully cover the repository.
    pub scan_errors: Vec<ScanError>,
}

impl ValidationReport {
    /// Total number of files attempted (scanned + failed).
    #[must_use]
    pub fn files_attempted(&self) -> usize {
        self.scanned_files + self.failed_files
    }

    /// Number of validation errors found.
    #[must_use]
    pub fn errors_count(&self) -> usize {
        self.validation_errors.len()
    }
}
