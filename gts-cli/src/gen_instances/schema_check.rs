use anyhow::{Result, bail};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

use super::parser::ParsedInstance;
use super::writer::build_instance_value;

/// GTS URI prefix used in schema `$id` fields.
const GTS_URI_PREFIX: &str = "gts://";

/// Registry of discovered GTS schemas, keyed by `$id` URI (e.g. `gts://gts.x.core.events.topic.v1~`).
///
/// Implements `jsonschema::Retrieve` so the `jsonschema` crate can resolve `$ref` URIs
/// pointing to other GTS schemas during validation of inherited (allOf) schemas.
#[derive(Clone)]
struct SchemaRegistry {
    schemas: Arc<HashMap<String, Value>>,
}

impl SchemaRegistry {
    /// Walk `root` recursively for `*.schema.json` files and index by their `$id` field.
    fn discover(root: &Path) -> Self {
        let mut schemas = HashMap::new();

        for entry in WalkDir::new(root).follow_links(true).max_depth(64) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("warning: skipping unreadable path during schema discovery: {e}");
                    continue;
                }
            };
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if !name.ends_with(".schema.json") {
                continue;
            }

            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "warning: skipping unreadable schema file {}: {e}",
                        path.display()
                    );
                    continue;
                }
            };
            let value = match serde_json::from_str::<Value>(&content) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "warning: skipping malformed schema file {}: {e}",
                        path.display()
                    );
                    continue;
                }
            };
            if let Some(id) = value.get("$id").and_then(Value::as_str) {
                schemas.insert(id.to_owned(), value);
            } else {
                eprintln!(
                    "warning: schema file {} has no '$id' field -- skipping",
                    path.display()
                );
            }
        }

        Self {
            schemas: Arc::new(schemas),
        }
    }

    /// Look up a schema by its full `$id` URI.
    fn get(&self, id_uri: &str) -> Option<&Value> {
        self.schemas.get(id_uri)
    }

    fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }
}

impl jsonschema::Retrieve for SchemaRegistry {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> std::result::Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let uri_str = uri.as_str();

        if !uri_str.starts_with(GTS_URI_PREFIX) {
            return Err(format!("Unknown URI scheme: {uri_str}").into());
        }

        self.schemas
            .get(uri_str)
            .cloned()
            .ok_or_else(|| format!("Schema not found: {uri_str}").into())
    }
}

/// Cached result of compiling a JSON Schema validator for a given schema URI.
enum ValidatorCacheEntry {
    /// Successfully compiled validator, ready for reuse.
    Valid(jsonschema::Validator),
    /// Schema URI was not found in the registry.
    NotFound,
    /// Schema was found but failed to compile.
    CompileError(String),
}

/// Validate all parsed instances against their parent GTS schemas.
///
/// Schema discovery walks `sandbox_root` for `*.schema.json` files.
///
/// - If no schemas are found on disk the check is silently skipped (supports `--mode instances`).
/// - If the schema for a specific instance is not found a warning is printed and that instance is skipped.
/// - If validation fails for any instance, **all** errors are collected and returned as a single hard error.
///
/// # Errors
/// Returns an error if one or more instances fail schema validation.
pub fn validate_instances_against_schemas(
    instances: &[ParsedInstance],
    sandbox_root: &Path,
) -> Result<()> {
    let registry = SchemaRegistry::discover(sandbox_root);

    if registry.is_empty() {
        if !instances.is_empty() {
            println!(
                "  Schema validation: skipped (no *.schema.json files found in {})",
                sandbox_root.display()
            );
        }
        return Ok(());
    }

    // Cache compiled validators per schema URI to avoid re-compiling (and avoid
    // cloning the registry) for every instance that shares the same schema.
    let mut cache: HashMap<String, ValidatorCacheEntry> = HashMap::new();
    let mut errors: Vec<String> = Vec::new();

    for inst in instances {
        let schema_uri = format!("{GTS_URI_PREFIX}{}", inst.attrs.schema_id);

        // Track whether this is the first time we see this schema URI so we
        // can emit warnings only once per missing/broken schema.
        let is_new = !cache.contains_key(&schema_uri);

        // Build (or retrieve cached) validator for this schema.
        let entry = cache.entry(schema_uri.clone()).or_insert_with(|| {
            let Some(schema) = registry.get(&schema_uri) else {
                return ValidatorCacheEntry::NotFound;
            };
            match jsonschema::options()
                .with_retriever(registry.clone())
                .build(schema)
            {
                Ok(v) => ValidatorCacheEntry::Valid(v),
                Err(e) => ValidatorCacheEntry::CompileError(e.to_string()),
            }
        });

        let validator = match entry {
            ValidatorCacheEntry::Valid(v) => v,
            ValidatorCacheEntry::NotFound => {
                if is_new {
                    eprintln!(
                        "warning: schema '{}' not found on disk -- skipping validation",
                        inst.attrs.schema_id
                    );
                }
                continue;
            }
            ValidatorCacheEntry::CompileError(reason) => {
                errors.push(format!(
                    "{}:{}: Failed to compile schema '{}' for instance '{}': {}",
                    inst.source_file, inst.line, inst.attrs.schema_id, inst.attrs.id, reason
                ));
                continue;
            }
        };

        // Build the complete instance JSON with injected "id".
        let complete_instance = match build_instance_value(inst) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!(
                    "{}:{}: Failed to build instance JSON for '{}': {}",
                    inst.source_file, inst.line, inst.attrs.id, e
                ));
                continue;
            }
        };

        // Collect all validation errors for this instance.
        let validation_errors: Vec<String> = validator
            .iter_errors(&complete_instance)
            .map(|e| {
                let path = e.instance_path().to_string();
                if path.is_empty() {
                    format!("  - {e}")
                } else {
                    format!("  - {path}: {e}")
                }
            })
            .collect();

        if !validation_errors.is_empty() {
            errors.push(format!(
                "{}:{}: Instance '{}' does not conform to schema '{}':\n{}",
                inst.source_file,
                inst.line,
                inst.attrs.id,
                inst.attrs.schema_id,
                validation_errors.join("\n")
            ));
        }
    }

    if !errors.is_empty() {
        errors.sort();
        for err in &errors {
            eprintln!("error: {err}");
        }
        bail!(
            "Instance generation failed: {} schema validation error(s)",
            errors.len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen_instances::attrs::InstanceAttrs;
    use tempfile::TempDir;

    fn make_inst(dir_path: &str, id: &str, json_body: &str, source_file: &str) -> ParsedInstance {
        let tilde_pos = id.find('~').unwrap();
        ParsedInstance {
            attrs: InstanceAttrs {
                dir_path: dir_path.to_owned(),
                id: id.to_owned(),
                schema_id: id[..=tilde_pos].to_owned(),
                instance_segment: id[tilde_pos + 1..].to_owned(),
            },
            json_body: json_body.to_owned(),
            source_file: source_file.to_owned(),
            line: 1,
        }
    }

    fn write_schema(dir: &Path, schema_id: &str, schema: &Value) {
        let name = format!("{schema_id}.schema.json");
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join(name),
            serde_json::to_string_pretty(schema).unwrap(),
        )
        .unwrap();
    }

    fn base_schema(schema_id: &str, properties: &[&str]) -> Value {
        let mut props = serde_json::Map::new();
        // Always include "id" property (GtsInstanceId)
        props.insert(
            "id".to_owned(),
            serde_json::json!({ "type": "string", "format": "gts-instance-id" }),
        );
        for p in properties {
            props.insert((*p).to_owned(), serde_json::json!({ "type": "string" }));
        }
        let mut required: Vec<&str> = vec!["id"];
        required.extend_from_slice(properties);
        required.sort_unstable();
        serde_json::json!({
            "$id": format!("{GTS_URI_PREFIX}{schema_id}"),
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "additionalProperties": false,
            "properties": props,
            "required": required
        })
    }

    #[test]
    fn valid_instance_passes() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();

        let schema_dir = root.join("schemas");
        let schema = base_schema("gts.x.test.v1~", &["name"]);
        write_schema(&schema_dir, "gts.x.test.v1~", &schema);

        let inst = make_inst(
            "instances",
            "gts.x.test.v1~x.foo.v1",
            r#"{"name": "foo"}"#,
            root.join("src.rs").to_str().unwrap(),
        );

        let result = validate_instances_against_schemas(&[inst], &root);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn missing_required_field_fails() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();

        let schema_dir = root.join("schemas");
        let schema = base_schema("gts.x.test.v1~", &["name", "vendor"]);
        write_schema(&schema_dir, "gts.x.test.v1~", &schema);

        // Instance missing "vendor"
        let inst = make_inst(
            "instances",
            "gts.x.test.v1~x.foo.v1",
            r#"{"name": "foo"}"#,
            root.join("src.rs").to_str().unwrap(),
        );

        let result = validate_instances_against_schemas(&[inst], &root);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("schema validation error"), "Got: {msg}");
    }

    #[test]
    fn extra_field_with_additional_properties_false_fails() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();

        let schema_dir = root.join("schemas");
        let schema = base_schema("gts.x.test.v1~", &["name"]);
        write_schema(&schema_dir, "gts.x.test.v1~", &schema);

        // Instance has extra field "extra"
        let inst = make_inst(
            "instances",
            "gts.x.test.v1~x.foo.v1",
            r#"{"name": "foo", "extra": "bar"}"#,
            root.join("src.rs").to_str().unwrap(),
        );

        let result = validate_instances_against_schemas(&[inst], &root);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("schema validation error"), "Got: {msg}");
    }

    #[test]
    fn missing_schema_warns_not_errors() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();

        // Write a schema for a DIFFERENT schema_id so registry is non-empty
        let schema_dir = root.join("schemas");
        let schema = base_schema("gts.x.other.v1~", &["name"]);
        write_schema(&schema_dir, "gts.x.other.v1~", &schema);

        let inst = make_inst(
            "instances",
            "gts.x.test.v1~x.foo.v1",
            r#"{"name": "foo"}"#,
            root.join("src.rs").to_str().unwrap(),
        );

        // Should warn but NOT error
        let result = validate_instances_against_schemas(&[inst], &root);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn no_schemas_on_disk_skips_silently() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();

        let inst = make_inst(
            "instances",
            "gts.x.test.v1~x.foo.v1",
            r#"{"name": "foo"}"#,
            root.join("src.rs").to_str().unwrap(),
        );

        let result = validate_instances_against_schemas(&[inst], &root);
        assert!(result.is_ok(), "{result:?}");
    }

    /// Build a child schema that uses `allOf` + `$ref` to inherit from a parent.
    fn child_schema(
        schema_id: &str,
        parent_schema_id: &str,
        own_properties: &[(&str, &str)],
    ) -> Value {
        let mut own_props = serde_json::Map::new();
        let mut required = Vec::new();
        for (name, ty) in own_properties {
            own_props.insert((*name).to_owned(), serde_json::json!({ "type": *ty }));
            required.push(*name);
        }
        required.sort_unstable();
        serde_json::json!({
            "$id": format!("{GTS_URI_PREFIX}{schema_id}"),
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "allOf": [
                { "$ref": format!("{GTS_URI_PREFIX}{parent_schema_id}") },
                {
                    "type": "object",
                    "properties": own_props,
                    "required": required
                }
            ]
        })
    }

    /// Build a base schema **without** `additionalProperties: false`.
    /// Parent schemas used for `allOf` inheritance must be open so that child
    /// properties are not rejected by the parent's constraint.
    fn base_schema_open(schema_id: &str, properties: &[&str]) -> Value {
        let mut props = serde_json::Map::new();
        props.insert(
            "id".to_owned(),
            serde_json::json!({ "type": "string", "format": "gts-instance-id" }),
        );
        for p in properties {
            props.insert((*p).to_owned(), serde_json::json!({ "type": "string" }));
        }
        let mut required: Vec<&str> = vec!["id"];
        required.extend_from_slice(properties);
        required.sort_unstable();
        serde_json::json!({
            "$id": format!("{GTS_URI_PREFIX}{schema_id}"),
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": props,
            "required": required
        })
    }

    #[test]
    fn allof_ref_inheritance_valid_passes() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let schema_dir = root.join("schemas");

        // Parent schema (open — no additionalProperties: false)
        let parent = base_schema_open("gts.x.test.v1~", &["name"]);
        write_schema(&schema_dir, "gts.x.test.v1~", &parent);

        // Child schema: inherits parent via allOf + $ref, adds own "vendor"
        let child = child_schema(
            "gts.x.test.v1~x.child.v1~",
            "gts.x.test.v1~",
            &[("vendor", "string")],
        );
        write_schema(&schema_dir, "gts.x.test.v1~x.child.v1~", &child);

        // Instance satisfies both parent ("id", "name") and child ("vendor")
        let inst = make_inst(
            "instances",
            "gts.x.test.v1~x.child.v1~x.foo.v1",
            r#"{"name": "foo", "vendor": "acme"}"#,
            root.join("src.rs").to_str().unwrap(),
        );

        let result = validate_instances_against_schemas(&[inst], &root);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn allof_ref_inheritance_missing_parent_field_fails() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let schema_dir = root.join("schemas");

        // Parent schema (open)
        let parent = base_schema_open("gts.x.test.v1~", &["name"]);
        write_schema(&schema_dir, "gts.x.test.v1~", &parent);

        // Child schema: inherits parent, adds own "vendor"
        let child = child_schema(
            "gts.x.test.v1~x.child.v1~",
            "gts.x.test.v1~",
            &[("vendor", "string")],
        );
        write_schema(&schema_dir, "gts.x.test.v1~x.child.v1~", &child);

        // Instance provides "vendor" but missing parent-required "name"
        let inst = make_inst(
            "instances",
            "gts.x.test.v1~x.child.v1~x.foo.v1",
            r#"{"vendor": "acme"}"#,
            root.join("src.rs").to_str().unwrap(),
        );

        let result = validate_instances_against_schemas(&[inst], &root);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("schema validation error"), "Got: {msg}");
    }

    #[test]
    fn wrong_type_fails() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();

        let schema_dir = root.join("schemas");
        // Schema requires "count" to be integer
        let mut schema = base_schema("gts.x.test.v1~", &[]);
        schema["properties"]["count"] = serde_json::json!({ "type": "integer" });
        schema["required"] = serde_json::json!(["count", "id"]);
        write_schema(&schema_dir, "gts.x.test.v1~", &schema);

        // Instance provides "count" as a string
        let inst = make_inst(
            "instances",
            "gts.x.test.v1~x.foo.v1",
            r#"{"count": "not-a-number"}"#,
            root.join("src.rs").to_str().unwrap(),
        );

        let result = validate_instances_against_schemas(&[inst], &root);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("schema validation error"), "Got: {msg}");
    }
}
