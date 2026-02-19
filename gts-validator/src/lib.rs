//! # gts-validator
//!
//! GTS identifier validator for documentation and configuration files.
//!
//! This crate provides a clean separation between the **core validation engine**
//! (input-agnostic) and **input strategies** (starting with filesystem scanning).
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use std::path::PathBuf;
//! use gts_validator::{validate_fs, FsSourceConfig, ValidationConfig, VendorPolicy};
//!
//! let mut fs_config = FsSourceConfig::default();
//! fs_config.paths = vec![PathBuf::from("docs"), PathBuf::from("modules")];
//! fs_config.exclude = vec!["target/*".to_owned()];
//!
//! let mut validation_config = ValidationConfig::default();
//! validation_config.vendor_policy = VendorPolicy::MustMatch("x".to_owned());
//!
//! let report = validate_fs(&fs_config, &validation_config).unwrap();
//! println!("Files scanned: {}", report.scanned_files);
//! println!("Validation errors: {}", report.errors_count());
//! println!("Scan errors: {}", report.scan_errors.len());
//! println!("OK: {}", report.ok);
//! ```

mod config;
mod error;
mod format;
mod normalize;
pub mod output;
mod report;
mod strategy;
mod validator;

pub use config::{DiscoveryMode, FsSourceConfig, ValidationConfig, VendorPolicy};
pub use error::{ScanError, ScanErrorKind, ValidationError};
pub use report::ValidationReport;

use strategy::ContentFormat;
use strategy::fs::{ScanResult, content_format_for, find_files, read_file_bounded};

/// Validate GTS identifiers in files on disk.
///
/// This is the primary public API.
///
/// # Arguments
///
/// * `fs_config` - Filesystem-specific source options (paths, exclude, max file size, limits)
/// * `validation_config` - Core validation config (vendor policy, `scan_keys`, discovery mode)
///
/// # Errors
///
/// Returns an error if `fs_config.paths` is empty or if any provided path does not exist.
/// Returns `Ok` with `scanned_files: 0` if paths exist but contain no scannable files.
/// Scan failures (unreadable files, parse errors, etc.) are reported in `report.scan_errors`
/// and never silently discarded.
pub fn validate_fs(
    fs_config: &FsSourceConfig,
    validation_config: &ValidationConfig,
) -> anyhow::Result<ValidationReport> {
    if fs_config.paths.is_empty() {
        anyhow::bail!("No paths provided for validation");
    }

    for path in &fs_config.paths {
        if !path.exists() {
            anyhow::bail!("Path does not exist: {}", path.display());
        }
    }

    let (files, mut scan_errors) = find_files(fs_config);

    if files.is_empty() && scan_errors.is_empty() {
        return Ok(ValidationReport {
            scanned_files: 0,
            failed_files: 0,
            ok: true,
            validation_errors: vec![],
            scan_errors: vec![],
        });
    }

    let heuristic = validation_config.discovery_mode == DiscoveryMode::Heuristic;
    // For AllowList, pass a sentinel vendor that no real GTS ID can match.
    // This causes validate_candidate to emit "Vendor mismatch" for every non-example
    // vendor, and apply_allow_list_filter then removes the allowed ones — leaving only
    // genuinely disallowed vendors as errors.
    let effective_vendor = effective_vendor_for_scanning(&validation_config.vendor_policy);

    let mut validation_errors = Vec::new();
    let mut scanned_files: usize = 0;
    // Discovery-stage failures (walk errors, boundary violations, canonicalization errors)
    // are already in scan_errors from find_files. Count them as failed files upfront.
    let mut failed_files: usize = scan_errors.len();
    let mut total_bytes: u64 = 0;

    'files: for file_path in &files {
        if scanned_files + failed_files >= fs_config.max_files {
            scan_errors.push(ScanError {
                file: file_path.clone(),
                kind: ScanErrorKind::LimitExceeded,
                message: format!(
                    "Scan aborted: max_files limit ({}) reached; remaining files not scanned",
                    fs_config.max_files
                ),
            });
            failed_files += 1;
            break;
        }

        let content = match read_file_bounded(file_path, fs_config.max_file_size) {
            ScanResult::Ok(c) => c,
            ScanResult::Err(e) => {
                scan_errors.push(e);
                failed_files += 1;
                continue;
            }
        };

        let file_bytes = content.len() as u64;
        if total_bytes.saturating_add(file_bytes) > fs_config.max_total_bytes {
            scan_errors.push(ScanError {
                file: file_path.clone(),
                kind: ScanErrorKind::LimitExceeded,
                message: format!(
                    "Scan aborted: max_total_bytes limit ({}) reached; remaining files not scanned",
                    fs_config.max_total_bytes
                ),
            });
            failed_files += 1;
            break;
        }
        total_bytes = total_bytes.saturating_add(file_bytes);

        let vendor = effective_vendor.as_deref();
        let file_errors = match content_format_for(file_path) {
            Some(ContentFormat::Markdown) => format::markdown::scan_markdown_content(
                &content,
                file_path,
                vendor,
                heuristic,
                &validation_config.skip_tokens,
            ),
            Some(ContentFormat::Json) => {
                match format::json::scan_json_content(
                    &content,
                    file_path,
                    vendor,
                    validation_config.scan_keys,
                ) {
                    Ok(errs) => errs,
                    Err(scan_err) => {
                        scan_errors.push(scan_err);
                        failed_files += 1;
                        continue 'files;
                    }
                }
            }
            Some(ContentFormat::Yaml) => {
                let (val_errs, yaml_scan_errs) = format::yaml::scan_yaml_content(
                    &content,
                    file_path,
                    vendor,
                    validation_config.scan_keys,
                );
                if !yaml_scan_errs.is_empty() {
                    failed_files += 1;
                    scan_errors.extend(yaml_scan_errs);
                }
                val_errs
            }
            None => continue,
        };

        scanned_files += 1;

        // For AllowList: filter out errors where the vendor IS in the allow list.
        // The sentinel vendor caused mismatches for all vendors; remove the allowed ones.
        let file_errors = apply_allow_list_filter(file_errors, &validation_config.vendor_policy);
        validation_errors.extend(file_errors);
    }

    let ok = validation_errors.is_empty() && scan_errors.is_empty();
    Ok(ValidationReport {
        scanned_files,
        failed_files,
        ok,
        validation_errors,
        scan_errors,
    })
}

/// Determine the effective vendor string to pass to scanners for a given policy.
///
/// - `Any` → `None` (no vendor enforcement).
/// - `MustMatch(v)` → `Some(v)` (scanner enforces exact match directly).
/// - `AllowList(_)` → `Some("\x00")` (sentinel that no real GTS vendor can match).
///   GTS vendors must be lowercase alphanumeric, so `\x00` is guaranteed to never
///   equal any real vendor. This causes `validate_candidate` to emit "Vendor mismatch"
///   for every non-example vendor, and `apply_allow_list_filter` then removes the
///   vendors that are in the allow list — leaving only genuinely disallowed vendors.
fn effective_vendor_for_scanning(policy: &VendorPolicy) -> Option<String> {
    match policy {
        VendorPolicy::Any => None,
        VendorPolicy::MustMatch(v) => Some(v.clone()),
        VendorPolicy::AllowList(_) => Some("\x00".to_owned()),
    }
}

/// For `VendorPolicy::AllowList`, remove validation errors whose vendor IS in the list.
///
/// Scanners run with a sentinel vendor (`\x00`) that generates "Vendor mismatch" for
/// every non-example vendor. This function retains only errors where the vendor is NOT
/// in the allow list — i.e., genuinely disallowed vendors produce errors.
fn apply_allow_list_filter(
    errors: Vec<ValidationError>,
    policy: &VendorPolicy,
) -> Vec<ValidationError> {
    let VendorPolicy::AllowList(allowed) = policy else {
        return errors;
    };

    errors
        .into_iter()
        .filter(|e| {
            // Keep the error only if it is NOT a vendor-mismatch for an allowed vendor.
            // Vendor-mismatch errors contain "Vendor mismatch" in the message.
            // Extract the actual vendor from normalized_id (first segment before '.').
            if !e.error.contains("Vendor mismatch") {
                return true; // non-vendor errors always kept
            }
            // normalized_id format: "gts.<vendor>.<rest>..."
            // The vendor is the second dot-separated segment (index 1).
            let id_vendor = e.normalized_id.split('.').nth(1).unwrap_or("");
            !allowed.iter().any(|a| a == id_vendor)
        })
        .collect()
}
