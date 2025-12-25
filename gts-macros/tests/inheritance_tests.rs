#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::nonminimal_bool,
    clippy::uninlined_format_args,
    clippy::bool_assert_comparison
)]

use gts::{GtsSchema, GtsStore};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/* ============================================================
Chained inheritance
============================================================ */

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
    pub event_type: String,
    pub id: Uuid,
    pub tenant_id: Uuid,
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
    pub user_id: Uuid,
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
pub struct PlaceOrderDataV1 {
    pub order_id: Uuid,
    pub product_id: Uuid,
}

/* ============================================================
Chained inheritance w/o new attributes
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition",
    properties = "name,description"
)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TopicV1<P> {
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = TopicV1,
    schema_id = "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~",
    description = "Order topic configuration",
    properties = ""
)]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
// there are no new fields in OrderTopicConfigV1, we only need it to get dedicated GTS id
pub struct OrderTopicConfigV1;

/* ============================================================
The macro automatically generates:
- GTS_SCHEMA_JSON constants with proper allOf inheritance
- GTS_SCHEMA_ID constants
- make_gts_instance_id() methods

No more manual schema implementation needed!
============================================================ */

/* ============================================================
Demo
============================================================ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_serialization() {
        let event = BaseEventV1 {
            event_type: PlaceOrderDataV1::GTS_SCHEMA_ID.into(),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 42,
            payload: AuditPayloadV1 {
                user_agent: "Mozilla/5.0".into(),
                user_id: Uuid::new_v4(),
                ip_address: "127.0.0.1".into(),
                data: PlaceOrderDataV1 {
                    order_id: Uuid::new_v4(),
                    product_id: Uuid::new_v4(),
                },
            },
        };

        let json = serde_json::to_string_pretty(&event).unwrap();
        println!("\nRUNTIME JSON:\n{}", json);

        assert!(json.contains("type"));
        assert!(json.contains("payload"));
        assert!(json.contains("user_agent"));
        assert!(json.contains("data"));
    }

    #[test]
    fn test_schema_inheritance() {
        // Only base type can access schema methods directly
        // Multi-segment schemas are blocked from direct access
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();

        println!("\n=== BASE EVENT SCHEMA ===");
        println!("{}", serde_json::to_string_pretty(&base_schema).unwrap());

        // Verify schema IDs are still accessible
        assert!(BaseEventV1::<()>::GTS_SCHEMA_ID == "gts.x.core.events.type.v1~");
        assert!(
            AuditPayloadV1::<()>::GTS_SCHEMA_ID
                == "gts.x.core.events.type.v1~x.core.audit.event.v1~"
        );
        assert!(PlaceOrderDataV1::GTS_SCHEMA_ID == "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~");

        // BaseEventV1 should have direct properties, no allOf
        assert!(
            !base_schema.get("allOf").is_some(),
            "BaseEventV1 should not have allOf"
        );
        assert!(
            base_schema.get("properties").is_some(),
            "BaseEventV1 should have direct properties"
        );

        // Multi-segment schemas (AuditPayloadV1, PlaceOrderDataV1) are blocked from direct method access
        // They must be loaded from schema files via GtsStore
    }

    #[test]
    fn test_schema_inline_vs_refs_structure() {
        // Only base type can access schema methods directly
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();

        // Base schema should have direct properties, no allOf
        assert!(
            !base_schema.get("allOf").is_some(),
            "BaseEventV1 should not have allOf"
        );
        assert!(
            base_schema.get("properties").is_some(),
            "BaseEventV1 should have direct properties"
        );

        // Test INLINE resolves $refs using store (only for base type)
        let mut store = GtsStore::new(None);
        store
            .register_schema(BaseEventV1::<()>::GTS_SCHEMA_ID, &base_schema)
            .unwrap();

        let base_inline = store.resolve_schema_refs(&BaseEventV1::<()>::gts_schema_with_refs());

        // Base has no $refs to resolve, so inline should be same as with_refs
        assert!(
            base_inline.get("properties").is_some(),
            "INLINE should have direct properties"
        );
        let inline_props = base_inline.get("properties").unwrap().as_object().unwrap();
        assert!(
            inline_props.contains_key("type"),
            "Should contain type (schemars uses serde rename)"
        );
        assert!(inline_props.contains_key("id"), "Should contain id");
        assert!(
            inline_props.contains_key("tenant_id"),
            "Should contain tenant_id"
        );
        assert!(
            inline_props.contains_key("sequence_id"),
            "Should contain sequence_id"
        );
        assert!(
            inline_props.contains_key("payload"),
            "Should contain payload"
        );
    }

    #[test]
    fn test_schema_matches_object_structure() {
        // Create an instance to test against schema
        let event = BaseEventV1 {
            event_type: "test.event".to_string(),
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            sequence_id: 42,
            payload: AuditPayloadV1 {
                user_agent: "test-agent".to_string(),
                user_id: uuid::Uuid::new_v4(),
                ip_address: "127.0.0.1".to_string(),
                data: PlaceOrderDataV1 {
                    order_id: uuid::Uuid::new_v4(),
                    product_id: uuid::Uuid::new_v4(),
                },
            },
        };

        let json = serde_json::to_value(&event).unwrap();

        // Verify base schema contains expected properties
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();
        let schema_props = base_schema.get("properties").unwrap().as_object().unwrap();

        // All base object properties should exist in schema
        for (key, _) in json.as_object().unwrap() {
            if key == "payload" {
                // payload is generic, handled separately
                continue;
            }
            // schemars uses serde rename, so "event_type" becomes "type" in schema
            assert!(
                schema_props.contains_key(key),
                "Schema should contain property: {}",
                key
            );
        }
    }

    #[test]
    fn test_nesting_issues_current_behavior() {
        // This test demonstrates the FIXED behavior where nesting is now respected

        // Parse the BaseEventV1 schema (single-segment, can use WITH_REFS)
        let base_schema: serde_json::Value =
            serde_json::from_str(&BaseEventV1::<()>::gts_json_schema_with_refs()).unwrap();

        // The payload field should be a nested object, and now it's correctly treated as "object"
        let base_props = base_schema.get("properties").unwrap().as_object().unwrap();
        let payload_prop = base_props.get("payload").unwrap();

        // FIXED BEHAVIOR: payload is correctly treated as object
        assert_eq!(
            payload_prop["type"], "object",
            "Payload is now correctly treated as object"
        );

        // Multi-segment schemas are blocked from direct method access
        // They must be loaded from schema files via GtsStore
    }

    #[test]
    fn test_expected_nesting_behavior() {
        // This test shows what the CORRECT behavior should be

        // Create an actual instance to see the real structure
        let event = BaseEventV1 {
            event_type: "test.event".to_string(),
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            sequence_id: 42,
            payload: AuditPayloadV1 {
                user_agent: "test-agent".to_string(),
                user_id: uuid::Uuid::new_v4(),
                ip_address: "127.0.0.1".to_string(),
                data: PlaceOrderDataV1 {
                    order_id: uuid::Uuid::new_v4(),
                    product_id: uuid::Uuid::new_v4(),
                },
            },
        };

        let json = serde_json::to_value(&event).unwrap();

        // The actual JSON has nested objects:
        // - payload is an object with user_agent, user_id, ip_address, data
        // - data is an object with order_id, product_id
        let payload = json.get("payload").unwrap();
        assert_eq!(
            payload.get("user_agent").unwrap().as_str().unwrap(),
            "test-agent"
        );
        assert_eq!(payload.get("user_id").unwrap().is_string(), true);
        assert_eq!(
            payload.get("ip_address").unwrap().as_str().unwrap(),
            "127.0.0.1"
        );

        let data = payload.get("data").unwrap();
        assert_eq!(data.get("order_id").unwrap().is_string(), true);
        assert_eq!(data.get("product_id").unwrap().is_string(), true);

        // But the current schema doesn't reflect this nested structure!
        // The schema should have:
        // - payload: { type: "object", properties: { user_agent: {...}, user_id: {...}, ip_address: {...}, data: {...} } }
        // - data: { type: "object", properties: { order_id: {...}, product_id: {...} } }
    }

    // =============================================================================
    // Tests for explicit 'base' attribute
    // =============================================================================

    #[test]
    fn test_base_schema_id_constants() {
        // Base type should have BASE_SCHEMA_ID = None
        assert_eq!(BaseEventV1::<()>::BASE_SCHEMA_ID, None);
        assert_eq!(TopicV1::<()>::BASE_SCHEMA_ID, None);

        // Child types should have BASE_SCHEMA_ID = Some(parent's schema ID)
        assert_eq!(
            AuditPayloadV1::<()>::BASE_SCHEMA_ID,
            Some("gts.x.core.events.type.v1~")
        );
        assert_eq!(
            PlaceOrderDataV1::BASE_SCHEMA_ID,
            Some("gts.x.core.events.type.v1~x.core.audit.event.v1~")
        );
        assert_eq!(
            OrderTopicConfigV1::BASE_SCHEMA_ID,
            Some("gts.x.core.events.topic.v1~")
        );
    }

    #[test]
    fn test_explicit_base_attribute_schema_generation() {
        // TopicV1 is marked with base = true
        let topic_schema = TopicV1::<()>::gts_schema_with_refs();

        // Base schema should have direct properties, no allOf
        assert!(
            !topic_schema.get("allOf").is_some(),
            "TopicV1 (base = true) should not have allOf"
        );
        assert!(
            topic_schema.get("properties").is_some(),
            "TopicV1 should have direct properties"
        );

        // Verify $id
        assert_eq!(topic_schema["$id"], "gts://gts.x.core.events.topic.v1~");
    }

    #[test]
    fn test_explicit_base_parent_relationship() {
        // OrderTopicConfigV1 is marked with base = TopicV1
        // The compile-time assertion already verified that TopicV1::GTS_SCHEMA_ID
        // matches the parent segment in OrderTopicConfigV1's schema_id

        // Verify the schema IDs are correctly related
        assert_eq!(TopicV1::<()>::GTS_SCHEMA_ID, "gts.x.core.events.topic.v1~");
        assert_eq!(
            OrderTopicConfigV1::GTS_SCHEMA_ID,
            "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~"
        );

        // The parent segment should match
        assert_eq!(
            OrderTopicConfigV1::BASE_SCHEMA_ID,
            Some(TopicV1::<()>::GTS_SCHEMA_ID)
        );
    }

    // =============================================================================
    // Tests for unit structs (empty nested types)
    // =============================================================================

    #[test]
    fn test_unit_struct_schema_generation() {
        // OrderTopicConfigV1 is a unit struct with no fields
        // It should still generate a valid schema with allOf inheritance
        use gts::GtsSchema;

        let schema = OrderTopicConfigV1::gts_schema_with_refs();

        // Should have $id
        assert_eq!(
            schema["$id"],
            "gts://gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~"
        );

        // Should have allOf with parent reference (since it's a child type)
        let all_of = schema
            .get("allOf")
            .expect("OrderTopicConfigV1 should have allOf");
        assert!(all_of.is_array(), "allOf should be an array");

        // First element should be $ref to parent
        let first = &all_of[0];
        assert_eq!(first["$ref"], "gts://gts.x.core.events.topic.v1~");
    }

    #[test]
    fn test_unit_struct_instantiation() {
        // Unit struct should be usable as a type parameter for parent
        let topic = TopicV1 {
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1 {},
        };

        // Serialize should work
        let json = serde_json::to_value(&topic).unwrap();
        assert_eq!(json["name"], "orders");
        assert_eq!(json["description"], "Order events");
        // config field will serialize to null for unit struct with default serde
    }

    #[test]
    fn test_unit_struct_gts_schema_trait() {
        use gts::GtsSchema;

        // Unit struct should implement GtsSchema
        assert_eq!(
            OrderTopicConfigV1::SCHEMA_ID,
            "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~"
        );
        assert_eq!(OrderTopicConfigV1::GENERIC_FIELD, None);

        // innermost_schema_id for a non-generic type returns itself
        assert_eq!(
            OrderTopicConfigV1::innermost_schema_id(),
            "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~"
        );
    }
}
