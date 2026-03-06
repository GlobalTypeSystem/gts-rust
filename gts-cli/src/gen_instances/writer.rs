use anyhow::{Result, bail};
use std::fs;
use std::path::Path;

use crate::gen_common::safe_canonicalize_nonexistent;

use super::parser::ParsedInstance;

/// Generate the instance JSON file for a single parsed annotation.
///
/// Validates the output path against the sandbox boundary **before** any
/// filesystem mutations (validate-before-mkdir policy).
///
/// Injects the `"id"` field (composed as `schema_id + instance_segment`) into
/// the JSON object before writing.
///
/// Returns the written file path as a `String` on success.
///
/// # Errors
/// Returns an error if the output path escapes the sandbox or if the file cannot be written.
pub fn generate_single_instance(
    inst: &ParsedInstance,
    output: Option<&str>,
    sandbox_root: &Path,
) -> Result<String> {
    let composed = format!("{}{}", inst.attrs.schema_id, inst.attrs.instance_segment);
    let file_rel =
        std::path::Path::new(&inst.attrs.dir_path).join(format!("{composed}.instance.json"));

    let raw_output_path = if let Some(od) = output {
        Path::new(od).join(&file_rel)
    } else {
        let src_dir = Path::new(&inst.source_file)
            .parent()
            .unwrap_or(sandbox_root);
        src_dir.join(&file_rel)
    };

    // Validate sandbox boundary BEFORE any filesystem writes
    let output_canonical = safe_canonicalize_nonexistent(&raw_output_path).map_err(|e| {
        anyhow::anyhow!(
            "Security error - dir_path '{}' in {}: {}",
            inst.attrs.dir_path,
            inst.source_file,
            e
        )
    })?;

    if !output_canonical.starts_with(sandbox_root) {
        bail!(
            "Security error in {} - dir_path '{}' escapes sandbox boundary. \
             Resolved to: {}, but must be within: {}",
            inst.source_file,
            inst.attrs.dir_path,
            output_canonical.display(),
            sandbox_root.display()
        );
    }

    // Build JSON with injected "id" field
    let mut obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&inst.json_body)?;
    obj.insert("id".to_owned(), serde_json::Value::String(composed));
    let output_value = serde_json::Value::Object(obj);

    // Create parent directories only after sandbox validation passes
    if let Some(parent) = raw_output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &raw_output_path,
        serde_json::to_string_pretty(&output_value)?,
    )?;

    Ok(raw_output_path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen_instances::attrs::InstanceAttrs;
    use tempfile::TempDir;

    fn make_inst(
        dir_path: &str,
        schema_id: &str,
        instance_segment: &str,
        json_body: &str,
        source_file: &str,
    ) -> ParsedInstance {
        ParsedInstance {
            attrs: InstanceAttrs {
                dir_path: dir_path.to_owned(),
                schema_id: schema_id.to_owned(),
                instance_segment: instance_segment.to_owned(),
            },
            json_body: json_body.to_owned(),
            source_file: source_file.to_owned(),
            line: 1,
        }
    }

    #[test]
    fn test_generates_file_with_id_injected() {
        let temp = TempDir::new().unwrap();
        let sandbox = temp.path().canonicalize().unwrap();
        let src = sandbox.join("module.rs");

        let inst = make_inst(
            "instances",
            "gts.x.core.events.topic.v1~",
            "x.commerce._.orders.v1.0",
            r#"{"name": "orders", "partitions": 16}"#,
            src.to_str().unwrap(),
        );

        let result = generate_single_instance(&inst, Some(sandbox.to_str().unwrap()), &sandbox);
        assert!(result.is_ok(), "{result:?}");

        let expected = sandbox
            .join("instances")
            .join("gts.x.core.events.topic.v1~x.commerce._.orders.v1.0.instance.json");
        assert!(expected.exists());

        let content: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&expected).unwrap()).unwrap();
        assert_eq!(
            content["id"],
            "gts.x.core.events.topic.v1~x.commerce._.orders.v1.0"
        );
        assert_eq!(content["name"], "orders");
        assert_eq!(content["partitions"], 16);
    }

    #[test]
    fn test_sandbox_escape_rejected() {
        let temp = TempDir::new().unwrap();
        let sandbox = temp.path().canonicalize().unwrap();
        let src = sandbox.join("module.rs");

        let inst = make_inst(
            "../../etc",
            "gts.x.core.events.topic.v1~",
            "x.commerce._.orders.v1.0",
            r#"{"name": "x"}"#,
            src.to_str().unwrap(),
        );

        let result = generate_single_instance(&inst, Some(sandbox.to_str().unwrap()), &sandbox);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Security error") || msg.contains("sandbox"),
            "Unexpected error: {msg}"
        );
    }

    #[test]
    fn test_output_uses_source_dir_when_no_output_override() {
        let temp = TempDir::new().unwrap();
        let sandbox = temp.path().canonicalize().unwrap();
        let src = sandbox.join("subdir").join("module.rs");
        fs::create_dir_all(src.parent().unwrap()).unwrap();

        let inst = make_inst(
            "instances",
            "gts.x.core.events.topic.v1~",
            "x.commerce._.orders.v1.0",
            r#"{"name": "x"}"#,
            src.to_str().unwrap(),
        );

        let result = generate_single_instance(&inst, None, &sandbox);
        assert!(result.is_ok(), "{result:?}");

        let expected = sandbox
            .join("subdir")
            .join("instances")
            .join("gts.x.core.events.topic.v1~x.commerce._.orders.v1.0.instance.json");
        assert!(expected.exists());
    }
}
