//! Shared output formatting for validation reports.
//!
//! Provides JSON and human-readable formatters for `ValidationReport`.

use std::io::Write;

use colored::Colorize;

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

/// Format a `ValidationReport` as human-readable text to a writer.
///
/// # Errors
///
/// Returns an error if writing fails.
pub fn write_human(
    report: &ValidationReport,
    writer: &mut dyn Write,
    use_color: bool,
) -> anyhow::Result<()> {
    writeln!(writer)?;
    writeln!(writer, "{}", "=".repeat(80))?;
    if use_color {
        writeln!(writer, "  {}", "GTS DOCUMENTATION VALIDATOR".bold())?;
    } else {
        writeln!(writer, "  GTS DOCUMENTATION VALIDATOR")?;
    }
    writeln!(writer, "{}", "=".repeat(80))?;
    writeln!(writer)?;
    writeln!(writer, "  Files scanned: {}", report.files_scanned)?;
    writeln!(writer, "  Errors found:  {}", report.errors_count)?;
    writeln!(writer)?;

    if !report.errors.is_empty() {
        writeln!(writer, "{}", "-".repeat(80))?;
        if use_color {
            writeln!(writer, "  {}", "ERRORS".red().bold())?;
        } else {
            writeln!(writer, "  ERRORS")?;
        }
        writeln!(writer, "{}", "-".repeat(80))?;

        // Print errors
        for error in &report.errors {
            let formatted = error.format_human_readable();
            if use_color {
                writeln!(writer, "{}", formatted.red())?;
            } else {
                writeln!(writer, "{formatted}")?;
            }
        }
        writeln!(writer)?;
    }

    writeln!(writer, "{}", "=".repeat(80))?;
    if report.ok {
        let msg = format!(
            "\u{2713} All {} files passed validation",
            report.files_scanned
        );
        if use_color {
            writeln!(writer, "{}", msg.green())?;
        } else {
            writeln!(writer, "{msg}")?;
        }
    } else {
        let msg = format!(
            "\u{2717} {} invalid GTS identifiers found",
            report.errors_count
        );
        if use_color {
            writeln!(writer, "{}", msg.red())?;
        } else {
            writeln!(writer, "{msg}")?;
        }
        writeln!(writer)?;
        writeln!(writer, "  To fix:")?;

        // Only show hints relevant to the actual errors found
        let has_vendor_mismatch = report
            .errors
            .iter()
            .any(|e| e.error.contains("Vendor mismatch"));
        let has_wildcard_error = report.errors.iter().any(|e| e.error.contains("Wildcard"));
        let has_parse_error = report
            .errors
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
    writeln!(writer, "{}", "=".repeat(80))?;

    Ok(())
}
