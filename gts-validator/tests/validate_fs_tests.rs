//! Integration tests for `gts_validator::validate_fs`.

use std::fs;
use std::path::PathBuf;

use gts_validator::{FsSourceConfig, ValidationConfig, VendorPolicy, validate_fs};
use tempfile::TempDir;

fn default_validation_config() -> ValidationConfig {
    ValidationConfig::default()
}

fn default_fs_config(paths: Vec<PathBuf>) -> FsSourceConfig {
    let mut cfg = FsSourceConfig::default();
    cfg.paths = paths;
    cfg
}

#[test]
fn test_validate_fs_empty_paths_errors() {
    let fs_config = default_fs_config(vec![]);
    let result = validate_fs(&fs_config, &default_validation_config());
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("No paths provided"), "got: {msg}");
}

#[test]
fn test_validate_fs_nonexistent_path_errors() {
    let tmp = TempDir::new().unwrap();
    let nonexistent = tmp.path().join("does_not_exist");
    let fs_config = default_fs_config(vec![nonexistent]);
    let result = validate_fs(&fs_config, &default_validation_config());
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("does not exist"), "got: {msg}");
}

#[test]
fn test_validate_fs_valid_markdown() {
    let tmp = TempDir::new().unwrap();
    let md = tmp.path().join("test.md");
    fs::write(&md, "# Title\n\nUses `gts.x.core.pkg.mytype.v1~` schema.\n").unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();

    assert_eq!(report.scanned_files, 1);
    assert!(
        report.ok,
        "expected ok, got errors: {:?}",
        report.validation_errors
    );
    assert_eq!(report.errors_count(), 0);
}

#[test]
fn test_validate_fs_vendor_mismatch() {
    let tmp = TempDir::new().unwrap();
    let md = tmp.path().join("test.md");
    // Valid structure but wrong vendor — triggers vendor mismatch error
    fs::write(&md, "# Title\n\nUses `gts.y.core.pkg.mytype.v1~` schema.\n").unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let mut config = ValidationConfig::default();
    config.vendor_policy = VendorPolicy::MustMatch("x".to_owned());
    let report = validate_fs(&fs_config, &config).unwrap();

    assert_eq!(report.scanned_files, 1);
    assert!(!report.ok);
    assert!(report.errors_count() > 0);
    assert!(
        report
            .validation_errors
            .iter()
            .any(|e| e.error.contains("Vendor mismatch")),
        "expected vendor mismatch error, got: {:?}",
        report.validation_errors
    );
}

#[test]
fn test_validate_fs_valid_json() {
    let tmp = TempDir::new().unwrap();
    let json_file = tmp.path().join("test.json");
    fs::write(
        &json_file,
        r#"{"schema": "gts.x.core.pkg.mytype.v1~", "name": "test"}"#,
    )
    .unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();

    assert_eq!(report.scanned_files, 1);
    assert!(
        report.ok,
        "expected ok, got errors: {:?}",
        report.validation_errors
    );
}

#[test]
fn test_validate_fs_valid_yaml() {
    let tmp = TempDir::new().unwrap();
    let yaml_file = tmp.path().join("test.yaml");
    fs::write(
        &yaml_file,
        "schema: gts.x.core.pkg.mytype.v1~\nname: test\n",
    )
    .unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();

    assert_eq!(report.scanned_files, 1);
    assert!(
        report.ok,
        "expected ok, got errors: {:?}",
        report.validation_errors
    );
}

#[test]
fn test_validate_fs_json_output_contract() {
    let tmp = TempDir::new().unwrap();
    let md = tmp.path().join("test.md");
    fs::write(&md, "# Title\n\nUses `gts.x.core.pkg.mytype.v1~` schema.\n").unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();

    // Verify JSON serialization contract
    let mut buf = Vec::new();
    gts_validator::output::write_json(&report, &mut buf).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();

    assert!(json.get("scanned_files").is_some());
    assert!(json.get("failed_files").is_some());
    assert!(json.get("ok").is_some());
    assert!(json.get("validation_errors").is_some());
    assert!(json.get("scan_errors").is_some());
    assert!(json["ok"].as_bool().unwrap());
}

#[test]
fn test_validate_fs_exclude_pattern() {
    let tmp = TempDir::new().unwrap();

    // Create an included file (valid) and an excluded file (would fail vendor check)
    let included = tmp.path().join("included.md");
    fs::write(
        &included,
        "# Title\n\nUses `gts.x.core.pkg.mytype.v1~` schema.\n",
    )
    .unwrap();

    let excluded_dir = tmp.path().join("excluded");
    fs::create_dir(&excluded_dir).unwrap();
    let excluded_md = excluded_dir.join("test.md");
    fs::write(
        &excluded_md,
        "# Title\n\nUses `gts.y.core.pkg.mytype.v1~` schema.\n",
    )
    .unwrap();

    let mut config = ValidationConfig::default();
    config.vendor_policy = VendorPolicy::MustMatch("x".to_owned());

    // Without exclude: should find vendor mismatch in excluded/test.md
    let mut fs_config_no_exclude = FsSourceConfig::default();
    fs_config_no_exclude.paths = vec![tmp.path().to_path_buf()];
    let report_no_exclude = validate_fs(&fs_config_no_exclude, &config).unwrap();
    assert_eq!(report_no_exclude.scanned_files, 2);
    assert!(
        !report_no_exclude.ok,
        "should find vendor mismatch without exclude"
    );

    // With exclude: file matching "test.md" should be skipped, only included.md scanned
    let mut fs_config_with_exclude = FsSourceConfig::default();
    fs_config_with_exclude.paths = vec![tmp.path().to_path_buf()];
    fs_config_with_exclude.exclude = vec!["test.md".to_owned()];
    let report_with_exclude = validate_fs(&fs_config_with_exclude, &config).unwrap();
    assert_eq!(
        report_with_exclude.scanned_files, 1,
        "exclude should reduce file count"
    );
    assert!(
        report_with_exclude.ok,
        "only included.md (valid vendor) should remain"
    );
}

#[test]
fn test_write_human_success_output() {
    let tmp = TempDir::new().unwrap();
    let md = tmp.path().join("test.md");
    fs::write(&md, "# Title\n\nUses `gts.x.core.pkg.mytype.v1~` schema.\n").unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();

    let mut buf = Vec::new();
    gts_validator::output::write_human(&report, &mut buf).unwrap();
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("GTS DOCUMENTATION VALIDATOR"),
        "missing header, got: {output}"
    );
    assert!(output.contains("Files scanned:  1"), "missing file count");
    assert!(output.contains("Errors found:   0"), "missing error count");
    assert!(
        output.contains("All 1 files passed"),
        "missing success message"
    );
    assert!(
        !output.contains("VALIDATION ERRORS"),
        "should not contain VALIDATION ERRORS section"
    );
}

#[test]
fn test_write_human_failure_output() {
    let tmp = TempDir::new().unwrap();
    let md = tmp.path().join("test.md");
    fs::write(&md, "# Title\n\nUses `gts.y.core.pkg.mytype.v1~` schema.\n").unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let mut config = ValidationConfig::default();
    config.vendor_policy = VendorPolicy::MustMatch("x".to_owned());
    let report = validate_fs(&fs_config, &config).unwrap();

    let mut buf = Vec::new();
    gts_validator::output::write_human(&report, &mut buf).unwrap();
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("VALIDATION ERRORS"),
        "missing VALIDATION ERRORS section"
    );
    assert!(
        output.contains("Vendor mismatch"),
        "missing vendor mismatch hint"
    );
    assert!(
        output.contains("invalid GTS identifier"),
        "missing failure summary"
    );
    assert!(
        output.contains("Ensure all GTS IDs use the expected vendor"),
        "missing vendor hint"
    );
}

#[test]
fn test_validate_fs_no_matching_files_returns_ok() {
    let tmp = TempDir::new().unwrap();
    // Create a directory with no .md/.json/.yaml files
    let txt = tmp.path().join("readme.txt");
    fs::write(&txt, "This is not a markdown file").unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();
    assert_eq!(report.scanned_files, 0);
    assert!(report.ok, "empty scan should be ok, not an error");
}

#[test]
fn test_validate_fs_max_file_size_produces_scan_error() {
    let tmp = TempDir::new().unwrap();
    let md = tmp.path().join("big.md");
    fs::write(&md, "# Title\n\nUses `gts.y.core.pkg.mytype.v1~` schema.\n").unwrap();

    // Set max_file_size to 10 bytes — file is larger, so it should produce a scan error
    let mut fs_config = FsSourceConfig::default();
    fs_config.paths = vec![tmp.path().to_path_buf()];
    fs_config.max_file_size = 10;

    let mut config = ValidationConfig::default();
    config.vendor_policy = VendorPolicy::MustMatch("x".to_owned());
    let report = validate_fs(&fs_config, &config).unwrap();

    assert_eq!(
        report.scanned_files, 0,
        "Oversized file should not be counted as scanned"
    );
    assert_eq!(
        report.failed_files, 1,
        "Oversized file must produce a scan error"
    );
    assert!(!report.ok, "Scan errors must make the report not-ok");
}

#[test]
fn test_validate_fs_non_utf8_file_produces_scan_error() {
    let tmp = TempDir::new().unwrap();
    let md = tmp.path().join("binary.md");
    fs::write(&md, [0xFF, 0xFE, 0x00, 0x01, 0x80, 0x81]).unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();

    assert_eq!(
        report.scanned_files, 0,
        "Non-UTF-8 file should not be counted as scanned"
    );
    assert_eq!(
        report.failed_files, 1,
        "Non-UTF-8 file must produce a scan error"
    );
    assert!(!report.ok, "Scan errors must make the report not-ok");
}

#[test]
fn test_validate_fs_invalid_json_produces_scan_error() {
    let tmp = TempDir::new().unwrap();
    let json_file = tmp.path().join("bad.json");
    fs::write(&json_file, "{ not valid json !!!").unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();

    assert_eq!(
        report.failed_files, 1,
        "Invalid JSON must produce a scan error"
    );
    assert!(!report.ok, "Scan errors must make the report not-ok");
}

#[test]
fn test_validate_fs_invalid_yaml_produces_scan_error() {
    let tmp = TempDir::new().unwrap();
    let yaml_file = tmp.path().join("bad.yaml");
    fs::write(&yaml_file, "key: [unclosed bracket").unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);
    let report = validate_fs(&fs_config, &default_validation_config()).unwrap();

    assert_eq!(
        report.failed_files, 1,
        "Invalid YAML must produce a scan error"
    );
    assert!(!report.ok, "Scan errors must make the report not-ok");
}

#[test]
fn test_validate_fs_skip_tokens_integration() {
    let tmp = TempDir::new().unwrap();
    let md = tmp.path().join("bdd.md");
    // BDD-style content where **given** precedes a GTS ID with wrong vendor
    fs::write(
        &md,
        "# BDD\n\n**given** gts.y.core.pkg.mytype.v1~ is registered\n",
    )
    .unwrap();

    let fs_config = default_fs_config(vec![tmp.path().to_path_buf()]);

    // Without skip_tokens: should report vendor mismatch
    let mut config_no_skip = ValidationConfig::default();
    config_no_skip.vendor_policy = VendorPolicy::MustMatch("x".to_owned());
    let report_no_skip = validate_fs(&fs_config, &config_no_skip).unwrap();
    assert!(
        !report_no_skip.ok,
        "Without skip_tokens, vendor mismatch should be reported"
    );

    // With skip_tokens: should suppress the error
    let mut config_skip = ValidationConfig::default();
    config_skip.vendor_policy = VendorPolicy::MustMatch("x".to_owned());
    config_skip.skip_tokens = vec!["**given**".to_owned()];
    let report_skip = validate_fs(&fs_config, &config_skip).unwrap();
    assert!(
        report_skip.ok,
        "With skip_tokens, the error should be suppressed: {:?}",
        report_skip.validation_errors
    );
}
