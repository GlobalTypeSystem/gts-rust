//! Validation report types.

use serde::Serialize;

use crate::error::ValidationError;

/// Result of a validation run.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ValidationReport {
    /// Number of files scanned.
    pub files_scanned: usize,
    /// Number of errors found.
    pub errors_count: usize,
    /// Whether all files passed validation.
    pub ok: bool,
    /// Individual validation errors.
    pub errors: Vec<ValidationError>,
}
