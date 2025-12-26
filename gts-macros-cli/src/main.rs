use gts::gts::GtsSchemaId;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use gts::gts_schema_for;
use serde::{Deserialize, Serialize};

use crate::test_structs::PlaceOrderDataPayloadV1;

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
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
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
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
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
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
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
        event_type: PlaceOrderDataPayloadV1::gts_schema_id().clone(),
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
