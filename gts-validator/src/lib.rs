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
//! use gts_validator::{validate_fs, FsSourceConfig, ValidationConfig};
//!
//! let mut fs_config = FsSourceConfig::default();
//! fs_config.paths = vec![PathBuf::from("docs"), PathBuf::from("modules")];
//! fs_config.exclude = vec!["target/*".to_owned()];
//!
//! let mut validation_config = ValidationConfig::default();
//! validation_config.vendor = Some("x".to_owned());
//!
//! let report = validate_fs(&fs_config, &validation_config).unwrap();
//! println!("Files scanned: {}", report.files_scanned);
//! println!("Errors: {}", report.errors_count);
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

pub use config::{FsSourceConfig, ValidationConfig};
pub use error::ValidationError;
pub use report::ValidationReport;

use strategy::ContentFormat;
use strategy::fs::{content_format_for, find_files, read_validation_item};

/// Validate GTS identifiers in files on disk.
///
/// This is the primary public API for Phase 1.
///
/// # Arguments
///
/// * `fs_config` - Filesystem-specific source options (paths, exclude, max file size, etc.)
/// * `validation_config` - Core validation config (vendor, `scan_keys`, strict)
///
/// # Errors
///
/// Returns an error if `fs_config.paths` is empty or if any provided path does not exist.
/// Returns `Ok` with `files_scanned: 0` if paths exist but contain no scannable files.
pub fn validate_fs(
    fs_config: &FsSourceConfig,
    validation_config: &ValidationConfig,
) -> anyhow::Result<ValidationReport> {
    if fs_config.paths.is_empty() {
        anyhow::bail!("No paths provided for validation");
    }

    // Validate explicitly provided paths exist
    for path in &fs_config.paths {
        if !path.exists() {
            anyhow::bail!("Path does not exist: {}", path.display());
        }
    }

    let files = find_files(fs_config);

    if files.is_empty() {
        return Ok(ValidationReport {
            files_scanned: 0,
            errors_count: 0,
            ok: true,
            errors: vec![],
        });
    }

    let mut errors = Vec::new();
    let vendor = validation_config.vendor.as_deref();
    let mut files_scanned: usize = 0;

    for file_path in &files {
        let Some(item) = read_validation_item(file_path, fs_config.max_file_size) else {
            continue; // skip unreadable/oversized files — don't count as scanned
        };

        let file_errors = match content_format_for(file_path) {
            Some(ContentFormat::Markdown) => format::markdown::scan_markdown_content(
                &item.content,
                file_path,
                vendor,
                validation_config.strict,
                &validation_config.skip_tokens,
            ),
            Some(ContentFormat::Json) => format::json::scan_json_content(
                &item.content,
                file_path,
                vendor,
                validation_config.scan_keys,
            ),
            Some(ContentFormat::Yaml) => format::yaml::scan_yaml_content(
                &item.content,
                file_path,
                vendor,
                validation_config.scan_keys,
            ),
            None => continue,
        };

        files_scanned += 1;
        errors.extend(file_errors);
    }

    let errors_count = errors.len();
    Ok(ValidationReport {
        files_scanned,
        errors_count,
        ok: errors.is_empty(),
        errors,
    })
}
