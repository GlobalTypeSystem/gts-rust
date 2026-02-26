// These Clippy lints are disabled because this is a CLI binary:
// - print_stdout/print_stderr: CLI tools are expected to print to stdout/stderr.
// - exit/expect_used: process-level exit and explicit failure in CLI contexts are acceptable.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::exit,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use gts_validator::output;
use gts_validator::{DiscoveryMode, FsSourceConfig, ValidationConfig, VendorPolicy};

/// GTS Documentation Validator (DE0903)
///
/// Validates GTS identifiers in .md/.json/.yaml/.yml files.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    /// Paths to scan (files or directories)
    /// Defaults to: docs, modules, libs, examples
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,

    /// Expected vendor for all GTS IDs (validates vendor matches)
    #[arg(long)]
    vendor: Option<String>,

    /// Exclude patterns (can be specified multiple times)
    #[arg(long, short = 'e', action = clap::ArgAction::Append)]
    exclude: Vec<String>,

    /// Output results as JSON
    #[arg(long)]
    json: bool,

    /// Show verbose output including file scanning progress
    #[arg(long, short = 'v')]
    verbose: bool,

    /// Maximum file size in bytes (default: 10 MB)
    #[arg(long, default_value = "10485760")]
    max_file_size: u64,

    /// Scan JSON/YAML object keys for GTS identifiers (default: off)
    #[arg(long)]
    scan_keys: bool,

    /// Strict mode: catches ALL gts.* strings including malformed IDs.
    #[arg(long)]
    strict: bool,

    /// Skip tokens for markdown scanning (repeatable)
    #[arg(long = "skip-token", action = clap::ArgAction::Append)]
    skip_tokens: Vec<String>,
}

/// Default directories to scan if no paths are provided.
const DEFAULT_SCAN_DIRS: &[&str] = &["docs", "modules", "libs", "examples"];

fn main() -> ExitCode {
    let cli = Cli::parse();

    let paths: Vec<PathBuf> = if cli.paths.is_empty() {
        DEFAULT_SCAN_DIRS
            .iter()
            .map(PathBuf::from)
            .filter(|path| path.exists())
            .collect()
    } else {
        cli.paths
    };

    if paths.is_empty() {
        eprintln!("No existing paths to scan. Provide paths explicitly.");
        return ExitCode::FAILURE;
    }

    let mut fs_config = FsSourceConfig::default();
    fs_config.paths = paths;
    fs_config.exclude = cli.exclude;
    fs_config.max_file_size = cli.max_file_size;

    let mut validation_config = ValidationConfig::default();
    validation_config.scan_keys = cli.scan_keys;
    validation_config.discovery_mode = if cli.strict {
        DiscoveryMode::Heuristic
    } else {
        DiscoveryMode::StrictSpecOnly
    };
    validation_config.skip_tokens = cli.skip_tokens;

    validation_config.vendor_policy = match cli.vendor {
        Some(vendor) => VendorPolicy::MustMatch(vendor),
        None => VendorPolicy::Any,
    };

    if cli.verbose {
        let path_list: Vec<String> = fs_config
            .paths
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        eprintln!("Scanning paths: {}", path_list.join(", "));

        if let VendorPolicy::MustMatch(ref vendor) = validation_config.vendor_policy {
            eprintln!("Expected vendor: {vendor}");
        }
    }

    let report = match gts_validator::validate_fs(&fs_config, &validation_config) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::FAILURE;
        }
    };

    if cli.verbose {
        eprintln!("Scanned {} files", report.scanned_files);
    }

    let mut stdout = std::io::stdout();
    let result = if cli.json {
        output::write_json(&report, &mut stdout)
    } else {
        output::write_human(&report, &mut stdout)
    };

    if let Err(error) = result {
        eprintln!("Error writing output: {error}");
        return ExitCode::FAILURE;
    }

    if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
