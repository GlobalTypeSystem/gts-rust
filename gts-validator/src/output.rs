//! Shared output formatting for validation reports.
//!
//! Provides JSON and plain-text formatters for `ValidationReport`.
//! Color/terminal formatting is intentionally excluded from this core module —
//! that concern belongs to the CLI layer.

use std::io::Write;

use crate::report::ValidationReport;

/// Format a `ValidationReport` as JSON to a writer.
///
/// # Errors
///
/// Returns an error if serialization or writing fails.
pub fn write_json(report: &ValidationReport, writer: &mut dyn Write) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    writeln!(writer, "{json}")?;
    Ok(())
}

/// Format a `ValidationReport` as human-readable plain text to a writer.
///
/// Color/ANSI formatting is the responsibility of the caller (CLI layer).
///
/// # Errors
///
/// Returns an error if writing fails.
pub fn write_human(report: &ValidationReport, writer: &mut dyn Write) -> anyhow::Result<()> {
    writeln!(writer)?;
    writeln!(writer, "{}", "=".repeat(80))?;
    writeln!(writer, "  GTS DOCUMENTATION VALIDATOR")?;
    writeln!(writer, "{}", "=".repeat(80))?;
    writeln!(writer)?;
    writeln!(writer, "  Files scanned:  {}", report.scanned_files)?;
    writeln!(writer, "  Files failed:   {}", report.failed_files)?;
    writeln!(writer, "  Errors found:   {}", report.errors_count())?;
    writeln!(writer)?;

    if !report.scan_errors.is_empty() {
        writeln!(writer, "{}", "-".repeat(80))?;
        writeln!(writer, "  SCAN ERRORS (files that could not be validated)")?;
        writeln!(writer, "{}", "-".repeat(80))?;
        for scan_err in &report.scan_errors {
            writeln!(writer, "{}", scan_err.format_human_readable())?;
        }
        writeln!(writer)?;
    }

    if !report.validation_errors.is_empty() {
        writeln!(writer, "{}", "-".repeat(80))?;
        writeln!(writer, "  VALIDATION ERRORS")?;
        writeln!(writer, "{}", "-".repeat(80))?;
        for error in &report.validation_errors {
            writeln!(writer, "{}", error.format_human_readable())?;
        }
        writeln!(writer)?;
    }

    writeln!(writer, "{}", "=".repeat(80))?;
    if report.ok {
        writeln!(
            writer,
            "\u{2713} All {} files passed validation",
            report.scanned_files
        )?;
    } else {
        if !report.scan_errors.is_empty() {
            writeln!(
                writer,
                "\u{2717} {} file(s) could not be scanned \u{2014} CI must treat this as a failure",
                report.failed_files
            )?;
        }
        if !report.validation_errors.is_empty() {
            writeln!(
                writer,
                "\u{2717} {} invalid GTS identifier(s) found",
                report.errors_count()
            )?;
            writeln!(writer)?;
            writeln!(writer, "  To fix:")?;

            let has_vendor_mismatch = report
                .validation_errors
                .iter()
                .any(|e| e.error.contains("Vendor mismatch"));
            let has_wildcard_error = report
                .validation_errors
                .iter()
                .any(|e| e.error.contains("Wildcard"));
            let has_parse_error = report
                .validation_errors
                .iter()
                .any(|e| !e.error.contains("Vendor mismatch") && !e.error.contains("Wildcard"));

            if has_parse_error {
                writeln!(
                    writer,
                    "    - Schema IDs must end with ~ (e.g., gts.x.core.type.v1~)"
                )?;
                writeln!(
                    writer,
                    "    - Each segment needs 5 parts: vendor.package.namespace.type.version"
                )?;
                writeln!(writer, "    - No hyphens allowed, use underscores")?;
            }
            if has_wildcard_error {
                writeln!(
                    writer,
                    "    - Wildcards (*) only in filter/pattern contexts"
                )?;
            }
            if has_vendor_mismatch {
                writeln!(writer, "    - Ensure all GTS IDs use the expected vendor")?;
            }
        }
    }
    writeln!(writer, "{}", "=".repeat(80))?;

    Ok(())
}
