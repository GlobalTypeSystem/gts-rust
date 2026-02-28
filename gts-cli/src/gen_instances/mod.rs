pub mod attrs;
pub mod parser;
pub mod string_lit;
pub mod writer;

use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::Path;

use crate::gen_common::{compute_sandbox_root, walk_rust_files};
use parser::ParsedInstance;
use writer::generate_single_instance;

/// Generate GTS well-known instance files from Rust source code annotated with
/// `#[gts_well_known_instance]`.
///
/// # Arguments
/// * `source` - Source directory or file to scan
/// * `output` - Optional output directory override (default: adjacent to the source file)
/// * `exclude_patterns` - Glob-style patterns to exclude during file walking
/// * `verbose` - Verbosity level (0 = normal, 1+ = show skipped files)
///
/// # Errors
/// Returns an error if:
/// - The source path does not exist
/// - Any annotation is malformed or uses an unsupported form
/// - Duplicate instance IDs are detected (hard error, both locations reported)
/// - Duplicate output paths are detected
/// - Any output path escapes the sandbox boundary
/// - File I/O fails
pub fn generate_instances_from_rust(
    source: &str,
    output: Option<&str>,
    exclude_patterns: &[String],
    verbose: u8,
) -> Result<()> {
    println!("Scanning Rust source files for instances in: {source}");

    let source_path = Path::new(source);
    if !source_path.exists() {
        bail!("Source path does not exist: {source}");
    }

    let source_canonical = source_path.canonicalize()?;
    let sandbox_root = compute_sandbox_root(&source_canonical, output)?;

    let mut all_instances: Vec<ParsedInstance> = Vec::new();
    let mut parse_errors: Vec<String> = Vec::new();

    let (files_scanned, files_skipped) =
        walk_rust_files(source_path, exclude_patterns, verbose, |path, content| {
            match parser::extract_instances_from_source(content, path) {
                Ok(instances) => all_instances.extend(instances),
                Err(e) => parse_errors.push(format!("{}: {}", path.display(), e)),
            }
            Ok(())
        })?;

    // Report all parse errors before bailing
    if !parse_errors.is_empty() {
        let mut sorted = parse_errors;
        sorted.sort();
        sorted.dedup();
        for err in &sorted {
            eprintln!("error: {err}");
        }
        bail!(
            "Instance generation failed with {} parse error(s):\n{}",
            sorted.len(),
            sorted.join("\n")
        );
    }

    check_duplicate_ids(&all_instances)?;
    check_duplicate_output_paths(&all_instances, output, &sandbox_root)?;

    let instances_generated = emit_instances(&all_instances, output, &sandbox_root)?;

    print_summary(files_scanned, files_skipped, instances_generated);
    Ok(())
}

/// Hard-error if two annotations share the same composed instance ID.
fn check_duplicate_ids(instances: &[ParsedInstance]) -> Result<()> {
    let mut id_seen: HashMap<String, String> = HashMap::new();
    let mut errors: Vec<String> = Vec::new();

    for inst in instances {
        let composed = format!("{}{}", inst.attrs.schema_id, inst.attrs.instance_segment);
        if let Some(prev) = id_seen.get(composed.as_str()) {
            errors.push(format!(
                "Duplicate instance ID '{composed}':\n  first: {}\n  second: {}:{}",
                prev, inst.source_file, inst.line
            ));
        } else {
            id_seen.insert(composed, format!("{}:{}", inst.source_file, inst.line));
        }
    }

    if !errors.is_empty() {
        errors.sort();
        for err in &errors {
            eprintln!("error: {err}");
        }
        bail!(
            "Instance generation failed: {} duplicate instance ID(s)",
            errors.len()
        );
    }
    Ok(())
}

/// Hard-error if two annotations would produce the same output file path.
fn check_duplicate_output_paths(
    instances: &[ParsedInstance],
    output: Option<&str>,
    sandbox_root: &Path,
) -> Result<()> {
    let mut path_seen: HashMap<String, String> = HashMap::new();
    let mut errors: Vec<String> = Vec::new();

    for inst in instances {
        let composed = format!("{}{}", inst.attrs.schema_id, inst.attrs.instance_segment);
        let file_rel =
            std::path::Path::new(&inst.attrs.dir_path).join(format!("{composed}.instance.json"));
        let raw_path = if let Some(od) = output {
            Path::new(od).join(&file_rel)
        } else {
            let src_dir = Path::new(inst.source_file.as_str())
                .parent()
                .unwrap_or(sandbox_root);
            src_dir.join(&file_rel)
        };
        let key = raw_path
            .components()
            .collect::<std::path::PathBuf>()
            .to_string_lossy()
            .into_owned();
        if let Some(prev) = path_seen.get(&key) {
            errors.push(format!(
                "Duplicate output path '{}':\n  first: {}\n  second: {}:{}",
                raw_path.display(),
                prev,
                inst.source_file,
                inst.line
            ));
        } else {
            path_seen.insert(key, format!("{}:{}", inst.source_file, inst.line));
        }
    }

    if !errors.is_empty() {
        errors.sort();
        for err in &errors {
            eprintln!("error: {err}");
        }
        bail!(
            "Instance generation failed: {} duplicate output path(s)",
            errors.len()
        );
    }
    Ok(())
}

/// Generate all instance files, returning the count of files written.
fn emit_instances(
    instances: &[ParsedInstance],
    output: Option<&str>,
    sandbox_root: &Path,
) -> Result<usize> {
    let mut count = 0;
    for inst in instances {
        let file_path = generate_single_instance(inst, output, sandbox_root)
            .map_err(|e| anyhow::anyhow!("{}: {}", inst.source_file, e))?;
        let composed = format!("{}{}", inst.attrs.schema_id, inst.attrs.instance_segment);
        println!("  Generated instance: {composed} @ {file_path}");
        count += 1;
    }
    Ok(count)
}

fn print_summary(files_scanned: usize, files_skipped: usize, instances_generated: usize) {
    println!("\nSummary:");
    println!("  Files scanned:      {files_scanned}");
    println!("  Files skipped:      {files_skipped}");
    println!("  Instances generated: {instances_generated}");
    if instances_generated == 0 {
        println!("\n  No instances found. Annotate consts with `#[gts_well_known_instance(...)]`.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_src(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    fn valid_src(instance_segment: &str, json_body: &str) -> String {
        format!(
            concat!(
                "#[gts_well_known_instance(\n",
                "    dir_path = \"instances\",\n",
                "    schema_id = \"gts.x.core.events.topic.v1~\",\n",
                "    instance_segment = \"{}\"\n",
                ")]\n",
                "pub const FOO: &str = {};\n"
            ),
            instance_segment, json_body
        )
    }

    #[test]
    fn test_end_to_end_single_instance() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        write_src(
            &root,
            "module.rs",
            &valid_src(
                "x.commerce._.orders.v1.0",
                r#""{\"name\": \"orders\", \"partitions\": 16}""#,
            ),
        );

        generate_instances_from_rust(root.to_str().unwrap(), Some(root.to_str().unwrap()), &[], 0)
            .unwrap();

        let expected = root
            .join("instances")
            .join("gts.x.core.events.topic.v1~x.commerce._.orders.v1.0.instance.json");
        assert!(expected.exists());

        let val: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&expected).unwrap()).unwrap();
        assert_eq!(
            val["id"],
            "gts.x.core.events.topic.v1~x.commerce._.orders.v1.0"
        );
        assert_eq!(val["name"], "orders");
        assert_eq!(val["partitions"], 16);
    }

    #[test]
    fn test_nonexistent_source_errors() {
        let result =
            generate_instances_from_rust("/nonexistent/path/that/does/not/exist", None, &[], 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_duplicate_id_hard_error() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        let dup_src = concat!(
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    schema_id = \"gts.x.core.events.topic.v1~\",\n",
            "    instance_segment = \"x.commerce._.orders.v1.0\"\n",
            ")]\n",
            "const A: &str = \"{\\\"name\\\": \\\"a\\\"}\";\n",
            "#[gts_well_known_instance(\n",
            "    dir_path = \"instances\",\n",
            "    schema_id = \"gts.x.core.events.topic.v1~\",\n",
            "    instance_segment = \"x.commerce._.orders.v1.0\"\n",
            ")]\n",
            "const B: &str = \"{\\\"name\\\": \\\"b\\\"}\";\n"
        );
        write_src(&root, "dup.rs", dup_src);

        let result = generate_instances_from_rust(
            root.to_str().unwrap(),
            Some(root.to_str().unwrap()),
            &[],
            0,
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("duplicate instance ID"),
            "Expected duplicate ID error"
        );
    }

    #[test]
    fn test_exclude_pattern_skips_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        // This file has a malformed annotation that would error if scanned
        write_src(
            &root,
            "excluded.rs",
            concat!(
                "#[gts_well_known_instance(\n",
                "    dir_path = \"instances\",\n",
                "    schema_id = \"bad-no-tilde\",\n",
                "    instance_segment = \"x.a.v1.0\"\n",
                ")]\n",
                "const X: &str = \"{}\";\n"
            ),
        );

        let result = generate_instances_from_rust(
            root.to_str().unwrap(),
            Some(root.to_str().unwrap()),
            &["excluded.rs".to_owned()],
            0,
        );
        assert!(
            result.is_ok(),
            "Expected excluded file to be skipped: {result:?}"
        );
    }

    #[test]
    fn test_no_annotations_succeeds_with_zero_generated() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        write_src(&root, "plain.rs", "const FOO: u32 = 42;\n");

        let result = generate_instances_from_rust(
            root.to_str().unwrap(),
            Some(root.to_str().unwrap()),
            &[],
            0,
        );
        assert!(result.is_ok());
    }
}
