//! Compiled backend: `cargo test` runs the exporters `#[struct_to_gts_schema]`
//! emits, so schemas match `GtsSchema::gts_schema_with_refs()` exactly.

use anyhow::{Context, Result, anyhow, bail};
use gts::schema_export::{EXPORT_DIR_ENV, EXPORT_FILES_ENV, EXPORT_FORMAT, EXPORT_ROOT_ENV};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::inventory::{Decl, Inventory};

/// Test-name filter matching only the exporters.
const EXPORT_TEST_FILTER: &str = "__gts_schema_export_";

/// A selected source file of the package, by canonical path.
pub struct SourceFile {
    pub path: PathBuf,
    pub inventory: Inventory,
}

/// A schema exported for one declaration.
#[derive(Debug, Clone)]
pub struct Exported {
    pub file: PathBuf,
    pub line: usize,
    pub struct_name: String,
    pub type_id: String,
    pub dir_path: String,
    pub schema: Value,
}

#[derive(Debug, Deserialize)]
struct Record {
    format: u64,
    #[serde(rename = "struct")]
    struct_name: String,
    module: String,
    file: PathBuf,
    dir_path: String,
    type_id: String,
    schema: Value,
}

/// Exports every declaration in `files`, which all belong to the package at
/// `manifest`. Fails unless each declaration has exactly one consistent schema.
///
/// # Errors
///
/// Unexportable declarations, a failing `cargo` run (build errors or invalid
/// overrides), or declarations no exporter reported.
pub fn export_package(manifest: &Path, files: &[SourceFile]) -> Result<Vec<Exported>> {
    reject_unexportable(files)?;

    let metadata = cargo_metadata(manifest)?;
    let workspace_root = metadata
        .get("workspace_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            anyhow!(
                "`cargo metadata` for {} has no workspace_root",
                manifest.display()
            )
        })?;
    let package = find_package(&metadata, manifest)?;
    let package_name = package
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned();
    let manifest_toml: toml::Table = fs::read_to_string(manifest)?
        .parse()
        .with_context(|| format!("cannot parse {}", manifest.display()))?;
    let targets = test_targets(package, &manifest_toml);
    if targets.args.is_empty() {
        bail!(
            "package `{package_name}` ({}) has no test targets with the libtest harness to run \
             schema exporters in{}",
            manifest.display(),
            targets.skipped_note()
        );
    }

    let spool = Spool::create()?;
    let selected: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
    let mut command = cargo_in(manifest);
    command
        .arg("test")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--no-fail-fast")
        .args(&targets.args)
        .arg("--")
        .arg(EXPORT_TEST_FILTER)
        .env(EXPORT_DIR_ENV, &spool.0)
        .env(EXPORT_FILES_ENV, selected.join("\n"))
        .env(EXPORT_ROOT_ENV, &workspace_root)
        .stdin(Stdio::null());
    println!(
        "  Exporting compiled schemas of package `{package_name}`: cargo test --manifest-path {} {} -- {EXPORT_TEST_FILTER}",
        manifest.display(),
        targets.args.join(" ")
    );
    let output = command
        .output()
        .context("cannot run `cargo`; compiled schema export needs a Rust toolchain")?;
    if !output.status.success() {
        bail!(
            "schema export for package `{package_name}` failed (`cargo test` exited with {}):\n{}{}",
            output.status,
            tail(&String::from_utf8_lossy(&output.stdout), 60),
            tail(&String::from_utf8_lossy(&output.stderr), 40),
        );
    }

    let records = read_records(&spool.0)?;
    match_records(&package_name, files, records, &targets)
}

/// Declarations whose exporter can never run are reported before building.
fn reject_unexportable(files: &[SourceFile]) -> Result<()> {
    let mut problems = String::new();
    for file in files {
        for local in &file.inventory.locals {
            let _ = writeln!(
                problems,
                "  {}:{}: `{}` is declared inside a function or block; move it to module level",
                file.path.display(),
                local.line,
                local.name
            );
        }
        for decl in file.inventory.decls.iter().filter(|d| d.bounded) {
            let _ = writeln!(
                problems,
                "  {}:{}: `{}` has its own generic bounds or a where-clause, so `{}<()>` has no \
                 exporter; remove the bounds",
                file.path.display(),
                decl.line,
                decl.name,
                decl.name
            );
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        bail!(
            "these schemas cannot be exported from compiled code (exclude their files with \
             --exclude or `// gts:ignore` to skip them):\n{problems}"
        )
    }
}

/// `cargo` run from the package directory: `--manifest-path` alone does not make
/// Cargo read the package's `.cargo/config.toml` (rustflags, registries, env).
fn cargo_in(manifest: &Path) -> Command {
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    if let Some(dir) = manifest.parent() {
        command.current_dir(dir);
    }
    command
}

fn cargo_metadata(manifest: &Path) -> Result<Value> {
    let output = cargo_in(manifest)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(manifest)
        .stdin(Stdio::null())
        .output()
        .context("cannot run `cargo metadata`; compiled schema export needs a Rust toolchain")?;
    if !output.status.success() {
        bail!(
            "`cargo metadata` failed for {}:\n{}",
            manifest.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn find_package<'a>(metadata: &'a Value, manifest: &Path) -> Result<&'a Value> {
    metadata
        .get("packages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|p| {
            p.get("manifest_path")
                .and_then(Value::as_str)
                .and_then(|m| Path::new(m).canonicalize().ok())
                .is_some_and(|m| m == manifest)
        })
        .ok_or_else(|| {
            anyhow!(
                "`cargo metadata` does not list the package {}",
                manifest.display()
            )
        })
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Targets {
    args: Vec<String>,
    skipped: Vec<String>,
}

impl Targets {
    fn skipped_note(&self) -> String {
        if self.skipped.is_empty() {
            String::new()
        } else {
            format!(" (skipped: {})", self.skipped.join(", "))
        }
    }
}

/// Lib, bin and integration-test targets that run libtest. `harness = false`
/// targets would run their `main`; benches and examples are never selected.
/// `cargo test` builds with default features, so other required features skip.
fn test_targets(package: &Value, manifest: &toml::Table) -> Targets {
    let enabled = default_features(package);
    let mut targets = Targets::default();
    for target in package
        .get("targets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = target
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kinds: Vec<&str> = target
            .get("kind")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let (section, arg) = if kinds.contains(&"bin") {
            ("bin", vec!["--bin".to_owned(), name.to_owned()])
        } else if kinds.contains(&"test") {
            ("test", vec!["--test".to_owned(), name.to_owned()])
        } else if kinds
            .iter()
            .any(|k| matches!(*k, "lib" | "rlib" | "dylib" | "cdylib" | "staticlib"))
        {
            ("lib", vec!["--lib".to_owned()])
        } else {
            continue;
        };
        let label = format!("{section} `{name}`");
        let reason = if target.get("test").and_then(Value::as_bool) == Some(false) {
            Some("test = false")
        } else if target
            .get("required-features")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|f| !enabled.contains(feature_name(f)))
        {
            Some("required-features not enabled by default")
        } else if manifest_harness(manifest, section, name) == Some(false) {
            Some("harness = false")
        } else {
            None
        };
        match reason {
            Some(reason) => targets.skipped.push(format!("{label}: {reason}")),
            None => targets.args.extend(arg),
        }
    }
    targets
}

/// The package's own features enabled by `default`, transitively.
fn default_features(package: &Value) -> HashSet<String> {
    let table = package.get("features").and_then(Value::as_object);
    let mut enabled = HashSet::new();
    let mut pending = vec!["default".to_owned()];
    while let Some(feature) = pending.pop() {
        let Some(entries) = table
            .and_then(|t| t.get(&feature))
            .and_then(Value::as_array)
        else {
            continue;
        };
        if !enabled.insert(feature) {
            continue;
        }
        for entry in entries.iter().filter_map(Value::as_str) {
            // `dep:x` and `x?/f` enable no feature of this package; `x/f` enables `x`.
            if !entry.starts_with("dep:") && !entry.contains("?/") {
                pending.push(feature_name(entry).to_owned());
            }
        }
    }
    enabled
}

/// `x` of a `x/f` feature reference.
fn feature_name(entry: &str) -> &str {
    entry.split_once('/').map_or(entry, |(name, _)| name)
}

/// `harness` as declared in the manifest; `cargo metadata` does not report it.
fn manifest_harness(manifest: &toml::Table, section: &str, name: &str) -> Option<bool> {
    let entry = if section == "lib" {
        manifest.get("lib")
    } else {
        manifest
            .get(section)
            .and_then(toml::Value::as_array)?
            .iter()
            .find(|t| t.get("name").and_then(toml::Value::as_str) == Some(name))
    };
    entry?.get("harness")?.as_bool()
}

fn read_records(dir: &Path) -> Result<Vec<Record>> {
    let mut records = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let record: Record = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("invalid schema export record {}", path.display()))?;
        if record.format != EXPORT_FORMAT {
            bail!(
                "schema export record {} has format {}, expected {EXPORT_FORMAT}; use matching \
                 versions of gts and gts-cli",
                path.display(),
                record.format
            );
        }
        records.push(record);
    }
    Ok(records)
}

/// Assigns each record to the declaration with the same canonical file and name
/// whose inline-module path is the longest suffix of the record's module path.
fn match_records(
    package_name: &str,
    files: &[SourceFile],
    records: Vec<Record>,
    targets: &Targets,
) -> Result<Vec<Exported>> {
    let total = records.len();
    let mut by_decl: HashMap<(usize, usize), Vec<Record>> = HashMap::new();
    for record in records {
        let Some((file_index, file)) = files
            .iter()
            .enumerate()
            .find(|(_, f)| f.path == record.file)
        else {
            continue;
        };
        let module: Vec<&str> = record.module.split("::").collect();
        let decl = file
            .inventory
            .decls
            .iter()
            .enumerate()
            .filter(|(_, d)| d.name == record.struct_name && module_ends_with(&module, d))
            .max_by_key(|(_, d)| d.mods.len());
        match decl {
            Some((decl_index, _)) => by_decl
                .entry((file_index, decl_index))
                .or_default()
                .push(record),
            None => println!(
                "  Note: ignored export of `{}` ({}) from {}: not a declaration in the source text",
                record.struct_name,
                record.module,
                record.file.display()
            ),
        }
    }

    let mut exported = Vec::new();
    let mut missing = String::new();
    for (file_index, file) in files.iter().enumerate() {
        for (decl_index, decl) in file.inventory.decls.iter().enumerate() {
            let Some(records) = by_decl.remove(&(file_index, decl_index)) else {
                let _ = writeln!(
                    missing,
                    "  {}:{}: `{}`",
                    file.path.display(),
                    decl.line,
                    decl.name
                );
                continue;
            };
            let first = &records[0];
            if let Some(other) = records.iter().find(|r| {
                (&r.type_id, &r.dir_path, &r.schema)
                    != (&first.type_id, &first.dir_path, &first.schema)
            }) {
                bail!(
                    "{}:{}: `{}` was exported with different schemas from modules `{}` and `{}`",
                    file.path.display(),
                    decl.line,
                    decl.name,
                    first.module,
                    other.module
                );
            }
            exported.push(Exported {
                file: file.path.clone(),
                line: decl.line,
                struct_name: decl.name.clone(),
                type_id: first.type_id.clone(),
                dir_path: first.dir_path.clone(),
                schema: first.schema.clone(),
            });
        }
    }
    if missing.is_empty() {
        return Ok(exported);
    }
    let hint = if total == 0 {
        format!(
            "no exporter ran in package `{package_name}`: its gts-macros may predate schema \
             export (update gts and gts-macros together), or no selected target compiles these files"
        )
    } else {
        format!(
            "no selected test target of package `{package_name}` exported them: the file is not \
             compiled by a lib, bin or integration test using the libtest harness{}, or the \
             declaration is behind a disabled #[cfg] or feature",
            targets.skipped_note()
        )
    };
    bail!("these declarations were not exported; {hint}:\n{missing}")
}

fn module_ends_with(module: &[&str], decl: &Decl) -> bool {
    module.len() >= decl.mods.len()
        && module[module.len() - decl.mods.len()..]
            .iter()
            .zip(&decl.mods)
            .all(|(a, b)| a == b)
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let mut out = all[all.len().saturating_sub(lines)..].join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Temporary record directory, removed on drop.
struct Spool(PathBuf);

impl Spool {
    fn create() -> Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("gts-schema-export-{}-{nanos}", std::process::id()));
        fs::create_dir(&dir)
            .with_context(|| format!("cannot create schema export directory {}", dir.display()))?;
        Ok(Self(dir.canonicalize()?))
    }
}

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn selects_libtest_targets_only() {
        let package = serde_json::json!({
            "features": {
                "default": ["fast", "serde/std"],
                "fast": ["dep:simd"],
                "serde": ["dep:serde"],
                "x": [],
            },
            "targets": [
            { "name": "pkg", "kind": ["lib"], "test": true },
            { "name": "tool", "kind": ["bin"], "test": true },
            { "name": "manual", "kind": ["bin"], "test": true },
            { "name": "it", "kind": ["test"], "test": true },
            { "name": "custom", "kind": ["test"], "test": true },
            { "name": "gated", "kind": ["test"], "test": true, "required-features": ["x"] },
            { "name": "by_default", "kind": ["test"], "test": true, "required-features": ["fast"] },
            { "name": "via_dep", "kind": ["test"], "test": true, "required-features": ["serde/std"] },
            { "name": "off", "kind": ["test"], "test": false },
            { "name": "bench", "kind": ["bench"], "test": false },
            { "name": "example", "kind": ["example"], "test": false },
            { "name": "build-script-build", "kind": ["custom-build"], "test": false },
        ]});
        let manifest: toml::Table = r#"
            [[bin]]
            name = "manual"
            harness = false

            [[test]]
            name = "custom"
            harness = false
        "#
        .parse()
        .unwrap();
        let targets = test_targets(&package, &manifest);
        assert_eq!(
            targets.args,
            [
                "--lib",
                "--bin",
                "tool",
                "--test",
                "it",
                "--test",
                "by_default",
                "--test",
                "via_dep"
            ]
        );
        assert_eq!(
            targets.skipped,
            [
                "bin `manual`: harness = false",
                "test `custom`: harness = false",
                "test `gated`: required-features not enabled by default",
                "test `off`: test = false",
            ]
        );
    }

    #[test]
    fn default_features_are_transitive() {
        let package = serde_json::json!({ "features": {
            "default": ["a"], "a": ["b", "dep:c"], "b": [], "c": [], "d": ["a"],
        }});
        let mut enabled: Vec<String> = default_features(&package).into_iter().collect();
        enabled.sort();
        assert_eq!(enabled, ["a", "b", "default"]);
        assert!(default_features(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn lib_harness_false_is_skipped() {
        let package =
            serde_json::json!({ "targets": [{ "name": "pkg", "kind": ["lib"], "test": true }] });
        let manifest: toml::Table = "[lib]\nharness = false\n".parse().unwrap();
        assert_eq!(test_targets(&package, &manifest).args, Vec::<String>::new());
    }
}
