use std::fs;
use std::process::Command;

use tempfile::TempDir;

fn validator_bin() -> &'static str {
    env!("CARGO_BIN_EXE_gts-validator")
}

#[test]
fn cli_help_works() {
    let output = Command::new(validator_bin())
        .arg("--help")
        .output()
        .expect("failed to execute gts-validator --help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert!(stdout.contains("Usage: gts-validator"), "stdout: {stdout}");
    assert!(stdout.contains("--vendor"));
}

#[test]
fn cli_json_success_output() {
    let tmp = TempDir::new().expect("temp dir");
    let md = tmp.path().join("test.md");
    fs::write(&md, "# Title\n\nUses `gts.x.core.pkg.mytype.v1~` schema.\n")
        .expect("write markdown");

    let output = Command::new(validator_bin())
        .arg("--json")
        .arg(tmp.path())
        .output()
        .expect("failed to run gts-validator");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"ok\": true"), "stdout: {stdout}");
    assert!(stdout.contains("\"scanned_files\": 1"), "stdout: {stdout}");
}

#[test]
fn cli_scan_keys_flag_enables_key_validation() {
    let tmp = TempDir::new().expect("temp dir");
    let json_file = tmp.path().join("keys.json");
    fs::write(&json_file, r#"{"gts.y.core.pkg.badtype.v1~": "value"}"#).expect("write json");

    let without_scan_keys = Command::new(validator_bin())
        .arg("--json")
        .arg("--vendor")
        .arg("x")
        .arg(tmp.path())
        .output()
        .expect("failed to run gts-validator without --scan-keys");
    assert!(without_scan_keys.status.success());

    let with_scan_keys = Command::new(validator_bin())
        .arg("--json")
        .arg("--scan-keys")
        .arg("--vendor")
        .arg("x")
        .arg(tmp.path())
        .output()
        .expect("failed to run gts-validator with --scan-keys");

    assert!(!with_scan_keys.status.success());
    let stdout = String::from_utf8(with_scan_keys.stdout).expect("stdout utf-8");
    assert!(stdout.contains("Vendor mismatch"), "stdout: {stdout}");
}

#[test]
fn cli_skip_token_suppresses_markdown_token_matches() {
    let tmp = TempDir::new().expect("temp dir");
    let md = tmp.path().join("bdd.md");
    fs::write(
        &md,
        "# BDD\n\n**given** gts.y.core.pkg.mytype.v1~ is registered\n",
    )
    .expect("write markdown");

    let without_skip = Command::new(validator_bin())
        .arg("--vendor")
        .arg("x")
        .arg(tmp.path())
        .output()
        .expect("failed to run gts-validator without --skip-token");
    assert!(!without_skip.status.success());

    let with_skip = Command::new(validator_bin())
        .arg("--vendor")
        .arg("x")
        .arg("--skip-token")
        .arg("**given**")
        .arg(tmp.path())
        .output()
        .expect("failed to run gts-validator with --skip-token");
    assert!(with_skip.status.success());
}

#[test]
fn cli_strict_mode_catches_malformed_gts_tokens() {
    let tmp = TempDir::new().expect("temp dir");
    let md = tmp.path().join("strict.md");
    fs::write(
        &md,
        "# Strict\n\nMalformed token: gts.my-vendor.core.events.type.v1~\n",
    )
    .expect("write markdown");

    let without_strict = Command::new(validator_bin())
        .arg("--json")
        .arg(tmp.path())
        .output()
        .expect("failed to run gts-validator without --strict");
    assert!(without_strict.status.success());

    let with_strict = Command::new(validator_bin())
        .arg("--json")
        .arg("--strict")
        .arg(tmp.path())
        .output()
        .expect("failed to run gts-validator with --strict");

    assert!(!with_strict.status.success());
    let stdout = String::from_utf8(with_strict.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"ok\": false"), "stdout: {stdout}");
}

#[test]
fn cli_vendor_mismatch_returns_failure() {
    let tmp = TempDir::new().expect("temp dir");
    let md = tmp.path().join("test.md");
    fs::write(&md, "# Title\n\nUses `gts.y.core.pkg.mytype.v1~` schema.\n")
        .expect("write markdown");

    let output = Command::new(validator_bin())
        .arg("--vendor")
        .arg("x")
        .arg(tmp.path())
        .output()
        .expect("failed to run gts-validator");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(stdout.contains("VALIDATION ERRORS"), "stdout: {stdout}");
    assert!(stdout.contains("Vendor mismatch"), "stdout: {stdout}");
}

#[test]
fn cli_uses_default_paths_when_none_provided() {
    let tmp = TempDir::new().expect("temp dir");
    let docs = tmp.path().join("docs");
    fs::create_dir_all(&docs).expect("create docs dir");
    fs::write(
        docs.join("sample.md"),
        "# Doc\n\nSchema: gts.x.core.pkg.mytype.v1~\n",
    )
    .expect("write sample markdown");

    let output = Command::new(validator_bin())
        .arg("--json")
        .current_dir(tmp.path())
        .output()
        .expect("failed to run gts-validator");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"ok\": true"), "stdout: {stdout}");
    assert!(stdout.contains("\"scanned_files\": 1"), "stdout: {stdout}");
}
