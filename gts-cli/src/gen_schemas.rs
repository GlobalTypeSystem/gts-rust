use anyhow::{Result, bail};
use gts::{GtsInstanceId, GtsSchemaId};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::gen_common::{compute_sandbox_root, safe_canonicalize_nonexistent, walk_rust_files};

/// Parsed macro attributes from `#[struct_to_gts_schema(...)]`
#[derive(Debug, Clone)]
struct MacroAttrs {
    dir_path: String,
    schema_id: String,
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

/// Generate GTS schemas from Rust source code with `#[struct_to_gts_schema]` annotations
///
/// # Arguments
/// * `source` - Source directory or file to scan
/// * `output` - Optional output directory override
/// * `exclude_patterns` - Patterns to exclude (supports simple glob matching)
/// * `verbose` - Verbosity level (0 = normal, 1+ = show skipped files)
///
/// # Errors
///
/// Returns an error if:
/// - The source path does not exist
/// - The output path is outside the sandbox boundary
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

    let source_canonical = source_path.canonicalize()?;
    let sandbox_root = compute_sandbox_root(&source_canonical, output)?;

    let mut schemas_generated = 0;

    let (files_scanned, files_skipped) =
        walk_rust_files(source_path, exclude_patterns, verbose, |path, content| {
            let results = extract_and_generate_schemas(content, output, &sandbox_root, path)?;
            schemas_generated += results.len();
            for (schema_id, file_path) in results {
                println!("  Generated schema: {schema_id} @ {file_path}");
            }
            Ok(())
        })?;

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

/// Parse the attribute body of `#[struct_to_gts_schema(...)]` to extract individual attributes
fn parse_macro_attrs(attr_body: &str) -> Option<MacroAttrs> {
    // Patterns for extracting individual attributes
    let dir_path_re = Regex::new(r#"dir_path\s*=\s*"([^"]+)""#).ok()?;
    let schema_id_re = Regex::new(r#"schema_id\s*=\s*"([^"]+)""#).ok()?;
    let description_re = Regex::new(r#"description\s*=\s*"([^"]+)""#).ok()?;
    let properties_re = Regex::new(r#"properties\s*=\s*"([^"]+)""#).ok()?;
    let base_true_re = Regex::new(r"\bbase\s*=\s*true\b").ok()?;
    let base_parent_re = Regex::new(r"\bbase\s*=\s*([A-Z]\w*)").ok()?;

    // Extract required fields
    let dir_path = dir_path_re.captures(attr_body)?.get(1)?.as_str().to_owned();
    let schema_id = schema_id_re
        .captures(attr_body)?
        .get(1)?
        .as_str()
        .to_owned();

    // Extract optional fields
    let description = description_re
        .captures(attr_body)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()));
    let properties = properties_re
        .captures(attr_body)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()));

    // Parse base attribute
    let base = if base_true_re.is_match(attr_body) {
        BaseAttr::IsBase
    } else if let Some(cap) = base_parent_re.captures(attr_body) {
        BaseAttr::Parent(cap.get(1)?.as_str().to_owned())
    } else {
        // base is required but not found
        return None;
    };

    Some(MacroAttrs {
        dir_path,
        schema_id,
        description,
        properties,
        base,
    })
}

/// Extract schema metadata from Rust source and generate JSON files
/// Returns a vector of (`schema_id`, `file_path`) tuples for each generated schema
fn extract_and_generate_schemas(
    content: &str,
    output_override: Option<&str>,
    sandbox_root: &Path,
    source_file: &Path,
) -> Result<Vec<(String, String)>> {
    // Match #[struct_to_gts_schema(...)] followed by struct definition
    // Captures: (1) attribute body, (2) struct name, (3) optional generics, (4) struct body or semicolon for unit structs
    let re = Regex::new(
        r"(?s)#\[struct_to_gts_schema\(([^)]+)\)\]\s*(?:#\[[^\]]+\]\s*)*(?:pub\s+)?struct\s+(\w+)(?:<([^>]+)>)?\s*(?:\{([^}]*)\}|;)",
    )?;

    // Pre-compile field regex outside the loop
    let field_re = Regex::new(r"(?m)^\s*(?:pub\s+)?(\w+)\s*:\s*([^,\n]+)")?;

    let mut results = Vec::new();

    for cap in re.captures_iter(content) {
        let attr_body = &cap[1];
        let struct_name = &cap[2];
        let _generics = cap.get(3).map(|m| m.as_str());
        let struct_body = cap.get(4).map_or("", |m| m.as_str());

        // Parse macro attributes
        let Some(attrs) = parse_macro_attrs(attr_body) else {
            continue;
        };

        let schema_file_rel = format!("{}/{}.schema.json", attrs.dir_path, attrs.schema_id);

        // Determine raw (unvalidated) output path
        let raw_output_path = if let Some(output_dir) = output_override {
            Path::new(output_dir).join(&schema_file_rel)
        } else {
            let source_dir = source_file.parent().unwrap_or(sandbox_root);
            source_dir.join(&schema_file_rel)
        };

        // Validate path against sandbox BEFORE any filesystem writes (fix validate-before-mkdir bug)
        let output_canonical = safe_canonicalize_nonexistent(&raw_output_path).map_err(|e| {
            anyhow::anyhow!(
                "Security error in {}:{} - dir_path '{}': {}",
                source_file.display(),
                struct_name,
                attrs.dir_path,
                e
            )
        })?;

        if !output_canonical.starts_with(sandbox_root) {
            bail!(
                "Security error in {}:{} - dir_path '{}' attempts to write outside sandbox boundary. \
                Resolved to: {}, but must be within: {}",
                source_file.display(),
                struct_name,
                attrs.dir_path,
                output_canonical.display(),
                sandbox_root.display()
            );
        }

        // Parse struct fields
        let mut field_types = HashMap::new();

        for field_cap in field_re.captures_iter(struct_body) {
            let field_name = &field_cap[1];
            let field_type = field_cap[2].trim().trim_end_matches(',');
            field_types.insert(field_name.to_owned(), field_type.to_owned());
        }

        // Build JSON schema
        let schema = build_json_schema(
            &attrs.schema_id,
            struct_name,
            attrs.description.as_deref(),
            attrs.properties.as_deref(),
            &attrs.base,
            &field_types,
        );

        // Create parent directories (only after sandbox validation passes)
        if let Some(parent) = raw_output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write schema file
        fs::write(&raw_output_path, serde_json::to_string_pretty(&schema)?)?;

        results.push((attrs.schema_id, raw_output_path.display().to_string()));
    }

    Ok(results)
}

/// Build a JSON Schema object from parsed metadata
fn build_json_schema(
    schema_id: &str,
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
                "$id": format!("gts://{schema_id}"),
                "$schema": "http://json-schema.org/draft-07/schema#",
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
            // The parent's schema_id is derived from this schema's ID by removing the last segment
            let parent_schema_id = derive_parent_schema_id(schema_id);

            let mut own_properties = json!({
                "properties": schema_properties
            });

            if !required.is_empty() {
                own_properties["required"] = json!(required);
            }

            let mut s = json!({
                "$id": format!("gts://{schema_id}"),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "title": format!("{struct_name} (extends {parent_name})"),
                "type": "object",
                "allOf": [
                    { "$ref": format!("gts://{parent_schema_id}") },
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

/// Derive parent schema ID from child schema ID
/// e.g., "gts.x.core.events.type.v1~x.core.audit.event.v1~" -> "gts.x.core.events.type.v1~"
fn derive_parent_schema_id(schema_id: &str) -> String {
    // Remove trailing ~ if present for processing
    let s = schema_id.trim_end_matches('~');

    // Find the last ~ and take everything before it, then add ~ back
    if let Some(pos) = s.rfind('~') {
        format!("{}~", &s[..pos])
    } else {
        // No parent segment found, this shouldn't happen for child types
        schema_id.to_owned()
    }
}

/// Convert Rust type string to JSON Schema type
/// Returns (`is_required`, `json_schema_value`)
///
/// This function inlines the actual schema definitions for GTS types (like `GtsInstanceId`)
/// to match what schemars generates, including custom extensions like `x-gts-ref`.
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
        // GtsSchemaId - use the canonical schema from the gts crate
        "GtsSchemaId" => GtsSchemaId::json_schema_value(),
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
    use crate::gen_common::{
        has_ignore_directive, is_in_auto_ignored_dir, matches_glob_pattern, should_exclude_path,
    };

    #[test]
    fn test_matches_glob_pattern() {
        assert!(matches_glob_pattern(
            "src/tests/compile_fail/test.rs",
            "compile_fail"
        ));
        assert!(matches_glob_pattern(
            "tests/compile_fail/test.rs",
            "compile_fail"
        ));
        assert!(matches_glob_pattern("src/tests/foo.rs", "tests/*"));
        assert!(matches_glob_pattern("src/examples/bar.rs", "examples/*"));
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
            dir_path = "schemas",
            base = true,
            schema_id = "gts.x.core.events.type.v1~",
            description = "Base event type"
        "#;

        let attrs = parse_macro_attrs(attr_body).unwrap();
        assert_eq!(attrs.dir_path, "schemas");
        assert_eq!(attrs.schema_id, "gts.x.core.events.type.v1~");
        assert_eq!(attrs.description.as_deref(), Some("Base event type"));
        assert!(matches!(attrs.base, BaseAttr::IsBase));
    }

    #[test]
    fn test_parse_macro_attrs_base_parent() {
        let attr_body = r#"
            dir_path = "schemas",
            base = BaseEventV1,
            schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~"
        "#;

        let attrs = parse_macro_attrs(attr_body).unwrap();
        assert_eq!(attrs.dir_path, "schemas");
        assert_eq!(
            attrs.schema_id,
            "gts.x.core.events.type.v1~x.core.audit.event.v1~"
        );
        assert!(matches!(attrs.base, BaseAttr::Parent(ref p) if p == "BaseEventV1"));
    }

    #[test]
    fn test_derive_parent_schema_id() {
        assert_eq!(
            derive_parent_schema_id("gts.x.core.events.type.v1~x.core.audit.event.v1~"),
            "gts.x.core.events.type.v1~"
        );
        assert_eq!(
            derive_parent_schema_id(
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

        // GTS types - now inlined with x-gts-ref extension
        let (req, schema) = rust_type_to_json_schema("GtsInstanceId");
        assert!(req);
        assert_eq!(schema["type"], "string");
        assert_eq!(schema["format"], "gts-instance-id");
        assert_eq!(schema["x-gts-ref"], "gts.*");

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
            "gts.x.test.base.v1~",
            "BaseStruct",
            Some("A base test struct"),
            None, // Include all properties
            &BaseAttr::IsBase,
            &field_types,
        );

        assert_eq!(schema["$id"], "gts://gts.x.test.base.v1~");
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
            "gts.x.test.base.v1~x.test.child.v1~",
            "ChildStruct",
            Some("A child test struct"),
            None,
            &BaseAttr::Parent("BaseStruct".to_owned()),
            &field_types,
        );

        assert_eq!(schema["$id"], "gts://gts.x.test.base.v1~x.test.child.v1~");
        assert_eq!(schema["title"], "ChildStruct (extends BaseStruct)");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["description"], "A child test struct");

        // Check allOf structure
        let all_of = schema["allOf"].as_array().unwrap();
        assert_eq!(all_of.len(), 2);

        // First element should be $ref to parent
        assert_eq!(all_of[0]["$ref"], "gts://gts.x.test.base.v1~");

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
            dir_path = "schemas",
            base = true,
            schema_id = "gts.x.custom.id.v1~"
        "#;

        let attrs = parse_macro_attrs(attr_body).unwrap();
        assert_eq!(attrs.schema_id, "gts.x.custom.id.v1~");
        assert_eq!(attrs.dir_path, "schemas");
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
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.test.person.v1~",
    description = "A test person struct"
)]
pub struct Person {
    pub id: GtsInstanceId,
    pub name: String,
    pub age: i32,
}
"#;
        fs::write(&test_file, content).unwrap();

        // Call extract_and_generate_schemas
        let results = extract_and_generate_schemas(
            content,
            Some(temp_path.to_str().unwrap()),
            &temp_path,
            &test_file,
        )
        .unwrap();

        // Verify results
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "gts.x.test.person.v1~");

        // Verify schema file was created
        let schema_path = Path::new(&results[0].1);
        assert!(schema_path.exists());

        // Verify schema content
        let schema_content = fs::read_to_string(schema_path).unwrap();
        let schema: serde_json::Value = serde_json::from_str(&schema_content).unwrap();

        assert_eq!(schema["$id"], "gts://gts.x.test.person.v1~");
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
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.test.base.v1~",
    description = "Base event"
)]
pub struct BaseEvent {
    pub id: GtsInstanceId,
    pub timestamp: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEvent,
    schema_id = "gts.x.test.base.v1~x.test.child.v1~",
    description = "Child event"
)]
pub struct ChildEvent {
    pub event_type: String,
    pub data: String,
}
"#;
        fs::write(&test_file, content).unwrap();

        // Call extract_and_generate_schemas
        let results = extract_and_generate_schemas(
            content,
            Some(temp_path.to_str().unwrap()),
            &temp_path,
            &test_file,
        )
        .unwrap();

        // Verify results - should have 2 schemas
        assert_eq!(results.len(), 2);

        // Find base and child schemas
        let base_result = results
            .iter()
            .find(|(id, _)| id == "gts.x.test.base.v1~")
            .unwrap();
        let child_result = results
            .iter()
            .find(|(id, _)| id == "gts.x.test.base.v1~x.test.child.v1~")
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
        assert_eq!(all_of[0]["$ref"], "gts://gts.x.test.base.v1~");
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
        let attr1 = r#"dir_path = "schemas", base = true, schema_id = "gts.x.test.v1~""#;
        let result1 = parse_macro_attrs(attr1).unwrap();
        assert_eq!(result1.dir_path, "schemas");
        assert_eq!(result1.schema_id, "gts.x.test.v1~");
        assert!(matches!(result1.base, BaseAttr::IsBase));

        // With base parent
        let attr2 = r#"dir_path = "schemas", base = ParentStruct, schema_id = "gts.x.test.v1~""#;
        let result2 = parse_macro_attrs(attr2).unwrap();
        assert!(matches!(result2.base, BaseAttr::Parent(ref p) if p == "ParentStruct"));

        // Missing field
        assert!(parse_macro_attrs(r#"dir_path = "schemas""#).is_none());

        // Malformed
        assert!(parse_macro_attrs(r"invalid syntax here").is_none());
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
