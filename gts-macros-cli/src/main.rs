use gts::gts_schema_for;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

// Include test structs to access their generated constants
mod test_structs {
    use super::{Deserialize, JsonSchema, Serialize};

    pub use gts_macros::struct_to_gts_schema;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        schema_id = "gts.x.core.events.type.v1~",
        description = "Base event type definition",
        properties = "event_type,id,tenant_id,sequence_id,payload"
    )]
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    pub struct BaseEventV1<P> {
        #[serde(rename = "type")]
        pub event_type: String,
        pub id: uuid::Uuid,
        pub tenant_id: uuid::Uuid,
        pub sequence_id: u64,
        pub payload: P,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
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
        schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~x.marketplace.order_purchase.payload.v1~",
        description = "Order placement audit event",
        properties = "order_id"
    )]
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    pub struct PlaceOrderDataPayloadV1 {
        pub order_id: uuid::Uuid,
    }
}

fn main() -> anyhow::Result<()> {
    println!("GTS Macros Demo - Schema Inheritance Chain");
    println!("============================================\n");

    // Print instance examples
    print_instances()?;

    println!("\n{}\n", "=".repeat(80));

    // Print schemas WITH_REFS
    print_schemas_refs()?;

    println!("\n{}\n", "=".repeat(80));

    // Print schemas INLINE (resolved)
    // print_schemas_inline()?;

    println!("\n{}\n", "=".repeat(80));

    // Print gts_schema_for! macro output
    print_gts_schema_for()?;

    Ok(())
}

fn print_instances() -> anyhow::Result<()> {
    println!("INSTANCE EXAMPLES");
    println!("====================\n");

    // Create a complete inheritance chain instance
    let event = test_structs::BaseEventV1 {
        event_type: "order.placed".to_owned(),
        id: uuid::Uuid::new_v4(),
        tenant_id: uuid::Uuid::new_v4(),
        sequence_id: 42,
        payload: test_structs::AuditPayloadV1 {
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)".to_owned(),
            user_id: uuid::Uuid::new_v4(),
            ip_address: "192.168.1.100".to_owned(),
            data: test_structs::PlaceOrderDataV1 {
                order_id: uuid::Uuid::new_v4(),
                product_id: uuid::Uuid::new_v4(),
                last: test_structs::PlaceOrderDataPayloadV1 {
                    order_id: uuid::Uuid::new_v4(),
                },
            },
        },
    };

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

fn print_schemas_refs() -> anyhow::Result<()> {
    println!("REQUIRED OUTPUT (using gts:// external $ref)");
    println!("===============================================\n");

    println!("Each schema generated from struct, with gts:// $id and parent $ref.\n");

    // BaseEventV1 - no parent, generate from struct
    println!("1. BaseEventV1 Schema (no parent - base type):");
    println!("```json");
    let mut base_schema = serde_json::to_value(schema_for!(test_structs::BaseEventV1<()>))?;
    // Add gts:// $id
    base_schema["$id"] = serde_json::json!(format!(
        "gts://{}",
        test_structs::BaseEventV1::<()>::GTS_SCHEMA_ID
    ));
    base_schema["description"] =
        serde_json::json!(test_structs::BaseEventV1::<()>::GTS_SCHEMA_DESCRIPTION);
    // Fix payload type: change from "null" to "object" with additionalProperties: false for generic base types
    if let Some(properties) = base_schema
        .get_mut("properties")
        .and_then(|p| p.as_object_mut())
    {
        if let Some(payload) = properties.get_mut("payload") {
            if payload.get("type").and_then(|t| t.as_str()) == Some("null") {
                *payload = serde_json::json!({
                    "type": "object",
                    "additionalProperties": false
                });
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&base_schema)?);
    println!("```\n");

    // AuditPayloadV1 - generate from struct, add parent $ref
    println!("2. AuditPayloadV1 Schema (references parent via gts://):");
    println!("```json");
    let mut audit_schema = serde_json::to_value(schema_for!(test_structs::AuditPayloadV1<()>))?;
    // Add gts:// $id
    audit_schema["$id"] = serde_json::json!(format!(
        "gts://{}",
        test_structs::AuditPayloadV1::<()>::GTS_SCHEMA_ID
    ));
    audit_schema["description"] =
        serde_json::json!(test_structs::AuditPayloadV1::<()>::GTS_SCHEMA_DESCRIPTION);
    // Add allOf with parent $ref
    let mut own_properties = audit_schema["properties"].take();
    let own_required = audit_schema["required"].take();
    // Fix data type: change from "null" to "object" with additionalProperties: false for generic base types
    if let Some(props) = own_properties.as_object_mut() {
        if let Some(data) = props.get_mut("data") {
            if data.get("type").and_then(|t| t.as_str()) == Some("null") {
                *data = serde_json::json!({
                    "type": "object",
                    "additionalProperties": false
                });
            }
        }
    }
    audit_schema["allOf"] = serde_json::json!([
        { "$ref": format!("gts://{}", test_structs::BaseEventV1::<()>::GTS_SCHEMA_ID) },
        { "type": "object", "properties": { "payload": { "type": "object", "properties": own_properties, "required": own_required } } }
    ]);
    if let Some(obj) = audit_schema.as_object_mut() {
        obj.remove("properties");
        obj.remove("required");
    }
    println!("{}", serde_json::to_string_pretty(&audit_schema)?);
    println!("```\n");

    // PlaceOrderDataV1 - generate from struct, add parent $ref
    println!("3. PlaceOrderDataV1 Schema (references parent via gts://):");
    println!("```json");
    let mut place_order_schema =
        serde_json::to_value(schema_for!(test_structs::PlaceOrderDataV1<()>))?;
    // Add gts:// $id
    place_order_schema["$id"] = serde_json::json!(format!(
        "gts://{}",
        test_structs::PlaceOrderDataV1::<()>::GTS_SCHEMA_ID
    ));
    place_order_schema["description"] =
        serde_json::json!(test_structs::PlaceOrderDataV1::<()>::GTS_SCHEMA_DESCRIPTION);
    // Add allOf with parent $ref
    let own_properties = place_order_schema["properties"].take();
    let own_required = place_order_schema["required"].take();
    place_order_schema["allOf"] = serde_json::json!([
        { "$ref": format!("gts://{}", test_structs::AuditPayloadV1::<()>::GTS_SCHEMA_ID) },
        { "type": "object", "properties": { "payload": { "type": "object", "properties": { "data": { "type": "object", "properties": own_properties, "required": own_required } } } } }
    ]);
    if let Some(obj) = place_order_schema.as_object_mut() {
        obj.remove("properties");
        obj.remove("required");
    }
    println!("{}", serde_json::to_string_pretty(&place_order_schema)?);
    println!("```\n");

    Ok(())
}

#[allow(dead_code)]
fn print_schemas_inline() -> anyhow::Result<()> {
    println!("INLINE VERSION (using schemars with internal $ref)");
    println!("===================================================\n");

    println!("Fully resolved nested schema with definitions section.\n");

    // Generate schema for each type using schemars
    println!("1. BaseEventV1 (standalone):");
    println!("```json");
    let base_schema = schema_for!(test_structs::BaseEventV1<()>);
    println!("{}", serde_json::to_string_pretty(&base_schema)?);
    println!("```\n");

    println!("2. AuditPayloadV1 (standalone):");
    println!("```json");
    let audit_schema = schema_for!(test_structs::AuditPayloadV1<()>);
    println!("{}", serde_json::to_string_pretty(&audit_schema)?);
    println!("```\n");

    println!("3. PlaceOrderDataV1 (standalone):");
    println!("```json");
    let place_order_schema = schema_for!(test_structs::PlaceOrderDataV1<()>);
    println!("{}", serde_json::to_string_pretty(&place_order_schema)?);
    println!("```\n");

    println!("4. Full Composed Type (BaseEventV1<AuditPayloadV1<PlaceOrderDataV1<()>>>):");
    println!("```json");
    let composed_schema = schema_for!(
        test_structs::BaseEventV1<test_structs::AuditPayloadV1<test_structs::PlaceOrderDataV1<()>>>
    );
    println!("{}", serde_json::to_string_pretty(&composed_schema)?);
    println!("```\n");

    Ok(())
}

fn print_gts_schema_for() -> anyhow::Result<()> {
    println!("GTS_SCHEMA_FOR! MACRO (allOf with $ref to base)");
    println!("=================================================\n");

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
