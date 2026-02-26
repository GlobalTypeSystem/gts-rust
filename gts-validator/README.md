# gts-validator

GTS identifier validator for documentation and configuration files (.md, .json, .yaml).

## Overview

`gts-validator` provides both:

- a **CLI binary** (`gts-validator`) for CI and local validation, and
- a **library API** for embedding validation into Rust applications.

The crate provides a clean separation between:
- **Core validation engine** (input-agnostic): normalize → validate → report
- **Input strategies** (starting with filesystem scanning)

## CLI usage

Install:

```bash
cargo install gts-validator
```

Examples:

```bash
# Basic scan
gts-validator docs modules libs examples

# Vendor enforcement
gts-validator --vendor x docs modules

# Exclusions (repeatable)
gts-validator --exclude "target/*" --exclude "docs/api/*" docs

# Machine-readable output
gts-validator --json docs

# Strict markdown discovery mode
gts-validator --strict docs
```

If no paths are passed, the CLI scans existing default roots:
`docs`, `modules`, `libs`, `examples`.

## Library usage

```rust
use std::path::PathBuf;
use gts_validator::{validate_fs, FsSourceConfig, ValidationConfig, VendorPolicy};

let mut fs_config = FsSourceConfig::default();
fs_config.paths = vec![PathBuf::from("docs"), PathBuf::from("modules")];
fs_config.exclude = vec!["target/*".to_owned()];

let mut validation_config = ValidationConfig::default();
validation_config.vendor_policy = VendorPolicy::MustMatch("x".to_owned());

let report = validate_fs(&fs_config, &validation_config).unwrap();
println!("Files scanned: {}", report.scanned_files);
println!("Errors: {}", report.errors_count());
println!("OK: {}", report.ok);
```

## Output Formatting

The crate includes output formatters for rendering validation reports:

```rust
use gts_validator::output;

// JSON output
let mut stdout = std::io::stdout();
output::write_json(&report, &mut stdout).unwrap();

// Human-readable output
output::write_human(&report, &mut stdout).unwrap();
```

## License

Apache-2.0
