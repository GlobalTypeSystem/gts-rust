use gts::gts::GtsSchemaId;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use gts::gts_schema_for;
use serde::{Deserialize, Serialize};

const SEPARATOR: &str =
    "================================================================================";

// Include test structs to access their generated constants
mod test_structs {
    use super::{Deserialize, GtsSchemaId, Serialize};
    use gts_macros::struct_to_gts_schema;
    use schemars::JsonSchema;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        schema_id = "gts.x.core.events.type.v1~",
        description = "Base event type definition",
        properties = "event_type,id,tenant_id,sequence_id,payload"
    )]
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    pub struct BaseEventV1<P> {
        #[serde(rename = "type")]
        pub event_type: GtsSchemaId,
        pub id: uuid::Uuid,
        pub tenant_id: uuid::Uuid,
        pub sequence_id: u64,
        pub payload: P,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = BaseEventV1,
        schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
        description = "Audit event with user context",
        properties = "user_agent,user_id,ip_address,data"
    )]
    #[derive(Debug, JsonSchema)]
    pub struct AuditPayloadV1<D> {
        pub user_agent: String,
        pub user_id: uuid::Uuid,
        pub ip_address: String,
        pub data: D,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = AuditPayloadV1,
        schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
        description = "Order placement audit event",
        properties = "order_id,product_id"
    )]
    #[derive(Debug, JsonSchema)]
    pub struct PlaceOrderDataV1<E> {
        pub order_id: uuid::Uuid,
        pub product_id: uuid::Uuid,
        pub last: E,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = PlaceOrderDataV1,
        schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~x.marketplace.order_purchase.payload.v1~",
        description = "Order placement audit event",
        properties = "order_id"
    )]
    #[derive(Debug, JsonSchema)]
    pub struct PlaceOrderDataPayloadV1 {
        pub order_id: uuid::Uuid,
    }
}

/// GTS Macros CLI - Demo tool for GTS schema introspection and inheritance
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory to dump all schemas and instances to.
    /// Schemas are saved as `{schema_id}.schema.json` (without `gts://` prefix).
    /// Instances are saved as `{instance_id}.json`.
    #[arg(long, value_name = "DIR")]
    dump: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Some(dir) = args.dump {
        dump_to_directory(&dir)?;
    } else {
        run_demo()?;
    }

    Ok(())
}

/// Helper function to save a schema to a file
fn save_schema(
    dir: &std::path::Path,
    schema: &serde_json::Value,
    schema_id: &str,
) -> anyhow::Result<()> {
    let schema_path = dir.join(format!("{schema_id}.schema.json"));
    std::fs::write(&schema_path, serde_json::to_string_pretty(schema)? + "\n")?;
    println!("Saved schema: {}", schema_path.display());
    Ok(())
}

/// Helper function to create a sample event with fixed UUIDs
fn create_sample_event() -> anyhow::Result<
    test_structs::BaseEventV1<
        test_structs::AuditPayloadV1<
            test_structs::PlaceOrderDataV1<test_structs::PlaceOrderDataPayloadV1>,
        >,
    >,
> {
    Ok(test_structs::BaseEventV1 {
        event_type: test_structs::PlaceOrderDataPayloadV1::gts_schema_id().clone(),
        id: uuid::Uuid::parse_str("d1b475cf-8155-45c3-ab75-b245bd38116b")?,
        tenant_id: uuid::Uuid::parse_str("0a0bd7c0-e8ef-4d7d-b841-645715e25d20")?,
        sequence_id: 42,
        payload: test_structs::AuditPayloadV1 {
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)".to_owned(),
            user_id: uuid::Uuid::parse_str("5d4e4360-aa4d-4614-9aec-7779ef9177c1")?,
            ip_address: "192.168.1.100".to_owned(),
            data: test_structs::PlaceOrderDataV1 {
                order_id: uuid::Uuid::parse_str("d2e9495b-834f-4f46-a404-cd70801beeee")?,
                product_id: uuid::Uuid::parse_str("13121f11-f30e-49fc-a4c4-a45267ce96e1")?,
                last: test_structs::PlaceOrderDataPayloadV1 {
                    order_id: uuid::Uuid::parse_str("dcc12039-0119-4417-b3ed-90a4e91f9557")?,
                },
            },
        },
    })
}

/// Dump all schemas and instances to the specified directory
fn dump_to_directory(dir: &Path) -> anyhow::Result<()> {
    use gts::GtsSchema;

    // Create directory if it doesn't exist
    std::fs::create_dir_all(dir)?;

    // Create a sample instance with fixed UUIDs for reproducibility
    let event = create_sample_event()?;

    // Save instance with its ID as filename
    let instance_id = event.id.to_string();
    let instance_path = dir.join(format!("{instance_id}.json"));
    let instance_json = serde_json::to_string_pretty(&event)? + "\n";
    std::fs::write(&instance_path, instance_json)?;
    println!("Saved instance: {}", instance_path.display());

    // Save schemas using gts_schema_for! macro
    // Schema 1: BaseEventV1 (base type)
    let schema1 = gts_schema_for!(test_structs::BaseEventV1<()>);
    save_schema(dir, &schema1, test_structs::BaseEventV1::<()>::SCHEMA_ID)?;

    // Schema 2: BaseEventV1<AuditPayloadV1>
    let schema2 = gts_schema_for!(test_structs::BaseEventV1<test_structs::AuditPayloadV1<()>>);
    save_schema(dir, &schema2, test_structs::AuditPayloadV1::<()>::SCHEMA_ID)?;

    // Schema 3: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>
    let schema3 = gts_schema_for!(
        test_structs::BaseEventV1<test_structs::AuditPayloadV1<test_structs::PlaceOrderDataV1<()>>>
    );
    save_schema(
        dir,
        &schema3,
        test_structs::PlaceOrderDataV1::<()>::SCHEMA_ID,
    )?;

    // Schema 4: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1<PlaceOrderDataPayloadV1>>>
    let schema4 = gts_schema_for!(
        test_structs::BaseEventV1<
            test_structs::AuditPayloadV1<
                test_structs::PlaceOrderDataV1<test_structs::PlaceOrderDataPayloadV1>,
            >,
        >
    );
    save_schema(
        dir,
        &schema4,
        test_structs::PlaceOrderDataPayloadV1::SCHEMA_ID,
    )?;

    // Generate validate.sh script
    // The main schema is the innermost (most derived) schema
    // Referenced schemas are listed from most derived to base (excluding the main schema)
    let schema1_id = test_structs::BaseEventV1::<()>::SCHEMA_ID;
    let schema2_id = test_structs::AuditPayloadV1::<()>::SCHEMA_ID;
    let schema3_id = test_structs::PlaceOrderDataV1::<()>::SCHEMA_ID;
    let schema4_id = test_structs::PlaceOrderDataPayloadV1::SCHEMA_ID;
    let schema_ids = [schema4_id, schema3_id, schema2_id, schema1_id];

    let mut validate_script = String::from("#!/bin/bash\n\n");
    // Get the directory where this script is located, so it works from any location
    validate_script.push_str("SCRIPT_DIR=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\n\n");
    validate_script.push_str("npx ajv-cli validate \\\n");
    validate_script.push_str("  --spec=draft7 \\\n");
    validate_script.push_str("  -c ajv-formats \\\n");
    validate_script.push_str("  --strict=false \\\n");

    // Main schema (-s): the innermost/most derived schema
    let main_schema_id = schema_ids[0];
    writeln!(
        validate_script,
        "  -s \"$SCRIPT_DIR/{main_schema_id}.schema.json\" \\"
    )?;

    // Referenced schemas (-r): from most derived to base, excluding the main schema
    for schema_id in &schema_ids[1..] {
        writeln!(
            validate_script,
            "  -r \"$SCRIPT_DIR/{schema_id}.schema.json\" \\"
        )?;
    }

    // Data file (-d): the instance
    writeln!(validate_script, "  -d \"$SCRIPT_DIR/{instance_id}.json\"")?;

    let validate_path = dir.join("validate.sh");
    std::fs::write(&validate_path, validate_script)?;

    // Make the script executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&validate_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&validate_path, perms)?;
    }

    println!("Saved validate script: {}", validate_path.display());

    println!("\nDone! All files saved to: {}", dir.display());

    Ok(())
}

/// Run the original demo output
fn run_demo() -> anyhow::Result<()> {
    println!("{SEPARATOR}");
    println!("GTS Macros Demo - Schema Inheritance Chain");
    println!("{SEPARATOR}\n");

    // Print instance examples
    print_instances()?;

    // Print gts_schema_for! macro output
    print_gts_schema_for()?;

    Ok(())
}

fn print_instances() -> anyhow::Result<()> {
    println!("INSTANCE EXAMPLES");
    println!("-----------------\n");

    // Create a complete inheritance chain instance
    let event = create_sample_event()?;

    println!("Complete Inheritance Chain Instance:");
    println!("```json");
    println!("{}", serde_json::to_string_pretty(&event)?);
    println!("```\n");

    println!("Instance Components:");
    println!("  * BaseEventV1 (root) - Contains event metadata and generic payload");
    println!("  * AuditPayloadV1 (inherits BaseEventV1) - Adds user context");
    println!("  * PlaceOrderDataV1 (inherits AuditPayloadV1) - Adds order details");

    Ok(())
}

fn print_gts_schema_for() -> anyhow::Result<()> {
    println!("GTS schemas and instances examples");
    println!("----------------------------------\n");

    println!("gts_schema_for!(BaseEventV1):");
    println!("```json");
    let schema = gts_schema_for!(test_structs::BaseEventV1<()>);
    println!("{}", serde_json::to_string_pretty(&schema)?);
    println!("```\n");

    println!("gts_schema_for!(BaseEventV1<AuditPayloadV1>):");
    println!("```json");
    let schema = gts_schema_for!(test_structs::BaseEventV1<test_structs::AuditPayloadV1<()>>);
    println!("{}", serde_json::to_string_pretty(&schema)?);
    println!("```\n");

    println!("gts_schema_for!(BaseEventV1<AuditPayloadV1<PlaceOrderDataV1<()>>>):");
    println!("```json");
    let schema = gts_schema_for!(
        test_structs::BaseEventV1<test_structs::AuditPayloadV1<test_structs::PlaceOrderDataV1<()>>>
    );
    println!("{}", serde_json::to_string_pretty(&schema)?);
    println!("```\n");

    println!(
        "gts_schema_for!(BaseEventV1<AuditPayloadV1<PlaceOrderDataV1<PlaceOrderDataPayloadV1>>>):"
    );
    println!("```json");
    let schema = gts_schema_for!(
        test_structs::BaseEventV1<
            test_structs::AuditPayloadV1<
                test_structs::PlaceOrderDataV1<test_structs::PlaceOrderDataPayloadV1>,
            >,
        >
    );
    println!("{}", serde_json::to_string_pretty(&schema)?);
    println!("```\n");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_create_sample_event() {
        let event = create_sample_event().unwrap();

        // Verify fixed UUIDs
        assert_eq!(event.id.to_string(), "d1b475cf-8155-45c3-ab75-b245bd38116b");
        assert_eq!(
            event.tenant_id.to_string(),
            "0a0bd7c0-e8ef-4d7d-b841-645715e25d20"
        );
        assert_eq!(event.sequence_id, 42);

        // Verify nested payload
        assert_eq!(
            event.payload.user_id.to_string(),
            "5d4e4360-aa4d-4614-9aec-7779ef9177c1"
        );
        assert_eq!(event.payload.ip_address, "192.168.1.100");
        assert!(event.payload.user_agent.contains("Mozilla"));
    }

    #[test]
    #[ignore = "failing on windows as file names are invalid with certain characters in schema IDs"]
    fn test_save_schema() {
        let temp_dir = TempDir::new().unwrap();
        let schema = serde_json::json!({
            "$id": "gts://test:schema:v1",
            "type": "object"
        });

        save_schema(temp_dir.path(), &schema, "test:schema:v1").unwrap();

        let schema_path = temp_dir.path().join("test:schema:v1.schema.json");
        assert!(schema_path.exists());

        let content = fs::read_to_string(&schema_path).unwrap();
        assert!(content.contains("test:schema:v1"));
        assert!(content.contains("\"type\": \"object\""));
    }

    #[test]
    fn test_dump_to_directory() {
        let temp_dir = TempDir::new().unwrap();

        dump_to_directory(temp_dir.path()).unwrap();

        // Verify instance file was created
        let instance_files: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect();
        assert!(
            !instance_files.is_empty(),
            "Should have at least one instance file"
        );

        // Verify schema files were created
        let schema_files: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".schema.json"))
            })
            .collect();
        assert_eq!(schema_files.len(), 4, "Should have 4 schema files");

        // Verify validate.sh was created
        let validate_script = temp_dir.path().join("validate.sh");
        assert!(validate_script.exists());

        let script_content = fs::read_to_string(&validate_script).unwrap();
        assert!(script_content.contains("#!/bin/bash"));
        assert!(script_content.contains("npx ajv-cli validate"));
        assert!(script_content.contains("SCRIPT_DIR"));
    }

    #[test]
    fn test_dump_to_directory_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let nested_dir = temp_dir.path().join("nested").join("schemas");

        // Directory doesn't exist yet
        assert!(!nested_dir.exists());

        dump_to_directory(&nested_dir).unwrap();

        // Directory was created
        assert!(nested_dir.exists());
        assert!(nested_dir.is_dir());
    }

    #[test]
    fn test_validate_script_permissions_unix() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let temp_dir = TempDir::new().unwrap();
            dump_to_directory(temp_dir.path()).unwrap();

            let validate_script = temp_dir.path().join("validate.sh");
            let metadata = fs::metadata(&validate_script).unwrap();
            let permissions = metadata.permissions();

            // Check that the execute bit is set
            assert_ne!(
                permissions.mode() & 0o111,
                0,
                "Execute permission should be set"
            );
        }
    }

    #[test]
    fn test_print_instances() {
        // Just verify it doesn't panic
        let result = print_instances();
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_gts_schema_for() {
        // Just verify it doesn't panic
        let result = print_gts_schema_for();
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_demo() {
        // Just verify it doesn't panic
        let result = run_demo();
        assert!(result.is_ok());
    }

    #[test]
    fn test_separator_constant() {
        assert_eq!(SEPARATOR.len(), 80);
        assert!(SEPARATOR.chars().all(|c| c == '='));
    }

    #[test]
    fn test_schema_ids_are_valid() {
        use gts::GtsSchema;

        // Verify schema IDs can be retrieved
        let schema1_id = test_structs::BaseEventV1::<()>::SCHEMA_ID;
        let schema2_id = test_structs::AuditPayloadV1::<()>::SCHEMA_ID;
        let schema3_id = test_structs::PlaceOrderDataV1::<()>::SCHEMA_ID;
        let schema4_id = test_structs::PlaceOrderDataPayloadV1::SCHEMA_ID;

        // All should be non-empty
        assert!(!schema1_id.is_empty());
        assert!(!schema2_id.is_empty());
        assert!(!schema3_id.is_empty());
        assert!(!schema4_id.is_empty());

        // All should contain version markers
        assert!(schema1_id.contains("v1~"));
        assert!(schema2_id.contains("v1~"));
        assert!(schema3_id.contains("v1~"));
        assert!(schema4_id.contains("v1~"));
    }

    #[test]
    fn test_schema_serialization() {
        // Test that we can generate schemas
        let schema1 = gts_schema_for!(test_structs::BaseEventV1<()>);
        assert!(schema1.is_object());
        assert!(schema1["$id"].is_string());

        let schema2 = gts_schema_for!(test_structs::BaseEventV1<test_structs::AuditPayloadV1<()>>);
        assert!(schema2.is_object());
        assert!(schema2["$id"].is_string());
    }

    #[test]
    fn test_instance_serialization() {
        let event = create_sample_event().unwrap();

        // Verify it can be serialized to JSON
        let json = serde_json::to_string(&event);
        assert!(json.is_ok());

        let json_value = serde_json::to_value(&event).unwrap();
        assert!(json_value.is_object());
        assert!(json_value["id"].is_string());
        assert!(json_value["payload"].is_object());
    }

    #[test]
    #[ignore = "failing on windows as file names are invalid with certain characters in schema IDs"]
    fn test_schema_file_naming() {
        let temp_dir = TempDir::new().unwrap();
        let schema = serde_json::json!({"type": "object"});

        // Test with various schema IDs
        save_schema(temp_dir.path(), &schema, "gts.vendor:package:type~").unwrap();
        assert!(
            temp_dir
                .path()
                .join("gts.vendor:package:type~.schema.json")
                .exists()
        );

        save_schema(temp_dir.path(), &schema, "simple").unwrap();
        assert!(temp_dir.path().join("simple.schema.json").exists());
    }

    #[test]
    fn test_validate_script_structure() {
        let temp_dir = TempDir::new().unwrap();
        dump_to_directory(temp_dir.path()).unwrap();

        let script_content = fs::read_to_string(temp_dir.path().join("validate.sh")).unwrap();

        // Check for required components
        assert!(script_content.contains("#!/bin/bash"));
        assert!(script_content.contains("SCRIPT_DIR"));
        assert!(script_content.contains("npx ajv-cli validate"));
        assert!(script_content.contains("--spec=draft7"));
        assert!(script_content.contains("-c ajv-formats"));
        assert!(script_content.contains("--strict=false"));

        // Should have -s for main schema
        assert!(script_content.contains("-s \"$SCRIPT_DIR/"));

        // Should have -r for referenced schemas
        assert!(script_content.contains("-r \"$SCRIPT_DIR/"));

        // Should have -d for data file
        assert!(script_content.contains("-d \"$SCRIPT_DIR/"));
    }

    #[test]
    fn test_dump_creates_all_expected_files() {
        let temp_dir = TempDir::new().unwrap();
        dump_to_directory(temp_dir.path()).unwrap();

        let files: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        // Should have at least 5 files: 4 schemas + 1 instance + 1 script
        assert!(
            files.len() >= 6,
            "Expected at least 6 files, got {}",
            files.len()
        );

        // Check that we have a validate.sh
        assert!(files.iter().any(|f| f == "validate.sh"));

        // Check that we have schema files
        let schema_count = files.iter().filter(|f| f.ends_with(".schema.json")).count();
        assert_eq!(schema_count, 4, "Expected 4 schema files");

        // Check that we have an instance file
        let instance_count = files
            .iter()
            .filter(|f| {
                std::path::Path::new(f)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                    && !f.ends_with(".schema.json")
            })
            .count();
        assert_eq!(instance_count, 1, "Expected 1 instance file");
    }
}
