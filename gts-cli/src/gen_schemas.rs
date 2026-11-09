use anyhow::{Context, Result, bail};
use gts::{GTS_ID_URI_PREFIX, GtsInstanceId, GtsTypeId};
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

mod source;

use source::extract_schemas;
#[cfg(test)]
use source::parse_macro_attrs;
use walkdir::WalkDir;

/// Directories that are automatically ignored (e.g., trybuild `compile_fail` tests)
const AUTO_IGNORE_DIRS: &[&str] = &["compile_fail"];

/// Reason why a file was skipped
#[derive(Debug, Clone, Copy)]
enum SkipReason {
    ExcludePattern,
    AutoIgnoredDir,
    IgnoreDirective,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExcludePattern => write!(f, "matched --exclude pattern"),
            Self::AutoIgnoredDir => write!(f, "in auto-ignored directory (compile_fail)"),
            Self::IgnoreDirective => write!(f, "has // gts:ignore directive"),
        }
    }
}

/// Parsed macro attributes from `#[struct_to_gts_schema(...)]`
#[derive(Debug, Clone)]
struct MacroAttrs {
    type_id: String,
    description: Option<String>,
    properties: Option<String>,
    base: BaseAttr,
}

/// Base attribute type
#[derive(Debug, Clone)]
enum BaseAttr {
    /// `base = true` - this is a base type
    IsBase,
    /// `base = ParentStruct` - this type inherits from `ParentStruct`
    Parent(String),
}

struct SchemaDeclaration {
    type_id: String,
    schema: serde_json::Value,
    source_file: PathBuf,
    struct_name: String,
}

/// Preflight every destination before creating directories or writing files.
fn write_schemas(schemas: &BTreeMap<String, SchemaDeclaration>, output_dir: &Path) -> Result<()> {
    for type_id in schemas.keys() {
        let path = output_dir.join(format!("{type_id}.schema.json"));
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                bail!(
                    "Refusing to write schema to non-regular file {}",
                    path.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("Cannot inspect {}", path.display()));
            }
        }
    }
    if !schemas.is_empty() {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("Cannot create output directory {}", output_dir.display()))?;
    }
    for (type_id, declaration) in schemas {
        let path = output_dir.join(format!("{type_id}.schema.json"));
        fs::write(&path, serde_json::to_string_pretty(&declaration.schema)?)
            .with_context(|| format!("Cannot write schema {}", path.display()))?;
        println!("  Generated schema: {type_id} @ {}", path.display());
    }
    Ok(())
}

/// Generate GTS schemas from Rust source code with `#[struct_to_gts_schema]` annotations
///
/// # Arguments
/// * `source` - Source directory or file to scan
/// * `output` - Final output directory; defaults to `./schemas` relative to the working directory
/// * `exclude_patterns` - Patterns to exclude (supports simple glob matching)
/// * `verbose` - Verbosity level (0 = normal, 1+ = show skipped files)
///
/// # Errors
///
/// Returns an error if:
/// - The source path does not exist
/// - A source cannot be read or parsed, or a type ID is invalid
/// - Different declarations use the same type ID
/// - An output file is a symlink or another non-regular file
/// - File I/O operations fail
pub fn generate_schemas_from_rust(
    source: &str,
    output: Option<&str>,
    exclude_patterns: &[String],
    verbose: u8,
) -> Result<()> {
    println!("Scanning Rust source files in: {source}");

    let source_path = Path::new(source);
    if !source_path.exists() {
        bail!("Source path does not exist: {source}");
    }

    let mut schemas: BTreeMap<String, SchemaDeclaration> = BTreeMap::new();
    let mut files_scanned = 0;
    let mut files_skipped = 0;

    // Walk through all .rs files
    for entry in WalkDir::new(source_path)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| {
            !entry.file_type().is_dir()
                || (!should_exclude_path(entry.path(), exclude_patterns)
                    && !is_in_auto_ignored_dir(entry.path()))
        })
    {
        let entry = entry.with_context(|| format!("Failed to scan {}", source_path.display()))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }

        // Check if path should be excluded
        if should_exclude_path(path, exclude_patterns) {
            files_skipped += 1;
            if verbose > 0 {
                println!(
                    "  Skipped: {} ({})",
                    path.display(),
                    SkipReason::ExcludePattern
                );
            }
            continue;
        }

        // Check for auto-ignored directories (e.g., compile_fail)
        if is_in_auto_ignored_dir(path) {
            files_skipped += 1;
            if verbose > 0 {
                println!(
                    "  Skipped: {} ({})",
                    path.display(),
                    SkipReason::AutoIgnoredDir
                );
            }
            continue;
        }

        files_scanned += 1;
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        if has_ignore_directive(&content) {
            files_skipped += 1;
            if verbose > 0 {
                println!(
                    "  Skipped: {} ({})",
                    path.display(),
                    SkipReason::IgnoreDirective
                );
            }
            continue;
        }

        for declaration in extract_schemas(&content, path)? {
            if let Some(previous) = schemas.get(&declaration.type_id) {
                if previous.schema != declaration.schema {
                    bail!(
                        "Conflicting schemas for {}: {}:{} and {}:{}",
                        declaration.type_id,
                        previous.source_file.display(),
                        previous.struct_name,
                        declaration.source_file.display(),
                        declaration.struct_name
                    );
                }
            } else {
                schemas.insert(declaration.type_id.clone(), declaration);
            }
        }
    }

    let output_dir = Path::new(output.unwrap_or("schemas"));
    write_schemas(&schemas, output_dir)?;
    let schemas_generated = schemas.len();

    println!("\nSummary:");
    println!("  Files scanned: {files_scanned}");
    println!("  Files skipped: {files_skipped}");
    println!("  Schemas generated: {schemas_generated}");

    if schemas_generated == 0 {
        println!(
            "\n- No schemas found. Make sure your structs are annotated with `#[struct_to_gts_schema(...)]`"
        );
    }

    Ok(())
}

/// Check if a path matches any of the exclude patterns
fn should_exclude_path(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();

    for pattern in patterns {
        if matches_glob_pattern(&path_str, pattern) {
            return true;
        }
    }

    false
}

/// Simple glob pattern matching
/// Supports: * (any characters), ** (any path segments)
fn matches_glob_pattern(path: &str, pattern: &str) -> bool {
    // Convert glob pattern to regex
    let regex_pattern = pattern
        .replace('.', r"\.")
        .replace("**", "<<DOUBLESTAR>>")
        .replace('*', "[^/]*")
        .replace("<<DOUBLESTAR>>", ".*");

    if let Ok(re) = Regex::new(&format!("(^|/){regex_pattern}($|/)")) {
        re.is_match(path)
    } else {
        // Fallback to simple contains check
        path.contains(pattern)
    }
}

/// Check if path is in an auto-ignored directory (e.g., `compile_fail`)
fn is_in_auto_ignored_dir(path: &Path) -> bool {
    path.components().any(|component| {
        if let Some(name) = component.as_os_str().to_str() {
            AUTO_IGNORE_DIRS.contains(&name)
        } else {
            false
        }
    })
}

/// Check if file content starts with the gts:ignore directive
fn has_ignore_directive(content: &str) -> bool {
    // Check first few lines for the directive
    for line in content.lines().take(10) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Check for the directive (case-insensitive)
        if trimmed.to_lowercase().starts_with("// gts:ignore") {
            return true;
        }
        // If we hit a non-comment, non-empty line, stop looking
        if !trimmed.starts_with("//") && !trimmed.starts_with("#!") {
            break;
        }
    }
    false
}

/// Build a JSON Schema object from parsed metadata
fn build_json_schema(
    type_id: &str,
    struct_name: &str,
    description: Option<&str>,
    properties_list: Option<&str>,
    base: &BaseAttr,
    field_types: &HashMap<String, String>,
) -> serde_json::Value {
    use serde_json::json;

    let mut schema_properties = serde_json::Map::new();
    let mut required = Vec::new();

    // Determine which properties to include
    let property_names: Vec<&str> = if let Some(props) = properties_list {
        props.split(',').map(str::trim).collect()
    } else {
        // If no properties specified, include all fields
        field_types.keys().map(String::as_str).collect()
    };

    for prop in &property_names {
        if let Some(field_type) = field_types.get(*prop) {
            let (is_required, json_type_info) = rust_type_to_json_schema(field_type);

            schema_properties.insert((*prop).to_owned(), json_type_info);

            if is_required {
                required.push((*prop).to_owned());
            }
        }
    }

    // Sort required array for consistent output
    required.sort();

    // Build schema based on whether this has a parent

    match base {
        BaseAttr::IsBase => {
            // Base type - simple flat schema
            let mut s = json!({
                "$id": format!("{GTS_ID_URI_PREFIX}{type_id}"),
                "$schema": gts::JSON_SCHEMA_DRAFT_07,
                "title": struct_name,
                "type": "object",
                "additionalProperties": false,
                "properties": schema_properties
            });

            if let Some(desc) = description {
                s["description"] = json!(desc);
            }

            if !required.is_empty() {
                s["required"] = json!(required);
            }

            s
        }
        BaseAttr::Parent(parent_name) => {
            // Child type - use allOf with $ref to parent
            // The parent's type_id is derived from this schema's ID by removing the last segment
            let parent_type_id = derive_parent_type_id(type_id);

            let mut own_properties = json!({
                "properties": schema_properties
            });

            if !required.is_empty() {
                own_properties["required"] = json!(required);
            }

            let mut s = json!({
                "$id": format!("{GTS_ID_URI_PREFIX}{type_id}"),
                "$schema": gts::JSON_SCHEMA_DRAFT_07,
                "title": format!("{struct_name} (extends {parent_name})"),
                "type": "object",
                "allOf": [
                    { "$ref": format!("{GTS_ID_URI_PREFIX}{parent_type_id}") },
                    own_properties
                ]
            });

            if let Some(desc) = description {
                s["description"] = json!(desc);
            }

            s
        }
    }
}

/// Derive parent type ID from child type ID
/// e.g., "gts.x.core.events.type.v1~x.core.audit.event.v1~" -> "gts.x.core.events.type.v1~"
fn derive_parent_type_id(type_id: &str) -> String {
    // Remove trailing ~ if present for processing
    let s = type_id.trim_end_matches('~');

    // Find the last ~ and take everything before it, then add ~ back
    if let Some(pos) = s.rfind('~') {
        format!("{}~", &s[..pos])
    } else {
        // No parent segment found, this shouldn't happen for child types
        type_id.to_owned()
    }
}

/// Convert Rust type string to JSON Schema type
/// Returns (`is_required`, `json_schema_value`)
///
/// This function inlines the actual schema definitions for GTS types (like `GtsInstanceId`)
/// to match what schemars generates, including custom extensions like `x-gts-instance-ref`.
fn rust_type_to_json_schema(rust_type: &str) -> (bool, serde_json::Value) {
    use serde_json::json;

    let rust_type = rust_type.trim();

    // Check if it's an Option type
    let is_optional = rust_type.starts_with("Option<");
    let inner_type = if is_optional {
        rust_type
            .strip_prefix("Option<")
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(rust_type)
            .trim()
    } else {
        rust_type
    };

    let json_schema = match inner_type {
        "String" | "str" | "&str" => json!({ "type": "string" }),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => json!({ "type": "integer" }),
        "f32" | "f64" => json!({ "type": "number" }),
        "bool" => json!({ "type": "boolean" }),
        t if t.starts_with("Vec<") => {
            let item_type = t
                .strip_prefix("Vec<")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or("string");
            let (_, item_schema) = rust_type_to_json_schema(item_type);
            json!({
                "type": "array",
                "items": item_schema
            })
        }
        t if t.starts_with("HashMap<") || t.starts_with("BTreeMap<") => {
            json!({ "type": "object" })
        }
        t if t.contains("Uuid") || t.contains("uuid") => {
            json!({ "type": "string", "format": "uuid" })
        }
        // GtsInstanceId - use the canonical schema from the gts crate
        "GtsInstanceId" => GtsInstanceId::json_schema_value(),
        // GtsTypeId (or its deprecated alias GtsSchemaId) — use the canonical schema from the gts crate
        "GtsTypeId" | "GtsSchemaId" => GtsTypeId::json_schema_value(),
        // Generic type parameter (e.g., P, T, etc.) - treat as object
        t if t.len() <= 2 && t.chars().all(|c| c.is_ascii_uppercase()) => {
            json!({ "type": "object" })
        }
        // Other types - default to object (could be another struct)
        _ => json!({ "type": "object" }),
    };

    // For Option types, add null to the type array
    let final_schema = if is_optional {
        if let Some(type_val) = json_schema.get("type").and_then(|v| v.as_str()) {
            json!({ "type": [type_val, "null"] })
        } else {
            // For $ref types, wrap in oneOf with null
            json!({
                "oneOf": [
                    json_schema,
                    { "type": "null" }
                ]
            })
        }
    } else {
        json_schema
    };

    (!is_optional, final_schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_glob_pattern() {
        // Test simple patterns
        assert!(matches_glob_pattern(
            "src/tests/compile_fail/test.rs",
            "compile_fail"
        ));
        assert!(matches_glob_pattern(
            "tests/compile_fail/test.rs",
            "compile_fail"
        ));

        // Test wildcard patterns
        assert!(matches_glob_pattern("src/tests/foo.rs", "tests/*"));
        assert!(matches_glob_pattern("src/examples/bar.rs", "examples/*"));

        // Test double-star patterns
        assert!(matches_glob_pattern("a/b/c/d/test.rs", "**/test.rs"));
    }

    #[test]
    fn test_is_in_auto_ignored_dir() {
        assert!(is_in_auto_ignored_dir(Path::new(
            "tests/compile_fail/test.rs"
        )));
        assert!(is_in_auto_ignored_dir(Path::new("src/compile_fail/foo.rs")));
        assert!(!is_in_auto_ignored_dir(Path::new("src/models.rs")));
        assert!(!is_in_auto_ignored_dir(Path::new("tests/integration.rs")));
    }

    #[test]
    fn test_has_ignore_directive() {
        assert!(has_ignore_directive("// gts:ignore\nuse foo::bar;"));
        assert!(has_ignore_directive("// GTS:IGNORE\nuse foo::bar;"));
        assert!(has_ignore_directive(
            "//! Module doc\n// gts:ignore\nuse foo::bar;"
        ));
        assert!(!has_ignore_directive("use foo::bar;\n// gts:ignore"));
        assert!(!has_ignore_directive("use foo::bar;"));
    }

    #[test]
    fn test_parse_macro_attrs_base_true() {
        let attr_body = r#"
            base = true,
            schema_id = "gts.x.core.events.type.v1~",
            description = "Base event type"
        "#;

        let attrs = parse_macro_attrs(attr_body).unwrap();
        assert_eq!(attrs.type_id, "gts.x.core.events.type.v1~");
        assert_eq!(attrs.description.as_deref(), Some("Base event type"));
        assert!(matches!(attrs.base, BaseAttr::IsBase));
    }

    #[test]
    fn test_parse_macro_attrs_base_parent() {
        let attr_body = r#"
            base = BaseEventV1,
            schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~"
        "#;

        let attrs = parse_macro_attrs(attr_body).unwrap();
        assert_eq!(
            attrs.type_id,
            "gts.x.core.events.type.v1~x.core.audit.event.v1~"
        );
        assert!(matches!(attrs.base, BaseAttr::Parent(ref p) if p == "BaseEventV1"));
    }

    #[test]
    fn test_derive_parent_type_id() {
        assert_eq!(
            derive_parent_type_id("gts.x.core.events.type.v1~x.core.audit.event.v1~"),
            "gts.x.core.events.type.v1~"
        );
        assert_eq!(
            derive_parent_type_id(
                "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
            ),
            "gts.x.core.events.type.v1~x.core.audit.event.v1~"
        );
    }

    #[test]
    fn test_rust_type_to_json_schema() {
        // Basic types
        let (req, schema) = rust_type_to_json_schema("String");
        assert!(req);
        assert_eq!(schema["type"], "string");

        let (req, schema) = rust_type_to_json_schema("i32");
        assert!(req);
        assert_eq!(schema["type"], "integer");

        let (req, schema) = rust_type_to_json_schema("bool");
        assert!(req);
        assert_eq!(schema["type"], "boolean");

        // Optional types
        let (req, schema) = rust_type_to_json_schema("Option<String>");
        assert!(!req);
        assert_eq!(schema["type"][0], "string");
        assert_eq!(schema["type"][1], "null");

        // Vec types
        let (req, schema) = rust_type_to_json_schema("Vec<String>");
        assert!(req);
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "string");

        // GTS types - inlined with their entity-reference extension
        let (req, schema) = rust_type_to_json_schema("GtsInstanceId");
        assert!(req);
        assert_eq!(schema["type"], "string");
        assert!(schema.get("format").is_none());
        assert_eq!(
            schema["x-gts-instance-ref"],
            format!("{}*", gts::GTS_ID_PREFIX)
        );

        let (_, schema) = rust_type_to_json_schema("GtsTypeId");
        assert!(schema.get("format").is_none());
        assert_eq!(schema["x-gts-type-ref"], format!("{}*", gts::GTS_ID_PREFIX));

        // Generic type parameter
        let (req, schema) = rust_type_to_json_schema("P");
        assert!(req);
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_should_exclude_path_matching_pattern() {
        let patterns = vec!["test_*".to_owned(), "**/target/**".to_owned()];
        let path = Path::new("src/test_helper.rs");
        assert!(should_exclude_path(path, &patterns));
    }

    #[test]
    fn test_should_exclude_path_no_match() {
        let patterns = vec!["test_*".to_owned(), "**/compile_fail/**".to_owned()];
        let path = Path::new("src/main.rs");
        assert!(!should_exclude_path(path, &patterns));
    }

    #[test]
    fn test_build_json_schema_base_type() {
        use serde_json::json;

        let mut field_types = HashMap::new();
        field_types.insert("id".to_owned(), "String".to_owned());
        field_types.insert("count".to_owned(), "i32".to_owned());
        field_types.insert("active".to_owned(), "bool".to_owned());

        let schema = build_json_schema(
            "gts.x.test.entities.base.v1~",
            "BaseStruct",
            Some("A base test struct"),
            None, // Include all properties
            &BaseAttr::IsBase,
            &field_types,
        );

        assert_eq!(schema["$id"], "gts://gts.x.test.entities.base.v1~");
        assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
        assert_eq!(schema["title"], "BaseStruct");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["description"], "A base test struct");
        assert_eq!(schema["additionalProperties"], false);

        // Check properties
        assert!(schema["properties"]["id"].is_object());
        assert!(schema["properties"]["count"].is_object());
        assert!(schema["properties"]["active"].is_object());

        // Check required fields (all 3 should be required)
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 3);
        assert!(required.contains(&json!("active")));
        assert!(required.contains(&json!("count")));
        assert!(required.contains(&json!("id")));
    }

    #[test]
    fn test_build_json_schema_child_type() {
        let mut field_types = HashMap::new();
        field_types.insert("child_field".to_owned(), "String".to_owned());
        field_types.insert("optional_field".to_owned(), "Option<i32>".to_owned());

        let schema = build_json_schema(
            "gts.x.test.entities.base.v1~x.test.entities.child.v1~",
            "ChildStruct",
            Some("A child test struct"),
            None,
            &BaseAttr::Parent("BaseStruct".to_owned()),
            &field_types,
        );

        assert_eq!(
            schema["$id"],
            "gts://gts.x.test.entities.base.v1~x.test.entities.child.v1~"
        );
        assert_eq!(schema["title"], "ChildStruct (extends BaseStruct)");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["description"], "A child test struct");

        // Check allOf structure
        let all_of = schema["allOf"].as_array().unwrap();
        assert_eq!(all_of.len(), 2);

        // First element should be $ref to parent
        assert_eq!(all_of[0]["$ref"], "gts://gts.x.test.entities.base.v1~");

        // Second element should have child properties
        assert!(all_of[1]["properties"]["child_field"].is_object());
        assert!(all_of[1]["properties"]["optional_field"].is_object());

        // Check required fields (only child_field, not optional_field)
        let required = all_of[1]["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "child_field");
    }

    #[test]
    fn test_parse_macro_attrs_with_schema_id() {
        let attr_body = r#"
            base = true,
            schema_id = "gts.x.custom.entities.id.v1~"
        "#;

        let attrs = parse_macro_attrs(attr_body).unwrap();
        assert_eq!(attrs.type_id, "gts.x.custom.entities.id.v1~");
        assert!(matches!(attrs.base, BaseAttr::IsBase));
        assert!(attrs.description.is_none());
    }

    #[test]
    fn test_extract_and_generate_schemas_single_struct() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().canonicalize().unwrap();

        // Create a test Rust file with a struct
        let test_file = temp_path.join("test.rs");
        let content = r#"
use gts::GtsInstanceId;

#[struct_to_gts_schema(
    base = true,
    schema_id = "gts.x.test.entities.person.v1~",
    description = "A test person struct"
)]
pub struct Person {
    pub id: GtsInstanceId,
    pub name: String,
    pub age: i32,
}
"#;
        fs::write(&test_file, content).unwrap();

        generate_schemas_from_rust(
            test_file.to_str().unwrap(),
            Some(temp_path.to_str().unwrap()),
            &[],
            0,
        )
        .unwrap();
        let results: Vec<_> = extract_schemas(content, &test_file)
            .unwrap()
            .into_iter()
            .map(|s| {
                let path = temp_path.join(format!("{}.schema.json", s.type_id));
                (s.type_id, path.display().to_string())
            })
            .collect();

        // Verify results
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "gts.x.test.entities.person.v1~");

        // Verify schema file was created
        let schema_path = Path::new(&results[0].1);
        assert!(schema_path.exists());

        // Verify schema content
        let schema_content = fs::read_to_string(schema_path).unwrap();
        let schema: serde_json::Value = serde_json::from_str(&schema_content).unwrap();

        assert_eq!(schema["$id"], "gts://gts.x.test.entities.person.v1~");
        assert_eq!(schema["title"], "Person");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["description"], "A test person struct");
        assert!(schema["properties"]["id"].is_object());
        assert!(schema["properties"]["name"].is_object());
        assert!(schema["properties"]["age"].is_object());
    }

    #[test]
    fn test_extract_and_generate_schemas_with_parent() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().canonicalize().unwrap();

        // Create a test file with parent and child structs
        let test_file = temp_path.join("test.rs");
        let content = r#"
use gts::GtsInstanceId;

#[struct_to_gts_schema(
    base = true,
    schema_id = "gts.x.test.entities.base.v1~",
    description = "Base event"
)]
pub struct BaseEvent {
    pub id: GtsInstanceId,
    pub timestamp: String,
}

#[struct_to_gts_schema(
    base = BaseEvent,
    schema_id = "gts.x.test.entities.base.v1~x.test.entities.child.v1~",
    description = "Child event"
)]
pub struct ChildEvent {
    pub event_type: String,
    pub data: String,
}
"#;
        fs::write(&test_file, content).unwrap();

        generate_schemas_from_rust(
            test_file.to_str().unwrap(),
            Some(temp_path.to_str().unwrap()),
            &[],
            0,
        )
        .unwrap();
        let results: Vec<_> = extract_schemas(content, &test_file)
            .unwrap()
            .into_iter()
            .map(|s| {
                let path = temp_path.join(format!("{}.schema.json", s.type_id));
                (s.type_id, path.display().to_string())
            })
            .collect();

        // Verify results - should have 2 schemas
        assert_eq!(results.len(), 2);

        // Find base and child schemas
        let base_result = results
            .iter()
            .find(|(id, _)| id == "gts.x.test.entities.base.v1~")
            .unwrap();
        let child_result = results
            .iter()
            .find(|(id, _)| id == "gts.x.test.entities.base.v1~x.test.entities.child.v1~")
            .unwrap();

        // Verify base schema
        let base_schema_path = Path::new(&base_result.1);
        assert!(base_schema_path.exists());
        let base_schema: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(base_schema_path).unwrap()).unwrap();
        assert_eq!(base_schema["title"], "BaseEvent");
        assert!(base_schema["properties"]["id"].is_object());

        // Verify child schema
        let child_schema_path = Path::new(&child_result.1);
        assert!(child_schema_path.exists());
        let child_schema: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(child_schema_path).unwrap()).unwrap();
        assert_eq!(child_schema["title"], "ChildEvent (extends BaseEvent)");

        // Verify allOf structure with parent reference
        let all_of = child_schema["allOf"].as_array().unwrap();
        assert_eq!(all_of.len(), 2);
        assert_eq!(all_of[0]["$ref"], "gts://gts.x.test.entities.base.v1~");
        assert!(all_of[1]["properties"]["event_type"].is_object());
    }

    #[test]
    fn test_rust_type_to_json_schema_option_string() {
        let (required, schema) = rust_type_to_json_schema("Option<String>");
        assert!(!required);
        assert_eq!(schema["type"][0], "string");
        assert_eq!(schema["type"][1], "null");
    }

    #[test]
    fn test_rust_type_to_json_schema_hashmap() {
        let (required, schema) = rust_type_to_json_schema("HashMap<String, i32>");
        assert!(required);
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_rust_type_to_json_schema_vec_bool() {
        let (required, schema) = rust_type_to_json_schema("Vec<bool>");
        assert!(required);
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "boolean");
    }

    #[test]
    fn test_rust_type_to_json_schema_unknown_type() {
        let (required, schema) = rust_type_to_json_schema("CustomStruct");
        assert!(required);
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_should_exclude_path_with_patterns() {
        let patterns = vec!["target/**".to_owned(), "*.tmp".to_owned()];

        assert!(should_exclude_path(
            Path::new("target/debug/foo"),
            &patterns
        ));
        assert!(should_exclude_path(Path::new("file.tmp"), &patterns));
        assert!(!should_exclude_path(Path::new("src/main.rs"), &patterns));
    }

    #[test]
    fn test_should_exclude_path_empty_patterns() {
        let patterns = vec![];
        assert!(!should_exclude_path(Path::new("anything.rs"), &patterns));
    }

    #[test]
    fn test_is_in_auto_ignored_dir_specific_paths() {
        assert!(is_in_auto_ignored_dir(Path::new(
            "tests/compile_fail/test.rs"
        )));
        assert!(is_in_auto_ignored_dir(Path::new("src/compile_fail/foo.rs")));
        assert!(!is_in_auto_ignored_dir(Path::new("target/debug")));
        assert!(!is_in_auto_ignored_dir(Path::new("node_modules/pkg")));
        assert!(!is_in_auto_ignored_dir(Path::new("src/main.rs")));
        assert!(!is_in_auto_ignored_dir(Path::new("tests/test.rs")));
    }

    #[test]
    fn test_has_ignore_directive_variations() {
        assert!(has_ignore_directive("// gts:ignore\nstruct Foo {}"));
        assert!(has_ignore_directive("// GTS:IGNORE\nstruct Foo {}"));
        assert!(!has_ignore_directive("struct Foo {}\nfn bar() {}"));
        assert!(!has_ignore_directive("struct Foo {}\n// gts:ignore"));
    }

    #[test]
    fn test_parse_macro_attrs_edge_cases() {
        // With base true
        let attr1 = r#"base = true, schema_id = "gts.x.test.entities.item.v1~""#;
        let result1 = parse_macro_attrs(attr1).unwrap();
        assert_eq!(result1.type_id, "gts.x.test.entities.item.v1~");
        assert!(matches!(result1.base, BaseAttr::IsBase));

        // With base parent
        let attr2 = r#"base = ParentStruct, schema_id = "gts.x.test.entities.item.v1~""#;
        let result2 = parse_macro_attrs(attr2).unwrap();
        assert!(matches!(result2.base, BaseAttr::Parent(ref p) if p == "ParentStruct"));

        // Missing field
        assert!(parse_macro_attrs("base = true").is_err());

        // Malformed
        assert!(parse_macro_attrs(r"invalid syntax here").is_err());
    }

    #[test]
    fn test_generate_schemas_from_rust_with_exclude() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create test file that should be excluded
        let test_file = temp_path.join("test_excluded.rs");
        fs::write(&test_file, "// test file").unwrap();

        // Call with exclude pattern
        let result = generate_schemas_from_rust(
            temp_path.to_str().unwrap(),
            None,
            &["test_*.rs".to_owned()],
            1, // verbose
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_generate_schemas_from_rust_with_ignore_directive() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create test file with ignore directive
        let test_file = temp_path.join("ignored.rs");
        fs::write(&test_file, "// gts:ignore\nstruct Foo {}").unwrap();

        let result = generate_schemas_from_rust(
            temp_path.to_str().unwrap(),
            None,
            &[],
            1, // verbose
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_generate_schemas_from_rust_nonexistent_path() {
        let result =
            generate_schemas_from_rust("/nonexistent/path/that/does/not/exist", None, &[], 0);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }
}
