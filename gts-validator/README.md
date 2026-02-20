# gts-validator

GTS identifier validator for documentation and configuration files (.md, .json, .yaml).

## Overview

`gts-validator` provides a library for validating GTS (Global Type System) identifiers found in documentation and configuration files.

The crate provides a clean separation between:
- **Core validation engine** (input-agnostic): normalize → validate → report
- **Input strategies** (starting with filesystem scanning)

## Usage

```rust
use std::path::PathBuf;
use gts_validator::{validate_fs, FsSourceConfig, ValidationConfig};

let mut fs_config = FsSourceConfig::default();
fs_config.paths = vec![PathBuf::from("docs"), PathBuf::from("modules")];
fs_config.exclude = vec!["target/*".to_owned()];

let mut validation_config = ValidationConfig::default();
validation_config.vendor = Some("x".to_owned());

let report = validate_fs(&fs_config, &validation_config).unwrap();
println!("Files scanned: {}", report.files_scanned);
println!("Errors: {}", report.errors_count);
println!("OK: {}", report.ok);
```

## Output Formatting

The crate includes output formatters for rendering validation reports:

```rust
use gts_validator::output;

// JSON output
let mut stdout = std::io::stdout();
output::write_json(&report, &mut stdout).unwrap();

// Human-readable output (with color support)
output::write_human(&report, &mut stdout, true).unwrap();
```

## License

Apache-2.0
