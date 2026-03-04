#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::nonminimal_bool,
    clippy::uninlined_format_args,
    clippy::bool_assert_comparison
)]

use gts::gts::GtsSchemaId;
use gts::{GtsInstanceId, GtsSchema, GtsStore};
use gts_macros::struct_to_gts_schema;
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
#[derive(Debug)]
pub struct BaseEventV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
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
#[derive(Debug)]
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
#[derive(Debug)]
pub struct PlaceOrderDataV1 {
    pub order_id: Uuid,
    pub product_id: Uuid,
}

/* ============================================================
2-level inheritance (`BaseEventV1` -> `SimplePayloadV1``)
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.core.events.type.v1~x.core.simple.event.v1~",
    description = "Simple event payload with just a message",
    properties = "message,severity"
)]
#[derive(Debug)]
pub struct SimplePayloadV1 {
    pub message: String,
    pub severity: u8,
}

/* ============================================================
Base struct ID field validation tests
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition with id field",
    properties = "id,name,description"
)]
#[derive(Debug)]
pub struct TopicV1WithIdV1<P> {
    pub id: GtsInstanceId,
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition with gts_id field",
    properties = "gts_id,name,description"
)]
#[derive(Debug)]
pub struct TopicV1WithGtsIdV1<P> {
    pub gts_id: GtsInstanceId,
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition with gtsId field",
    properties = "gts_id,name,description"
)]
#[derive(Debug)]
pub struct TopicV1WithGtsIdCamelV1<P> {
    pub gts_id: GtsInstanceId,
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition with gts_type field",
    properties = "gts_type,name,description"
)]
#[derive(Debug)]
pub struct TopicV1WithGtsTypeV1<P> {
    pub gts_type: GtsSchemaId,
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition with gtsType field",
    properties = "gts_type,name,description"
)]
#[derive(Debug)]
pub struct TopicV1WithGtsTypeCamelV1<P> {
    pub gts_type: GtsSchemaId,
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

/* ============================================================
Chained inheritance w/o new attributes
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition",
    properties = "id,name,description"
)]
#[derive(Debug)]
pub struct TopicV1<P> {
    pub id: GtsInstanceId,
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
#[derive(Debug)]
// there are no new fields in OrderTopicConfigV1, we only need it to get dedicated GTS id
pub struct OrderTopicConfigV1;

/* ============================================================
Test serde rename on generic field - the serialized name should be used
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.container.v1~",
    description = "Container with renamed generic field",
    properties = "id,name,rust_field_name"
)]
#[derive(Debug)]
pub struct ContainerV1<T> {
    pub id: GtsInstanceId,
    pub name: String,
    #[serde(rename = "inner_data")]
    pub rust_field_name: T,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = ContainerV1,
    schema_id = "gts.x.core.events.container.v1~x.app.entities.content.v1~",
    description = "Content extending container",
    properties = "content_value"
)]
#[derive(Debug)]
pub struct ContentV1 {
    pub content_value: String,
}

/* ============================================================
The macro automatically generates:
- GTS_SCHEMA_JSON constants with proper allOf inheritance
- GTS_SCHEMA_ID constants
- gts_make_instance_id() methods

No more manual schema implementation needed!
============================================================ */

/* ============================================================
Issue #72: Base struct without type/id field
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.errors.quota_failure.v1~",
    description = "Quota failure with one or more violations",
    properties = "violations"
)]
#[derive(Debug, Clone)]
pub struct QuotaFailureV1 {
    pub violations: Vec<String>,
}

/* ============================================================
Issue #72: Base struct without gts_type field
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.errors.rate_limit.v1~",
    description = "Rate limit error without gts_type field",
    properties = "retry_after"
)]
#[derive(Debug, Clone)]
pub struct RateLimitErrorV1 {
    pub retry_after: u64,
}

/* ============================================================
Demo
============================================================ */

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to register 3-level event schemas (`BaseEventV1` -> `AuditPayloadV1` -> `PlaceOrderDataV1`)
    fn register_three_level_event_schemas(ops: &mut gts::GtsOps) {
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();
        let base_result = ops.add_schema(
            BaseEventV1::<()>::gts_schema_id().clone().into_string(),
            &base_schema,
        );
        assert!(
            base_result.ok,
            "BaseEventV1 schema registration failed: {}",
            base_result.error
        );

        let audit_schema = AuditPayloadV1::<()>::gts_schema_with_refs();
        let audit_result = ops.add_schema(
            AuditPayloadV1::<()>::gts_schema_id().clone().into_string(),
            &audit_schema,
        );
        assert!(
            audit_result.ok,
            "AuditPayloadV1 schema registration failed: {}",
            audit_result.error
        );

        let order_schema = PlaceOrderDataV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            PlaceOrderDataV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "PlaceOrderDataV1 schema registration failed: {}",
            order_result.error
        );
    }

    /// Helper to register 2-level event schemas (`BaseEventV1` -> `SimplePayloadV1`)
    fn register_two_level_event_schemas(ops: &mut gts::GtsOps) {
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();
        let base_result = ops.add_schema(
            BaseEventV1::<()>::gts_schema_id().clone().into_string(),
            &base_schema,
        );
        assert!(
            base_result.ok,
            "BaseEventV1 schema registration failed: {}",
            base_result.error
        );

        let simple_schema = SimplePayloadV1::gts_schema_with_refs();
        let simple_result = ops.add_schema(
            SimplePayloadV1::gts_schema_id().clone().into_string(),
            &simple_schema,
        );
        assert!(
            simple_result.ok,
            "SimplePayloadV1 schema registration failed: {}",
            simple_result.error
        );
    }

    #[test]
    fn test_runtime_serialization() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_three_level_event_schemas(&mut ops);

        let event = BaseEventV1 {
            event_type: PlaceOrderDataV1::gts_schema_id().clone(),
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

        // Validate instance JSON structure matches schema expectations
        let event_json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            event_json["type"],
            "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
        );
        assert!(event_json["payload"]["user_agent"].is_string());
        assert!(event_json["payload"]["data"]["order_id"].is_string());
    }

    #[test]
    fn test_schema_inheritance() {
        // Only base type can access schema methods directly
        // Multi-segment schemas are blocked from direct access
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();

        println!("\n=== BASE EVENT SCHEMA ===");
        println!("{}", serde_json::to_string_pretty(&base_schema).unwrap());

        // Verify schema IDs are still accessible
        assert!(
            BaseEventV1::<()>::gts_schema_id().clone().into_string()
                == "gts.x.core.events.type.v1~"
        );
        let _audit_payload_id = AuditPayloadV1::<()>::gts_schema_id().clone().into_string();
        assert!(
            PlaceOrderDataV1::gts_schema_id().clone().into_string()
                == "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
        );

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
            .register_schema(BaseEventV1::<()>::gts_schema_id().as_ref(), &base_schema)
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
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_three_level_event_schemas(&mut ops);

        // Create an instance to test against schema
        let event = BaseEventV1 {
            event_type: PlaceOrderDataV1::gts_schema_id().clone(),
            id: Uuid::new_v4(),
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

        // Validate instance field paths match schema structure
        assert_eq!(
            json["type"],
            "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
        );
        assert!(json["payload"]["user_agent"].is_string());
        assert!(json["payload"]["data"]["order_id"].is_string());
    }

    #[test]
    fn test_nesting_issues_current_behavior() {
        // This test demonstrates the FIXED behavior where nesting is now respected

        // Parse the BaseEventV1 schema (single-segment, can use WITH_REFS)
        let base_schema: serde_json::Value =
            serde_json::from_str(&BaseEventV1::<()>::gts_schema_with_refs_as_string()).unwrap();

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
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_three_level_event_schemas(&mut ops);

        // This test shows what the CORRECT behavior should be
        // Create an actual instance to see the real structure
        let event = BaseEventV1 {
            event_type: PlaceOrderDataV1::gts_schema_id().clone(),
            id: Uuid::new_v4(),
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

        // Validate type field matches PlaceOrderDataV1 schema ID
        assert_eq!(
            json["type"],
            "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
        );

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

        // Verify field nesting is correct - fields should NOT be at wrong levels
        assert!(
            json.get("user_agent").is_none(),
            "user_agent should be in payload"
        );
        assert!(
            json.get("order_id").is_none(),
            "order_id should be in payload.data"
        );
        assert!(
            payload.get("order_id").is_none(),
            "order_id should be in data, not payload"
        );
    }

    // =============================================================================
    // Tests for explicit 'base' attribute
    // =============================================================================

    #[test]
    fn test_base_schema_id_methods() {
        // Base type should have gts_base_schema_id() = None
        assert!(BaseEventV1::<()>::gts_base_schema_id().is_none());
        assert!(TopicV1::<()>::gts_base_schema_id().is_none());

        // Child types should have gts_base_schema_id() = Some(parent's schema ID)
        assert_eq!(
            AuditPayloadV1::<()>::gts_base_schema_id().map(AsRef::as_ref),
            Some("gts.x.core.events.type.v1~")
        );
        assert_eq!(
            PlaceOrderDataV1::gts_base_schema_id().map(AsRef::as_ref),
            Some("gts.x.core.events.type.v1~x.core.audit.event.v1~")
        );
        assert_eq!(
            OrderTopicConfigV1::gts_base_schema_id().map(AsRef::as_ref),
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
        assert_eq!(
            TopicV1::<()>::gts_schema_id().clone().into_string(),
            "gts.x.core.events.topic.v1~"
        );
        let order_topic_id = OrderTopicConfigV1::gts_schema_id().clone().into_string();
        assert_eq!(
            order_topic_id,
            "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~"
        );

        // Test that the GTS schema ID is a valid GTS schema ID
        assert!(order_topic_id.ends_with('~'), "Schema ID should end with ~");

        // Test that the GTS schema ID can be used to create a GtsSchemaId
        let schema_id_type = gts::gts::GtsSchemaId::new(&order_topic_id);
        assert_eq!(schema_id_type.into_string(), order_topic_id);
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
    fn test_unit_struct_instantiation_and_serialization() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_topic_schemas(&mut ops);

        // Unit struct should be usable as a type parameter for parent
        let topic = TopicV1::<OrderTopicConfigV1> {
            id: OrderTopicConfigV1::gts_make_instance_id("test.test._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };

        // Serialize to JSON string should work
        let json_str = serde_json::to_string(&topic).unwrap();
        assert!(
            json_str.contains("orders"),
            "JSON should contain topic name"
        );

        // Serialize to Value should work
        let mut json = serde_json::to_value(&topic).unwrap();
        assert_eq!(json["name"], "orders");
        assert_eq!(json["description"], "Order events");
        // Unit struct serializes to empty object {} with custom serialization
        assert_eq!(json["config"], serde_json::json!({}));

        // Serialize pretty should work
        let json_pretty = serde_json::to_string_pretty(&topic).unwrap();
        assert!(
            json_pretty.contains("orders"),
            "Pretty JSON should contain topic name"
        );

        // Validate instance against schema
        fix_null_config(&mut json);
        let add_result = ops.add_entity(&json, true);
        assert!(
            add_result.ok,
            "TopicV1<OrderTopicConfigV1> instance should validate: {}",
            add_result.error
        );
        let validate_result = ops.validate_instance(&topic.id);
        assert!(
            validate_result.ok,
            "TopicV1<OrderTopicConfigV1> validation failed: {}",
            validate_result.error
        );
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

    #[test]
    fn test_empty_struct_serialization_with_nested_empty() {
        // Test serialization and deserialization of struct_to_gts_schema macro-generated structs
        // with empty/nested struct definitions like OrderTopicConfigV1

        // Create a TopicV1 with OrderTopicConfigV1 (empty struct) as config
        let topic = TopicV1::<OrderTopicConfigV1> {
            id: OrderTopicConfigV1::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };

        // Test serialization to JSON
        let serialized = serde_json::to_string(&topic).expect("Serialization should succeed");
        assert!(!serialized.is_empty());
        assert!(serialized.contains("orders"));

        // Print current serialization output for debugging
        println!("Current serialization: {}", serialized);

        // Test deserialization back from JSON
        let deserialized: TopicV1<OrderTopicConfigV1> =
            serde_json::from_str(&serialized).expect("Deserialization should succeed");
        assert_eq!(topic.name, deserialized.name);
        assert_eq!(topic.description, deserialized.description);
        // Both config fields should be OrderTopicConfigV1 (unit struct)
        // Since it's a unit struct, we can compare them directly
        let _ = topic.config;
        let _ = deserialized.config;

        // Test with pretty serialization
        let serialized_pretty =
            serde_json::to_string_pretty(&topic).expect("Pretty serialization should succeed");
        println!("Current pretty serialization: {}", serialized_pretty);
        let deserialized_pretty: TopicV1<OrderTopicConfigV1> =
            serde_json::from_str(&serialized_pretty)
                .expect("Pretty deserialization should succeed");
        assert_eq!(topic.name, deserialized_pretty.name);
    }

    #[test]
    fn test_empty_struct_gts_instance_id_serialization() {
        // Test GTS instance ID serialization/deserialization for macro-generated structs with empty nested types

        // Create instance ID using the macro-generated method
        let instance_id = TopicV1::<OrderTopicConfigV1>::gts_make_instance_id("test-topic");

        // Serialize the instance ID
        let serialized = serde_json::to_string(&instance_id).expect("Serialization should succeed");
        assert!(!serialized.is_empty());

        // Deserialize back from JSON
        let deserialized: gts::GtsInstanceId =
            serde_json::from_str(&serialized).expect("Deserialization should succeed");
        assert_eq!(instance_id, deserialized);

        // Verify the instance ID contains the expected GTS ID chain
        let id_str = instance_id.as_ref();
        assert!(id_str.contains("gts.x.core.events.topic.v1~"));
        assert!(id_str.ends_with("test-topic"));
        // Note: gts_make_instance_id uses the schema ID of the type it's called on (TopicV1),
        // not the generic parameter (OrderTopicConfigV1)
    }

    #[test]
    fn test_empty_struct_in_memory_storage_matching() {
        // Test serialization and matching to appropriate GTS type using GtsOps
        // for macro-generated structs with empty nested types

        let mut ops = gts::GtsOps::new(None, None, 0);

        let base_schema = TopicV1::<()>::gts_schema_with_refs();
        let base_result = ops.add_schema(
            TopicV1::<()>::gts_schema_id().clone().into_string(),
            &base_schema,
        );
        assert!(
            base_result.ok,
            "Base schema registration should succeed: {}",
            base_result.error
        );

        // Register the OrderTopicConfigV1 schema (empty struct) using GtsOps
        let empty_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let empty_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &empty_schema,
        );
        assert!(
            empty_result.ok,
            "Empty schema registration should succeed: {}",
            empty_result.error
        );

        // Create a proper nested TopicV1<OrderTopicConfigV1> instance
        let topic_instance = TopicV1::<OrderTopicConfigV1> {
            id: OrderTopicConfigV1::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };

        // Register the instance in the store
        let mut instance_json =
            serde_json::to_value(&topic_instance).expect("Should convert to JSON");

        // Ensure the config field is an empty object, not null
        if let Some(config_obj) = instance_json.get_mut("config")
            && *config_obj == serde_json::Value::Null
        {
            *config_obj = serde_json::json!({});
        }

        let add_result = ops.add_entity(&instance_json, true);
        assert!(
            add_result.ok,
            "Instance registration should succeed: {}",
            add_result.error
        );

        // Test query functionality using GtsOps
        let query_result = ops.query(
            "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~*",
            10,
        );

        // The query pattern matches 3-segment IDs, so it should only find the instance, not the 2-segment schema
        assert_eq!(
            query_result.count, 1,
            "Query should find one result (the instance)"
        );
        assert_eq!(
            query_result.results.len(),
            1,
            "Query should return one result"
        );

        // Verify the queried result matches our original instance
        let queried_instance = &query_result.results[0];
        assert_eq!(
            *queried_instance, instance_json,
            "Queried result should match original instance"
        );
    }

    #[test]
    fn test_empty_struct_schema_resolution_and_matching() {
        // Test schema resolution and type matching for empty struct schemas using GtsOps
        let mut ops = gts::GtsOps::new(None, None, 0);

        // Register schemas and verify they can be retrieved
        let (retrieved_base_schema, retrieved_empty_schema) =
            register_and_retrieve_schemas(&mut ops);

        // Create and validate instances
        test_topic_instances(&mut ops);

        // Verify schema structure
        verify_schema_structure(&retrieved_base_schema, &retrieved_empty_schema);
    }

    fn register_and_retrieve_schemas(
        ops: &mut gts::GtsOps,
    ) -> (serde_json::Value, serde_json::Value) {
        // Register schemas for the inheritance chain using GtsOps
        let base_schema = TopicV1::<()>::gts_schema_with_refs();
        let empty_schema = OrderTopicConfigV1::gts_schema_with_refs();

        let base_result = ops.add_schema(
            TopicV1::<()>::gts_schema_id().clone().into_string(),
            &base_schema,
        );
        assert!(
            base_result.ok,
            "Base schema registration should succeed: {}",
            base_result.error
        );

        let empty_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &empty_schema,
        );
        assert!(
            empty_result.ok,
            "Empty schema registration should succeed: {}",
            empty_result.error
        );

        // Test schema retrieval and content verification using GtsOps
        let retrieved_base_entity = ops.get_entity(TopicV1::<()>::gts_schema_id().as_ref());
        assert!(retrieved_base_entity.ok, "Base schema should be found");
        let retrieved_empty_entity = ops.get_entity(OrderTopicConfigV1::gts_schema_id().as_ref());
        assert!(retrieved_empty_entity.ok, "Empty schema should be found");

        // Extract content from the entities
        let retrieved_base_schema = retrieved_base_entity
            .content
            .expect("Base schema should have content");
        let retrieved_empty_schema = retrieved_empty_entity
            .content
            .expect("Empty schema should have content");

        // Verify base schema structure
        assert!(
            retrieved_base_schema.is_object(),
            "Base schema should be a valid JSON object"
        );
        assert!(
            retrieved_empty_schema.is_object(),
            "Empty schema should be a valid JSON object"
        );

        (retrieved_base_schema, retrieved_empty_schema)
    }

    fn test_topic_instances(ops: &mut gts::GtsOps) {
        // Create and validate TopicV1 instance
        let topic_instance = TopicV1::<()> {
            id: TopicV1::<()>::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order lifecycle events".to_string()),
            config: (),
        };

        // Create and validate TopicV1<OrderTopicConfigV1> instance
        let nested_topic_instance = TopicV1::<OrderTopicConfigV1> {
            id: TopicV1::<OrderTopicConfigV1>::gts_make_instance_id("vendor.app.nested.topic.v1"),
            name: "nested-orders".to_string(),
            description: Some("Nested order events".to_string()),
            config: OrderTopicConfigV1 {},
        };

        // Serialize both instances
        let mut topic_json =
            serde_json::to_value(&topic_instance).expect("TopicV1 should serialize");
        let nested_topic_json = serde_json::to_value(&nested_topic_instance)
            .expect("TopicV1<OrderTopicConfigV1> should serialize");

        // Fix the config field for TopicV1<()> - convert null to {} to match schema
        if let Some(config_obj) = topic_json.get_mut("config")
            && *config_obj == serde_json::Value::Null
        {
            *config_obj = serde_json::json!({});
        }

        // Add instances to the store
        let topic_add_result = ops.add_entity(&topic_json, true);
        assert!(
            topic_add_result.ok,
            "TopicV1 instance should be added to store: {}",
            topic_add_result.error
        );

        let nested_topic_add_result = ops.add_entity(&nested_topic_json, true);
        assert!(
            nested_topic_add_result.ok,
            "TopicV1<OrderTopicConfigV1> instance should be added to store: {}",
            nested_topic_add_result.error
        );

        // Validate TopicV1 instance against its schema
        let topic_validation = ops.validate_instance(&topic_instance.id);
        assert!(
            topic_validation.ok,
            "TopicV1 instance should validate: {}",
            topic_validation.error
        );

        // Validate TopicV1<OrderTopicConfigV1> instance against its schema
        let nested_topic_validation = ops.validate_instance(&nested_topic_instance.id);
        assert!(
            nested_topic_validation.ok,
            "TopicV1<OrderTopicConfigV1> instance should validate: {}",
            nested_topic_validation.error
        );

        // Verify the instance IDs are correctly generated
        assert_eq!(
            topic_instance.id,
            "gts.x.core.events.topic.v1~vendor.app._.topic.v1"
        );
        assert_eq!(
            nested_topic_instance.id,
            "gts.x.core.events.topic.v1~vendor.app.nested.topic.v1"
        );

        // Verify the JSON structure contains expected fields
        let topic_obj = topic_json
            .as_object()
            .expect("TopicV1 JSON should be object");
        let nested_topic_obj = nested_topic_json
            .as_object()
            .expect("TopicV1<OrderTopicConfigV1> JSON should be object");

        assert!(topic_obj.contains_key("id"), "TopicV1 should have id field");
        assert!(
            topic_obj.contains_key("name"),
            "TopicV1 should have name field"
        );
        assert!(
            topic_obj.contains_key("description"),
            "TopicV1 should have description field"
        );

        assert!(
            nested_topic_obj.contains_key("id"),
            "TopicV1<OrderTopicConfigV1> should have id field"
        );
        assert!(
            nested_topic_obj.contains_key("name"),
            "TopicV1<OrderTopicConfigV1> should have name field"
        );
    }

    fn verify_schema_structure(
        _retrieved_base_schema: &serde_json::Value,
        retrieved_empty_schema: &serde_json::Value,
    ) {
        assert!(
            retrieved_empty_schema.is_object(),
            "Empty schema should be a valid JSON object"
        );
        assert_eq!(
            retrieved_empty_schema.get("$id").unwrap().as_str().unwrap(),
            "gts://gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~"
        );
        assert!(
            retrieved_empty_schema.get("allOf").is_some(),
            "Empty schema should have allOf inheritance"
        );

        // Verify allOf contains the expected reference
        let all_of = retrieved_empty_schema
            .get("allOf")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(!all_of.is_empty(), "allOf should not be empty");

        // Check that the allOf references the base schema
        let first_ref = &all_of[0];
        assert_eq!(
            first_ref.get("$ref").unwrap().as_str().unwrap(),
            "gts://gts.x.core.events.topic.v1~"
        );
    }

    #[test]
    fn test_base_struct_with_id_field_compiles() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        let schema = TopicV1WithIdV1::<()>::gts_schema_with_refs();
        let result = ops.add_schema(
            TopicV1WithIdV1::<()>::gts_schema_id().clone().into_string(),
            &schema,
        );
        assert!(
            result.ok,
            "TopicV1WithIdV1 schema registration failed: {}",
            result.error
        );
        let order_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "OrderTopicConfigV1 schema registration failed: {}",
            order_result.error
        );

        // Test that base structs with 'id' field compile and work correctly
        let topic = TopicV1WithIdV1::<OrderTopicConfigV1> {
            id: OrderTopicConfigV1::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };

        // Test that the schema constants are generated correctly
        assert_eq!(
            TopicV1WithIdV1::<()>::gts_schema_id().clone().into_string(),
            "gts.x.core.events.topic.v1~"
        );
        assert!(TopicV1WithIdV1::<()>::gts_base_schema_id().is_none());

        // Test serialization
        let serialized = serde_json::to_string(&topic).expect("Serialization should succeed");
        assert!(serialized.contains(
            "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~vendor.app._.topic.v1"
        ));
        assert!(serialized.contains("orders"));

        // Test instance ID generation
        let instance_id =
            TopicV1WithIdV1::<OrderTopicConfigV1>::gts_make_instance_id("test-instance");
        assert!(instance_id.as_ref().contains("gts.x.core.events.topic.v1~"));
        assert!(instance_id.as_ref().ends_with("test-instance"));

        // Validate instance against schema
        let mut topic_json = serde_json::to_value(&topic).unwrap();
        fix_null_config(&mut topic_json);
        let add_result = ops.add_entity(&topic_json, true);
        assert!(
            add_result.ok,
            "TopicV1WithIdV1 instance should validate: {}",
            add_result.error
        );
        let validate_result = ops.validate_instance(&topic.id);
        assert!(
            validate_result.ok,
            "TopicV1WithIdV1 validation failed: {}",
            validate_result.error
        );
    }

    #[test]
    fn test_base_struct_with_gts_id_field_compiles() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        let schema = TopicV1WithGtsIdV1::<()>::gts_schema_with_refs();
        let result = ops.add_schema(
            TopicV1WithGtsIdV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            &schema,
        );
        assert!(
            result.ok,
            "TopicV1WithGtsIdV1 schema registration failed: {}",
            result.error
        );
        let order_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "OrderTopicConfigV1 schema registration failed: {}",
            order_result.error
        );

        // Test that base structs with 'gts_id' field compile and work correctly
        let topic = TopicV1WithGtsIdV1::<OrderTopicConfigV1> {
            gts_id: OrderTopicConfigV1::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };

        // Test that the schema constants are generated correctly
        assert_eq!(
            TopicV1WithGtsIdV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.topic.v1~"
        );
        assert!(TopicV1WithGtsIdV1::<()>::gts_base_schema_id().is_none());

        // Test serialization
        let serialized = serde_json::to_string(&topic).expect("Serialization should succeed");
        assert!(serialized.contains(
            "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~vendor.app._.topic.v1"
        ));
        assert!(serialized.contains("orders"));

        // Validate instance against schema
        let mut topic_json = serde_json::to_value(&topic).unwrap();
        fix_null_config(&mut topic_json);
        let add_result = ops.add_entity(&topic_json, true);
        assert!(
            add_result.ok,
            "TopicV1WithGtsIdV1 instance should validate: {}",
            add_result.error
        );
        let validate_result = ops.validate_instance(&topic.gts_id);
        assert!(
            validate_result.ok,
            "TopicV1WithGtsIdV1 validation failed: {}",
            validate_result.error
        );
    }

    #[test]
    fn test_base_struct_with_gts_id_camel_field_compiles() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        let schema = TopicV1WithGtsIdCamelV1::<()>::gts_schema_with_refs();
        let result = ops.add_schema(
            TopicV1WithGtsIdCamelV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            &schema,
        );
        assert!(
            result.ok,
            "TopicV1WithGtsIdCamelV1 schema registration failed: {}",
            result.error
        );
        let order_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "OrderTopicConfigV1 schema registration failed: {}",
            order_result.error
        );

        // Test that base structs with 'gts_id' field (camelCase equivalent) compile and work correctly
        let topic = TopicV1WithGtsIdCamelV1::<OrderTopicConfigV1> {
            gts_id: OrderTopicConfigV1::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };

        // Test that the schema constants are generated correctly
        assert_eq!(
            TopicV1WithGtsIdCamelV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.topic.v1~"
        );
        assert!(TopicV1WithGtsIdCamelV1::<()>::gts_base_schema_id().is_none());

        // Test serialization
        let serialized = serde_json::to_string(&topic).expect("Serialization should succeed");
        assert!(serialized.contains(
            "gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~vendor.app._.topic.v1"
        ));
        assert!(serialized.contains("orders"));

        // Validate instance against schema
        let mut topic_json = serde_json::to_value(&topic).unwrap();
        fix_null_config(&mut topic_json);
        let add_result = ops.add_entity(&topic_json, true);
        assert!(
            add_result.ok,
            "TopicV1WithGtsIdCamelV1 instance should validate: {}",
            add_result.error
        );
        let validate_result = ops.validate_instance(&topic.gts_id);
        assert!(
            validate_result.ok,
            "TopicV1WithGtsIdCamelV1 validation failed: {}",
            validate_result.error
        );
    }

    #[test]
    fn test_base_struct_with_gts_type_field_compiles() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        let schema = TopicV1WithGtsTypeV1::<()>::gts_schema_with_refs();
        let result = ops.add_schema(
            TopicV1WithGtsTypeV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            &schema,
        );
        assert!(
            result.ok,
            "TopicV1WithGtsTypeV1 schema registration failed: {}",
            result.error
        );

        // Test that base structs with 'gts_type' field compile and work correctly
        let topic = TopicV1WithGtsTypeV1::<OrderTopicConfigV1> {
            gts_type: GtsSchemaId::new("gts.x.core.events.topic.v1~"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };

        // Test that the schema constants are generated correctly
        assert_eq!(
            TopicV1WithGtsTypeV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.topic.v1~"
        );
        assert!(TopicV1WithGtsTypeV1::<()>::gts_base_schema_id().is_none());

        // Test serialization
        let serialized = serde_json::to_string(&topic).expect("Serialization should succeed");
        assert!(serialized.contains("gts.x.core.events.topic.v1~"));
        assert!(serialized.contains("orders"));

        // Validate JSON structure matches schema (no GTS instance ID field, so verify structure)
        let topic_json = serde_json::to_value(&topic).unwrap();
        assert!(
            topic_json.get("gts_type").is_some(),
            "Should have gts_type field"
        );
        assert!(topic_json.get("name").is_some(), "Should have name field");
        assert!(
            topic_json.get("config").is_some(),
            "Should have config field"
        );
    }

    #[test]
    fn test_base_struct_with_gts_type_camel_field_compiles() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        let schema = TopicV1WithGtsTypeCamelV1::<()>::gts_schema_with_refs();
        let result = ops.add_schema(
            TopicV1WithGtsTypeCamelV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            &schema,
        );
        assert!(
            result.ok,
            "TopicV1WithGtsTypeCamelV1 schema registration failed: {}",
            result.error
        );

        // Test that base structs with 'gtsType' field compile and work correctly
        let topic = TopicV1WithGtsTypeCamelV1::<OrderTopicConfigV1> {
            gts_type: GtsSchemaId::new("gts.x.core.events.topic.v1~"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };

        // Test that the schema constants are generated correctly
        assert_eq!(
            TopicV1WithGtsTypeCamelV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            "gts.x.core.events.topic.v1~"
        );
        assert!(TopicV1WithGtsTypeCamelV1::<()>::gts_base_schema_id().is_none());

        // Test serialization
        let serialized = serde_json::to_string(&topic).expect("Serialization should succeed");
        assert!(serialized.contains("gts.x.core.events.topic.v1~"));
        assert!(serialized.contains("orders"));

        // Validate JSON structure matches schema (no GTS instance ID field, so verify structure)
        let topic_json = serde_json::to_value(&topic).unwrap();
        assert!(
            topic_json.get("gts_type").is_some(),
            "Should have gts_type field"
        );
        assert!(topic_json.get("name").is_some(), "Should have name field");
        assert!(
            topic_json.get("config").is_some(),
            "Should have config field"
        );
    }

    #[test]
    fn test_generic_base_struct_schema_consistency() {
        // Test that TopicV1<()> and TopicV1<OrderTopicConfigV1> generate EXACTLY the same base schema
        // The type parameter should not affect the schema structure for base types

        use gts::GtsSchema;

        let schema_unit = TopicV1::<()>::gts_schema_with_refs();
        let schema_concrete = TopicV1::<OrderTopicConfigV1>::gts_schema_with_refs();

        // Print schemas for debugging
        println!("TopicV1<()> schema:");
        println!("{}", serde_json::to_string_pretty(&schema_unit).unwrap());
        println!("\nTopicV1<OrderTopicConfigV1> schema:");
        println!(
            "{}",
            serde_json::to_string_pretty(&schema_concrete).unwrap()
        );

        // The schemas must be EXACTLY identical
        assert_eq!(
            schema_unit, schema_concrete,
            "TopicV1<()> and TopicV1<OrderTopicConfigV1> must generate identical schemas"
        );

        // Additional verification: check the $id is correct
        assert_eq!(
            schema_unit["$id"], "gts://gts.x.core.events.topic.v1~",
            "Schema should have TopicV1's schema ID"
        );

        // Verify it's a base schema (no allOf)
        assert!(
            !schema_unit.get("allOf").is_some(),
            "Base struct should not have allOf"
        );

        // Verify the config field is a simple placeholder
        assert_eq!(
            schema_unit["properties"]["config"]["type"], "object",
            "Generic field should be a simple object placeholder"
        );
    }

    #[test]
    fn test_instance_json_methods() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_three_level_event_schemas(&mut ops);

        // Test gts_instance_json(), gts_instance_json_as_string(), gts_instance_json_as_string_pretty()
        let event = BaseEventV1 {
            event_type: PlaceOrderDataV1::gts_schema_id().clone(),
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            tenant_id: Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap(),
            sequence_id: 42,
            payload: AuditPayloadV1 {
                user_agent: "Mozilla/5.0".to_string(),
                user_id: Uuid::parse_str("770e8400-e29b-41d4-a716-446655440000").unwrap(),
                ip_address: "192.168.1.1".to_string(),
                data: PlaceOrderDataV1 {
                    order_id: Uuid::parse_str("880e8400-e29b-41d4-a716-446655440000").unwrap(),
                    product_id: Uuid::parse_str("990e8400-e29b-41d4-a716-446655440000").unwrap(),
                },
            },
        };

        // Test gts_instance_json() - returns serde_json::Value
        let json_value = event.gts_instance_json();
        assert_eq!(json_value["sequence_id"], 42);
        assert_eq!(json_value["payload"]["ip_address"], "192.168.1.1");
        assert_eq!(
            json_value["payload"]["data"]["order_id"],
            "880e8400-e29b-41d4-a716-446655440000"
        );

        // Validate instance matches schema structure
        assert_eq!(
            json_value["type"],
            "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
        );
        assert!(json_value["payload"]["user_agent"].is_string());
        assert!(json_value["payload"]["data"]["product_id"].is_string());

        // Test gts_instance_json_as_string() - returns compact JSON string
        let json_string = event.gts_instance_json_as_string();
        assert!(json_string.contains("\"sequence_id\":42"));
        assert!(json_string.contains("192.168.1.1"));
        assert!(
            !json_string.contains('\n'),
            "Compact JSON should not contain newlines"
        );

        // Test gts_instance_json_as_string_pretty() - returns pretty-printed JSON string
        let json_pretty = event.gts_instance_json_as_string_pretty();
        assert!(json_pretty.contains("\"sequence_id\": 42"));
        assert!(json_pretty.contains("192.168.1.1"));
        assert!(
            json_pretty.contains('\n'),
            "Pretty JSON should contain newlines"
        );
    }

    #[test]
    fn test_schema_string_methods() {
        // Test gts_schema_with_refs_as_string() and gts_schema_with_refs_as_string_pretty()

        // Test compact string
        let schema_string = TopicV1::<()>::gts_schema_with_refs_as_string();
        assert!(schema_string.contains("\"$id\":\"gts://gts.x.core.events.topic.v1~\""));
        assert!(
            !schema_string.contains('\n'),
            "Compact JSON should not contain newlines"
        );

        // Test pretty string
        let schema_pretty = TopicV1::<()>::gts_schema_with_refs_as_string_pretty();
        assert!(schema_pretty.contains("\"$id\": \"gts://gts.x.core.events.topic.v1~\""));
        assert!(
            schema_pretty.contains('\n'),
            "Pretty JSON should contain newlines"
        );
    }

    #[test]
    fn test_gts_base_schema_id_returns_gts_schema_id_type() {
        // Verify gts_base_schema_id() returns the correct type

        // Base struct should return None
        let base_id: Option<&gts::gts::GtsSchemaId> = TopicV1::<()>::gts_base_schema_id();
        assert!(base_id.is_none());

        // Child struct should return Some with correct value
        let child_id: Option<&gts::gts::GtsSchemaId> = OrderTopicConfigV1::gts_base_schema_id();
        assert!(child_id.is_some());
        assert_eq!(child_id.unwrap().as_ref(), "gts.x.core.events.topic.v1~");

        // Verify it's usable as GtsSchemaId
        let parent_id = child_id.unwrap();
        assert!(parent_id.as_ref().ends_with('~'));
    }

    // =============================================================================
    // Tests for 2-level inheritance (`BaseEventV1` -> `SimplePayloadV1`)
    // =============================================================================

    #[test]
    fn test_two_level_inheritance_schema_ids() {
        // Verify schema IDs for 2-level inheritance chain
        assert_eq!(
            BaseEventV1::<()>::gts_schema_id().as_ref(),
            "gts.x.core.events.type.v1~"
        );
        assert_eq!(
            SimplePayloadV1::gts_schema_id().as_ref(),
            "gts.x.core.events.type.v1~x.core.simple.event.v1~"
        );

        // Base should have no parent
        assert!(BaseEventV1::<()>::gts_base_schema_id().is_none());

        // SimplePayloadV1 should have BaseEventV1 as parent
        assert_eq!(
            SimplePayloadV1::gts_base_schema_id().map(AsRef::as_ref),
            Some("gts.x.core.events.type.v1~")
        );
    }

    #[test]
    fn test_two_level_inheritance_field_path() {
        // Register schemas for validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_two_level_event_schemas(&mut ops);

        // Test that 2-level inheritance produces correct field nesting:
        // BaseEventV1.payload -> SimplePayloadV1 fields
        let event = BaseEventV1 {
            event_type: SimplePayloadV1::gts_schema_id().clone(),
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            tenant_id: Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap(),
            sequence_id: 100,
            payload: SimplePayloadV1 {
                message: "System started".to_string(),
                severity: 3,
            },
        };

        let json = serde_json::to_value(&event).unwrap();

        // Verify top-level fields from BaseEventV1
        assert_eq!(
            json["type"],
            "gts.x.core.events.type.v1~x.core.simple.event.v1~"
        );
        assert_eq!(json["id"], "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(json["tenant_id"], "660e8400-e29b-41d4-a716-446655440000");
        assert_eq!(json["sequence_id"], 100);

        // Verify payload is nested object with SimplePayloadV1 fields
        let payload = json.get("payload").expect("payload field should exist");
        assert!(payload.is_object(), "payload should be an object");
        assert_eq!(payload["message"], "System started");
        assert_eq!(payload["severity"], 3);

        // Verify field path: accessing nested fields requires going through payload
        assert!(
            json.get("message").is_none(),
            "message should NOT be at top level"
        );
        assert!(
            json.get("severity").is_none(),
            "severity should NOT be at top level"
        );
    }

    #[test]
    fn test_two_level_vs_three_level_field_paths() {
        // Register schemas for both 2-level and 3-level validation
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_two_level_event_schemas(&mut ops);
        // Also register 3-level schemas (AuditPayloadV1 and PlaceOrderDataV1)
        let audit_schema = AuditPayloadV1::<()>::gts_schema_with_refs();
        let audit_result = ops.add_schema(
            AuditPayloadV1::<()>::gts_schema_id().clone().into_string(),
            &audit_schema,
        );
        assert!(
            audit_result.ok,
            "AuditPayloadV1 schema registration failed: {}",
            audit_result.error
        );
        let order_schema = PlaceOrderDataV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            PlaceOrderDataV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "PlaceOrderDataV1 schema registration failed: {}",
            order_result.error
        );

        // Compare field paths between 2-level and 3-level inheritance

        // 2-level: BaseEventV1 -> SimplePayloadV1
        let two_level = BaseEventV1 {
            event_type: SimplePayloadV1::gts_schema_id().clone(),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 1,
            payload: SimplePayloadV1 {
                message: "test".to_string(),
                severity: 1,
            },
        };

        // 3-level: BaseEventV1 -> AuditPayloadV1 -> PlaceOrderDataV1
        let three_level = BaseEventV1 {
            event_type: PlaceOrderDataV1::gts_schema_id().clone(),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 2,
            payload: AuditPayloadV1 {
                user_agent: "agent".to_string(),
                user_id: Uuid::new_v4(),
                ip_address: "127.0.0.1".to_string(),
                data: PlaceOrderDataV1 {
                    order_id: Uuid::new_v4(),
                    product_id: Uuid::new_v4(),
                },
            },
        };

        let two_json = serde_json::to_value(&two_level).unwrap();
        let three_json = serde_json::to_value(&three_level).unwrap();

        // Validate type fields match their respective schemas
        assert_eq!(
            two_json["type"],
            "gts.x.core.events.type.v1~x.core.simple.event.v1~"
        );
        assert_eq!(
            three_json["type"],
            "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
        );

        // 2-level field path: payload.message, payload.severity
        assert!(two_json["payload"]["message"].is_string());
        assert!(two_json["payload"]["severity"].is_number());
        assert!(
            two_json["payload"].get("data").is_none(),
            "2-level should not have data field"
        );

        // 3-level field path: payload.user_agent, payload.data.order_id
        assert!(three_json["payload"]["user_agent"].is_string());
        assert!(three_json["payload"]["data"]["order_id"].is_string());
        assert!(
            three_json["payload"].get("message").is_none(),
            "3-level should not have message field"
        );
    }

    #[test]
    fn test_two_level_inheritance_schema_structure() {
        // Verify SimplePayloadV1 schema has correct allOf inheritance
        // Child properties should be nested under the parent's generic field (payload)
        let schema = SimplePayloadV1::gts_schema_with_refs();

        // Should have $id
        assert_eq!(
            schema["$id"],
            "gts://gts.x.core.events.type.v1~x.core.simple.event.v1~"
        );

        // Should have allOf with parent reference
        let all_of = schema
            .get("allOf")
            .expect("SimplePayloadV1 should have allOf");
        assert!(all_of.is_array(), "allOf should be an array");

        // First element should be $ref to BaseEventV1
        let first = &all_of[0];
        assert_eq!(first["$ref"], "gts://gts.x.core.events.type.v1~");

        // Second element should have properties nested under "payload"
        // (the parent's generic field)
        let second = &all_of[1];
        let props = second.get("properties").expect("Should have properties");

        // Properties should be nested under "payload" (parent's generic field)
        let payload_prop = props.get("payload").expect("Should have payload property");
        let payload_props = payload_prop
            .get("properties")
            .expect("payload should have properties");

        assert!(
            payload_props.get("message").is_some(),
            "Should have message property nested under payload"
        );
        assert!(
            payload_props.get("severity").is_some(),
            "Should have severity property nested under payload"
        );

        // Should NOT have BaseEventV1 fields or child fields at root level
        assert!(
            props.get("type").is_none(),
            "Should NOT have type in own properties"
        );
        assert!(
            props.get("message").is_none(),
            "Should NOT have message at root level - should be nested under payload"
        );
    }

    #[test]
    fn test_two_level_inheritance_validation() {
        // Test that 2-level inheritance schema registration works correctly with GtsOps
        let mut ops = gts::GtsOps::new(None, None, 0);

        // Register base schema
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();
        let base_result = ops.add_schema(
            BaseEventV1::<()>::gts_schema_id().clone().into_string(),
            &base_schema,
        );
        assert!(
            base_result.ok,
            "Base schema registration should succeed: {}",
            base_result.error
        );

        // Register SimplePayloadV1 schema
        let simple_schema = SimplePayloadV1::gts_schema_with_refs();
        let simple_result = ops.add_schema(
            SimplePayloadV1::gts_schema_id().clone().into_string(),
            &simple_schema,
        );
        assert!(
            simple_result.ok,
            "SimplePayloadV1 schema registration should succeed: {}",
            simple_result.error
        );

        // Verify schemas are retrievable
        let base_entity = ops.get_entity(BaseEventV1::<()>::gts_schema_id().as_ref());
        assert!(base_entity.ok, "Base schema should be retrievable");

        let simple_entity = ops.get_entity(SimplePayloadV1::gts_schema_id().as_ref());
        assert!(
            simple_entity.ok,
            "SimplePayloadV1 schema should be retrievable"
        );

        // Verify the SimplePayloadV1 schema has correct allOf reference
        let simple_content = simple_entity.content.expect("Should have content");
        let all_of = simple_content.get("allOf").expect("Should have allOf");
        assert_eq!(
            all_of[0]["$ref"], "gts://gts.x.core.events.type.v1~",
            "allOf should reference base schema"
        );
    }

    // =============================================================================
    // Comprehensive schema validation for all instance types
    // =============================================================================

    /// Helper to register all schemas needed for `TopicV1` hierarchy
    fn register_topic_schemas(ops: &mut gts::GtsOps) {
        let base_schema = TopicV1::<()>::gts_schema_with_refs();
        let base_result = ops.add_schema(
            TopicV1::<()>::gts_schema_id().clone().into_string(),
            &base_schema,
        );
        assert!(
            base_result.ok,
            "TopicV1 schema registration failed: {}",
            base_result.error
        );

        let order_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "OrderTopicConfigV1 schema registration failed: {}",
            order_result.error
        );
    }

    /// Helper to fix null config fields to empty objects for schema validation
    fn fix_null_config(json: &mut serde_json::Value) {
        if let Some(config_obj) = json.get_mut("config")
            && *config_obj == serde_json::Value::Null
        {
            *config_obj = serde_json::json!({});
        }
    }

    #[test]
    fn test_all_topic_instances_match_schema() {
        // Comprehensive test: all TopicV1 instances must validate against their schemas
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_topic_schemas(&mut ops);

        // Instance 1: TopicV1<OrderTopicConfigV1> from test_unit_struct_instantiation_and_serialization
        let topic1 = TopicV1::<OrderTopicConfigV1> {
            id: OrderTopicConfigV1::gts_make_instance_id("test.test._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };
        let mut topic1_json = serde_json::to_value(&topic1).unwrap();
        fix_null_config(&mut topic1_json);
        let add1 = ops.add_entity(&topic1_json, true);
        assert!(
            add1.ok,
            "TopicV1<OrderTopicConfigV1> instance 1 should validate: {}",
            add1.error
        );
        let validate1 = ops.validate_instance(&topic1.id);
        assert!(
            validate1.ok,
            "TopicV1<OrderTopicConfigV1> instance 1 validation failed: {}",
            validate1.error
        );

        // Instance 2: TopicV1<OrderTopicConfigV1> from test_empty_struct_serialization_with_nested_empty
        let topic2 = TopicV1::<OrderTopicConfigV1> {
            id: OrderTopicConfigV1::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };
        let mut topic2_json = serde_json::to_value(&topic2).unwrap();
        fix_null_config(&mut topic2_json);
        let add2 = ops.add_entity(&topic2_json, true);
        assert!(
            add2.ok,
            "TopicV1<OrderTopicConfigV1> instance 2 should validate: {}",
            add2.error
        );
        let validate2 = ops.validate_instance(&topic2.id);
        assert!(
            validate2.ok,
            "TopicV1<OrderTopicConfigV1> instance 2 validation failed: {}",
            validate2.error
        );

        // Instance 3: TopicV1<()> base type instance
        let topic3 = TopicV1::<()> {
            id: TopicV1::<()>::gts_make_instance_id("vendor.app.base.topic.v1"),
            name: "base-topic".to_string(),
            description: Some("Base topic instance".to_string()),
            config: (),
        };
        let mut topic3_json = serde_json::to_value(&topic3).unwrap();
        fix_null_config(&mut topic3_json);
        let add3 = ops.add_entity(&topic3_json, true);
        assert!(
            add3.ok,
            "TopicV1<()> instance should validate: {}",
            add3.error
        );
        let validate3 = ops.validate_instance(&topic3.id);
        assert!(
            validate3.ok,
            "TopicV1<()> instance validation failed: {}",
            validate3.error
        );
    }

    #[test]
    fn test_topic_with_id_variants_match_schema() {
        // Test all TopicV1WithId* variants validate against their schemas
        let mut ops = gts::GtsOps::new(None, None, 0);

        // Register TopicV1WithIdV1 schema
        let schema = TopicV1WithIdV1::<()>::gts_schema_with_refs();
        let result = ops.add_schema(
            TopicV1WithIdV1::<()>::gts_schema_id().clone().into_string(),
            &schema,
        );
        assert!(
            result.ok,
            "TopicV1WithIdV1 schema registration failed: {}",
            result.error
        );

        // Register OrderTopicConfigV1 schema (needed for nested type)
        let order_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "OrderTopicConfigV1 schema registration failed: {}",
            order_result.error
        );

        // Instance from test_base_struct_with_id_field_compiles
        let topic = TopicV1WithIdV1::<OrderTopicConfigV1> {
            id: OrderTopicConfigV1::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };
        let mut topic_json = serde_json::to_value(&topic).unwrap();
        fix_null_config(&mut topic_json);
        let add = ops.add_entity(&topic_json, true);
        assert!(
            add.ok,
            "TopicV1WithIdV1<OrderTopicConfigV1> instance should validate: {}",
            add.error
        );
        let validate = ops.validate_instance(&topic.id);
        assert!(
            validate.ok,
            "TopicV1WithIdV1<OrderTopicConfigV1> validation failed: {}",
            validate.error
        );
    }

    #[test]
    fn test_topic_with_gts_id_variants_match_schema() {
        // Test TopicV1WithGtsIdV1 and TopicV1WithGtsIdCamelV1 variants
        let mut ops = gts::GtsOps::new(None, None, 0);

        // Register schemas
        let schema1 = TopicV1WithGtsIdV1::<()>::gts_schema_with_refs();
        let result1 = ops.add_schema(
            TopicV1WithGtsIdV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            &schema1,
        );
        assert!(
            result1.ok,
            "TopicV1WithGtsIdV1 schema registration failed: {}",
            result1.error
        );

        let order_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "OrderTopicConfigV1 schema registration failed: {}",
            order_result.error
        );

        // Instance from test_base_struct_with_gts_id_field_compiles
        let topic1 = TopicV1WithGtsIdV1::<OrderTopicConfigV1> {
            gts_id: OrderTopicConfigV1::gts_make_instance_id("vendor.app._.topic.v1"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };
        let mut topic1_json = serde_json::to_value(&topic1).unwrap();
        fix_null_config(&mut topic1_json);
        let add1 = ops.add_entity(&topic1_json, true);
        assert!(
            add1.ok,
            "TopicV1WithGtsIdV1<OrderTopicConfigV1> instance should validate: {}",
            add1.error
        );
        let validate1 = ops.validate_instance(&topic1.gts_id);
        assert!(
            validate1.ok,
            "TopicV1WithGtsIdV1<OrderTopicConfigV1> validation failed: {}",
            validate1.error
        );

        // Instance from test_base_struct_with_gts_id_camel_field_compiles
        let topic2 = TopicV1WithGtsIdCamelV1::<OrderTopicConfigV1> {
            gts_id: OrderTopicConfigV1::gts_make_instance_id("vendor.app.camel.topic.v1"),
            name: "orders-camel".to_string(),
            description: Some("Order events camel".to_string()),
            config: OrderTopicConfigV1,
        };
        let mut topic2_json = serde_json::to_value(&topic2).unwrap();
        fix_null_config(&mut topic2_json);
        let add2 = ops.add_entity(&topic2_json, true);
        assert!(
            add2.ok,
            "TopicV1WithGtsIdCamelV1<OrderTopicConfigV1> instance should validate: {}",
            add2.error
        );
        let validate2 = ops.validate_instance(&topic2.gts_id);
        assert!(
            validate2.ok,
            "TopicV1WithGtsIdCamelV1<OrderTopicConfigV1> validation failed: {}",
            validate2.error
        );
    }

    #[test]
    fn test_topic_with_gts_type_variants_match_schema() {
        // Test TopicV1WithGtsTypeV1 and TopicV1WithGtsTypeCamelV1 variants
        let mut ops = gts::GtsOps::new(None, None, 0);

        // Register schemas
        let schema1 = TopicV1WithGtsTypeV1::<()>::gts_schema_with_refs();
        let result1 = ops.add_schema(
            TopicV1WithGtsTypeV1::<()>::gts_schema_id()
                .clone()
                .into_string(),
            &schema1,
        );
        assert!(
            result1.ok,
            "TopicV1WithGtsTypeV1 schema registration failed: {}",
            result1.error
        );

        let order_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            OrderTopicConfigV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "OrderTopicConfigV1 schema registration failed: {}",
            order_result.error
        );

        // These structs use gts_type field (GtsSchemaId), not gts_id (GtsInstanceId)
        // They don't have a GTS instance ID field, so we can't use validate_instance
        // Instead, we verify the JSON structure matches the schema properties

        let topic1 = TopicV1WithGtsTypeV1::<OrderTopicConfigV1> {
            gts_type: GtsSchemaId::new("gts.x.core.events.topic.v1~"),
            name: "orders".to_string(),
            description: Some("Order events".to_string()),
            config: OrderTopicConfigV1,
        };
        let topic1_json = serde_json::to_value(&topic1).unwrap();

        // Verify JSON structure has expected fields from schema
        assert!(
            topic1_json.get("gts_type").is_some(),
            "Should have gts_type field"
        );
        assert!(topic1_json.get("name").is_some(), "Should have name field");
        assert!(
            topic1_json.get("description").is_some(),
            "Should have description field"
        );
        assert!(
            topic1_json.get("config").is_some(),
            "Should have config field"
        );

        let topic2 = TopicV1WithGtsTypeCamelV1::<OrderTopicConfigV1> {
            gts_type: GtsSchemaId::new("gts.x.core.events.topic.v1~"),
            name: "orders-camel".to_string(),
            description: Some("Order events camel".to_string()),
            config: OrderTopicConfigV1,
        };
        let topic2_json = serde_json::to_value(&topic2).unwrap();

        // Verify JSON structure has expected fields from schema
        assert!(
            topic2_json.get("gts_type").is_some(),
            "Should have gts_type field"
        );
        assert!(topic2_json.get("name").is_some(), "Should have name field");
        assert!(
            topic2_json.get("description").is_some(),
            "Should have description field"
        );
        assert!(
            topic2_json.get("config").is_some(),
            "Should have config field"
        );
    }

    #[test]
    fn test_two_level_instance_matches_schema() {
        // Test 2-level inheritance instance (`BaseEventV1` -> `SimplePayloadV1`) matches schema
        let mut ops = gts::GtsOps::new(None, None, 0);

        // Register schemas
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();
        let base_result = ops.add_schema(
            BaseEventV1::<()>::gts_schema_id().clone().into_string(),
            &base_schema,
        );
        assert!(
            base_result.ok,
            "BaseEventV1 schema registration failed: {}",
            base_result.error
        );

        let simple_schema = SimplePayloadV1::gts_schema_with_refs();
        let simple_result = ops.add_schema(
            SimplePayloadV1::gts_schema_id().clone().into_string(),
            &simple_schema,
        );
        assert!(
            simple_result.ok,
            "SimplePayloadV1 schema registration failed: {}",
            simple_result.error
        );

        // Create 2-level instance from test_two_level_inheritance_field_path
        let event = BaseEventV1 {
            event_type: SimplePayloadV1::gts_schema_id().clone(),
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            tenant_id: Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap(),
            sequence_id: 100,
            payload: SimplePayloadV1 {
                message: "System started".to_string(),
                severity: 3,
            },
        };

        let event_json = serde_json::to_value(&event).unwrap();

        // Verify JSON structure matches expected field paths
        assert_eq!(
            event_json["type"],
            "gts.x.core.events.type.v1~x.core.simple.event.v1~"
        );
        assert_eq!(event_json["sequence_id"], 100);
        assert_eq!(event_json["payload"]["message"], "System started");
        assert_eq!(event_json["payload"]["severity"], 3);

        // Verify field nesting is correct (payload contains SimplePayloadV1 fields)
        assert!(
            event_json.get("message").is_none(),
            "message should be nested in payload"
        );
        assert!(
            event_json.get("severity").is_none(),
            "severity should be nested in payload"
        );
    }

    #[test]
    fn test_three_level_instance_matches_schema() {
        // Test 3-level inheritance instance (`BaseEventV1` -> `AuditPayloadV1` -> `PlaceOrderDataV1`) matches schema
        let mut ops = gts::GtsOps::new(None, None, 0);

        // Register all schemas in the inheritance chain
        let base_schema = BaseEventV1::<()>::gts_schema_with_refs();
        let base_result = ops.add_schema(
            BaseEventV1::<()>::gts_schema_id().clone().into_string(),
            &base_schema,
        );
        assert!(
            base_result.ok,
            "BaseEventV1 schema registration failed: {}",
            base_result.error
        );

        let audit_schema = AuditPayloadV1::<()>::gts_schema_with_refs();
        let audit_result = ops.add_schema(
            AuditPayloadV1::<()>::gts_schema_id().clone().into_string(),
            &audit_schema,
        );
        assert!(
            audit_result.ok,
            "AuditPayloadV1 schema registration failed: {}",
            audit_result.error
        );

        let order_schema = PlaceOrderDataV1::gts_schema_with_refs();
        let order_result = ops.add_schema(
            PlaceOrderDataV1::gts_schema_id().clone().into_string(),
            &order_schema,
        );
        assert!(
            order_result.ok,
            "PlaceOrderDataV1 schema registration failed: {}",
            order_result.error
        );

        // Create 3-level instance from test_runtime_serialization
        let event = BaseEventV1 {
            event_type: PlaceOrderDataV1::gts_schema_id().clone(),
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            tenant_id: Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap(),
            sequence_id: 42,
            payload: AuditPayloadV1 {
                user_agent: "Mozilla/5.0".to_string(),
                user_id: Uuid::parse_str("770e8400-e29b-41d4-a716-446655440000").unwrap(),
                ip_address: "192.168.1.1".to_string(),
                data: PlaceOrderDataV1 {
                    order_id: Uuid::parse_str("880e8400-e29b-41d4-a716-446655440000").unwrap(),
                    product_id: Uuid::parse_str("990e8400-e29b-41d4-a716-446655440000").unwrap(),
                },
            },
        };

        let event_json = serde_json::to_value(&event).unwrap();

        // Verify JSON structure matches expected field paths for 3-level nesting
        assert_eq!(
            event_json["type"],
            "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
        );
        assert_eq!(event_json["sequence_id"], 42);

        // Level 2: payload contains AuditPayloadV1 fields
        assert_eq!(event_json["payload"]["user_agent"], "Mozilla/5.0");
        assert_eq!(event_json["payload"]["ip_address"], "192.168.1.1");

        // Level 3: payload.data contains PlaceOrderDataV1 fields
        assert_eq!(
            event_json["payload"]["data"]["order_id"],
            "880e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            event_json["payload"]["data"]["product_id"],
            "990e8400-e29b-41d4-a716-446655440000"
        );

        // Verify field nesting is correct
        assert!(
            event_json.get("user_agent").is_none(),
            "user_agent should be nested in payload"
        );
        assert!(
            event_json.get("order_id").is_none(),
            "order_id should be nested in payload.data"
        );
        assert!(
            event_json["payload"].get("order_id").is_none(),
            "order_id should be in payload.data, not payload"
        );
    }

    // =============================================================================
    // Test for non-generic child extending generic base - schema nesting issue
    // =============================================================================

    /// This test validates that non-generic child types extending generic base types
    /// have their properties correctly nested under the parent's generic field path.
    ///
    /// For example, if `BaseEventV1<P>` has a `payload: P` field, and `SimplePayloadV1`
    /// extends it with `message` and `severity` fields, the schema should nest these
    /// under `payload`, not at the root level.
    ///
    /// Instance JSON structure:
    /// ```json
    /// {
    ///   "type": "...",
    ///   "id": "...",
    ///   "payload": {
    ///     "message": "...",
    ///     "severity": 3
    ///   }
    /// }
    /// ```
    ///
    /// Schema should have:
    /// ```json
    /// {
    ///   "allOf": [
    ///     { "$ref": "gts://gts.x.core.events.type.v1~" },
    ///     {
    ///       "properties": {
    ///         "payload": {
    ///           "properties": {
    ///             "message": { "type": "string" },
    ///             "severity": { "type": "integer" }
    ///           }
    ///         }
    ///       }
    ///     }
    ///   ]
    /// }
    /// ```
    #[test]
    fn test_non_generic_child_schema_nests_properties_under_parent_generic_field() {
        // Register schemas
        let mut ops = gts::GtsOps::new(None, None, 0);
        register_two_level_event_schemas(&mut ops);

        // Create a valid instance
        let event = BaseEventV1 {
            event_type: SimplePayloadV1::gts_schema_id().clone(),
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            tenant_id: Uuid::parse_str("660e8400-e29b-41d4-a716-446655440000").unwrap(),
            sequence_id: 100,
            payload: SimplePayloadV1 {
                message: "System started".to_string(),
                severity: 3,
            },
        };

        let event_json = serde_json::to_value(&event).unwrap();

        // The instance has payload.message and payload.severity (nested)
        assert!(event_json["payload"]["message"].is_string());
        assert!(event_json["payload"]["severity"].is_number());

        // Get the SimplePayloadV1 schema
        let schema = SimplePayloadV1::gts_schema_with_refs();
        println!(
            "SimplePayloadV1 schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );

        // The schema's allOf[1] should have properties nested under "payload"
        // to match the instance structure
        let all_of = schema.get("allOf").expect("Should have allOf");
        let child_schema = &all_of[1];
        let props = child_schema
            .get("properties")
            .expect("Should have properties");

        // EXPECTED BEHAVIOR: properties should be nested under "payload"
        // because BaseEventV1 has a generic field "payload: P"
        let payload_prop = props.get("payload");
        assert!(
            payload_prop.is_some(),
            "Child schema should nest properties under 'payload' field. \
             Got properties: {}",
            serde_json::to_string_pretty(&props).unwrap()
        );

        // The nested payload should have the child's properties
        let payload_props = payload_prop
            .unwrap()
            .get("properties")
            .expect("payload should have properties");
        assert!(
            payload_props.get("message").is_some(),
            "payload.properties should have 'message'"
        );
        assert!(
            payload_props.get("severity").is_some(),
            "payload.properties should have 'severity'"
        );
    }

    // =============================================================================
    // Schema field path validation for all nested schemas
    // =============================================================================

    /// Helper function to verify schema nesting structure and field paths
    fn verify_schema_field_path(
        schema: &serde_json::Value,
        expected_id: &str,
        expected_parent_ref: Option<&str>,
        expected_field_path: &[&str],
        expected_properties: &[&str],
    ) {
        // Verify $id
        assert_eq!(
            schema["$id"],
            format!("gts://{}", expected_id),
            "Schema $id mismatch"
        );

        if let Some(parent_ref) = expected_parent_ref {
            // Child schema - should have allOf
            let all_of = schema.get("allOf").expect("Child schema should have allOf");
            assert_eq!(
                all_of[0]["$ref"],
                format!("gts://{}", parent_ref),
                "allOf $ref should point to parent"
            );

            // Navigate to nested properties through field path
            let child_schema = &all_of[1];
            let mut current = child_schema
                .get("properties")
                .expect("Should have properties");

            for (i, field) in expected_field_path.iter().enumerate() {
                let field_obj = current.get(*field);
                assert!(
                    field_obj.is_some(),
                    "Missing field '{}' at path level {} in schema. Got: {}",
                    field,
                    i,
                    serde_json::to_string_pretty(&current).unwrap()
                );
                current = field_obj.unwrap();
                if i < expected_field_path.len() - 1 {
                    // Not the last field, navigate to its properties
                    current = current.get("properties").unwrap_or(current);
                }
            }

            // Now current should be the innermost field object, get its properties
            let innermost_props = current
                .get("properties")
                .expect("Innermost should have properties");

            // Verify expected properties exist
            for prop in expected_properties {
                assert!(
                    innermost_props.get(*prop).is_some(),
                    "Missing property '{}' in nested schema at path {:?}. Got: {}",
                    prop,
                    expected_field_path,
                    serde_json::to_string_pretty(&innermost_props).unwrap()
                );
            }
        } else {
            // Base schema - should NOT have allOf
            assert!(
                schema.get("allOf").is_none(),
                "Base schema should not have allOf"
            );

            // Properties should be at root level
            let props = schema
                .get("properties")
                .expect("Base schema should have properties");
            for prop in expected_properties {
                assert!(
                    props.get(*prop).is_some(),
                    "Missing property '{}' in base schema. Got: {}",
                    prop,
                    serde_json::to_string_pretty(&props).unwrap()
                );
            }
        }
    }

    #[test]
    fn test_base_event_v1_schema_field_path() {
        // BaseEventV1 is a base type - properties at root level
        let schema = BaseEventV1::<()>::gts_schema_with_refs();
        println!(
            "BaseEventV1 schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );

        verify_schema_field_path(
            &schema,
            "gts.x.core.events.type.v1~",
            None, // No parent
            &[],  // No nesting path
            &["type", "id", "tenant_id", "sequence_id", "payload"],
        );

        // Verify additionalProperties: false at root
        assert_eq!(
            schema["additionalProperties"], false,
            "Base schema should have additionalProperties: false"
        );
    }

    #[test]
    fn test_audit_payload_v1_schema_field_path() {
        // AuditPayloadV1<()> is a GENERIC type - its own properties are at root level in allOf[1]
        // (not nested under parent's generic field)
        // Only non-generic children nest under parent's generic field
        let schema = AuditPayloadV1::<()>::gts_schema_with_refs();
        println!(
            "AuditPayloadV1 schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );

        // Verify $id
        assert_eq!(
            schema["$id"],
            "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~"
        );

        // Should have allOf with parent reference
        let all_of = schema.get("allOf").expect("Should have allOf");
        assert_eq!(all_of[0]["$ref"], "gts://gts.x.core.events.type.v1~");

        // Generic type's own properties are at root level of allOf[1], not nested
        let child_schema = &all_of[1];
        let props = child_schema
            .get("properties")
            .expect("Should have properties");

        // Properties should be at root level (not nested under "payload")
        assert!(props.get("user_agent").is_some(), "Should have user_agent");
        assert!(props.get("user_id").is_some(), "Should have user_id");
        assert!(props.get("ip_address").is_some(), "Should have ip_address");
        assert!(
            props.get("data").is_some(),
            "Should have data (generic field placeholder)"
        );

        // data should be a simple {"type": "object"} placeholder for the generic field
        assert_eq!(props["data"]["type"], "object");
    }

    #[test]
    fn test_place_order_data_v1_schema_field_path() {
        // PlaceOrderDataV1 is a NON-GENERIC child extending AuditPayloadV1
        // Its properties should be nested under "data" (AuditPayloadV1's generic field)
        let schema = PlaceOrderDataV1::gts_schema_with_refs();
        println!(
            "PlaceOrderDataV1 schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );

        // Verify $id
        assert_eq!(
            schema["$id"],
            "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~"
        );

        // Should have allOf with parent reference
        let all_of = schema.get("allOf").expect("Should have allOf");
        assert_eq!(
            all_of[0]["$ref"],
            "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~"
        );

        // Non-generic child's properties should be nested under parent's generic field ("data")
        let child_schema = &all_of[1];
        let props = child_schema
            .get("properties")
            .expect("Should have properties");

        // Should have "data" field (parent's generic field)
        let data_prop = props.get("data").expect("Should have data property");
        let data_props = data_prop
            .get("properties")
            .expect("data should have properties");

        // Verify PlaceOrderDataV1's properties are nested under "data"
        assert!(
            data_props.get("order_id").is_some(),
            "data.properties should have order_id"
        );
        assert!(
            data_props.get("product_id").is_some(),
            "data.properties should have product_id"
        );

        // Verify additionalProperties: false on nested data
        assert_eq!(
            data_prop["additionalProperties"], false,
            "Nested data should have additionalProperties: false"
        );
    }

    #[test]
    fn test_simple_payload_v1_schema_field_path() {
        // SimplePayloadV1 extends BaseEventV1 - properties nested under "payload"
        let schema = SimplePayloadV1::gts_schema_with_refs();
        println!(
            "SimplePayloadV1 schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );

        verify_schema_field_path(
            &schema,
            "gts.x.core.events.type.v1~x.core.simple.event.v1~",
            Some("gts.x.core.events.type.v1~"),
            &["payload"], // Properties nested under "payload"
            &["message", "severity"],
        );
    }

    #[test]
    fn test_topic_v1_schema_field_path() {
        // TopicV1 is a base type - properties at root level
        let schema = TopicV1::<()>::gts_schema_with_refs();
        println!(
            "TopicV1 schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );

        verify_schema_field_path(
            &schema,
            "gts.x.core.events.topic.v1~",
            None, // No parent
            &[],  // No nesting path
            &["name", "description"],
        );

        // Verify additionalProperties: false at root
        assert_eq!(
            schema["additionalProperties"], false,
            "Base schema should have additionalProperties: false"
        );
    }

    #[test]
    fn test_order_topic_config_v1_schema_field_path() {
        // OrderTopicConfigV1 is a unit struct extending TopicV1
        // It has no properties of its own, but should still have proper nesting structure
        let schema = OrderTopicConfigV1::gts_schema_with_refs();
        println!(
            "OrderTopicConfigV1 schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );

        // Verify $id
        assert_eq!(
            schema["$id"],
            "gts://gts.x.core.events.topic.v1~x.commerce.orders.topic.v1~"
        );

        // Should have allOf with parent reference
        let all_of = schema.get("allOf").expect("Should have allOf");
        assert_eq!(all_of[0]["$ref"], "gts://gts.x.core.events.topic.v1~");

        // Second element should have properties nested under "config" (parent's generic field)
        let child_schema = &all_of[1];
        let props = child_schema
            .get("properties")
            .expect("Should have properties");

        // Should have "config" field (parent's generic field)
        let config_prop = props.get("config");
        assert!(
            config_prop.is_some(),
            "Unit struct child should nest under parent's generic field 'config'. Got: {}",
            serde_json::to_string_pretty(&props).unwrap()
        );

        // Config should have additionalProperties: false (empty object with no extra fields allowed)
        let config = config_prop.unwrap();
        assert_eq!(
            config["additionalProperties"], false,
            "Nested config should have additionalProperties: false"
        );
    }

    #[test]
    fn test_all_nested_schemas_have_additional_properties_false() {
        // Verify all nested schemas have additionalProperties: false at the correct level
        // Note: Generic types (like AuditPayloadV1<()>) have properties at root level of allOf[1],
        // only non-generic children nest under parent's generic field

        // Non-generic child: SimplePayloadV1 (extends BaseEventV1)
        // Properties nested under "payload" (parent's generic field)
        let simple_schema = SimplePayloadV1::gts_schema_with_refs();
        let simple_payload = &simple_schema["allOf"][1]["properties"]["payload"];
        assert_eq!(
            simple_payload["additionalProperties"], false,
            "SimplePayloadV1 nested payload should have additionalProperties: false"
        );

        // Generic type: AuditPayloadV1<()> - properties at root level of allOf[1]
        // The "data" field is a generic placeholder, not a nested child
        // So we don't check for additionalProperties on root level (it's not there for generic types)
        let audit_schema = AuditPayloadV1::<()>::gts_schema_with_refs();
        let audit_props = &audit_schema["allOf"][1]["properties"];
        // Verify "data" is a simple placeholder (no additionalProperties)
        assert_eq!(
            audit_props["data"]["type"], "object",
            "AuditPayloadV1 data field should be a simple object placeholder"
        );

        // Non-generic child: PlaceOrderDataV1 (extends AuditPayloadV1)
        // Properties nested under "data" (parent's generic field)
        let order_schema = PlaceOrderDataV1::gts_schema_with_refs();
        let order_data = &order_schema["allOf"][1]["properties"]["data"];
        assert_eq!(
            order_data["additionalProperties"], false,
            "PlaceOrderDataV1 nested data should have additionalProperties: false"
        );

        // Unit struct: OrderTopicConfigV1 (extends TopicV1)
        // Properties nested under "config" (parent's generic field)
        let topic_schema = OrderTopicConfigV1::gts_schema_with_refs();
        let topic_config = &topic_schema["allOf"][1]["properties"]["config"];
        assert_eq!(
            topic_config["additionalProperties"], false,
            "OrderTopicConfigV1 nested config should have additionalProperties: false"
        );
    }

    #[test]
    fn test_serde_rename_generic_field_uses_serialized_name() {
        // ContainerV1 has a generic field `rust_field_name` with #[serde(rename = "inner_data")]
        // The schema should use "inner_data" (serialized name), not "rust_field_name"

        // Verify ContainerV1 base schema uses the serialized name
        let container_schema = ContainerV1::<()>::gts_schema_with_refs();
        println!(
            "ContainerV1 schema:\n{}",
            serde_json::to_string_pretty(&container_schema).unwrap()
        );

        // The generic field should be named "inner_data" in the schema (serde rename)
        let props = container_schema
            .get("properties")
            .expect("Should have properties");
        assert!(
            props.get("inner_data").is_some(),
            "Schema should use serialized name 'inner_data', not Rust field name 'rust_field_name'. Got: {}",
            serde_json::to_string_pretty(&props).unwrap()
        );
        assert!(
            props.get("rust_field_name").is_none(),
            "Schema should NOT have Rust field name 'rust_field_name'"
        );

        // Verify ContentV1 child schema nests under "inner_data" (serialized name)
        let content_schema = ContentV1::gts_schema_with_refs();
        println!(
            "ContentV1 schema:\n{}",
            serde_json::to_string_pretty(&content_schema).unwrap()
        );

        // Child properties should be nested under "inner_data" (parent's serialized generic field name)
        let all_of = content_schema.get("allOf").expect("Should have allOf");
        let child_props = all_of[1].get("properties").expect("Should have properties");

        // Should have "inner_data" field (parent's serialized generic field name)
        let inner_data = child_props.get("inner_data");
        assert!(
            inner_data.is_some(),
            "Child schema should nest under 'inner_data' (serialized name). Got: {}",
            serde_json::to_string_pretty(&child_props).unwrap()
        );

        // Should NOT have "rust_field_name"
        assert!(
            child_props.get("rust_field_name").is_none(),
            "Child schema should NOT use Rust field name 'rust_field_name'"
        );

        // Verify the nested properties contain the child's fields
        let inner_props = inner_data
            .unwrap()
            .get("properties")
            .expect("inner_data should have properties");
        assert!(
            inner_props.get("content_value").is_some(),
            "inner_data.properties should have 'content_value'"
        );
    }

    /* ============================================================
    Issue #72: gts_type field blocks Deserialize
    ============================================================ */

    #[test]
    fn test_base_struct_without_type_or_id_field() {
        // QuotaFailureV1 has no type/id field at all — should compile and work
        let quota = QuotaFailureV1 {
            violations: vec!["exceeded".to_string()],
        };

        // Serialization should work
        let json = serde_json::to_string(&quota).unwrap();
        assert!(json.contains("exceeded"));

        // Round-trip deserialization should work
        let deserialized: QuotaFailureV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.violations, vec!["exceeded".to_string()]);

        // Schema ID should still be accessible via trait
        assert_eq!(
            QuotaFailureV1::gts_schema_id().as_ref(),
            "gts.x.core.errors.quota_failure.v1~"
        );
    }

    #[test]
    fn test_base_struct_without_gts_type_field() {
        // RateLimitErrorV1 has no gts_type field — schema ID is accessed via generated methods
        let error = RateLimitErrorV1 { retry_after: 30 };

        // Serialization works normally
        let json_value = serde_json::to_value(&error).unwrap();
        assert_eq!(json_value["retry_after"], 30);
        assert!(json_value.get("gts_type").is_none());

        // Deserialization works normally
        let json_str = r#"{"retry_after": 60}"#;
        let deserialized: RateLimitErrorV1 = serde_json::from_str(json_str).unwrap();
        assert_eq!(deserialized.retry_after, 60);

        // Schema ID is still accessible via the generated method
        assert_eq!(
            RateLimitErrorV1::gts_schema_id().as_ref(),
            "gts.x.core.errors.rate_limit.v1~"
        );
    }

    #[test]
    fn test_gts_type_in_properties_preserved() {
        // TopicV1WithGtsTypeV1 has gts_type IN properties — should be serialized normally
        let topic = TopicV1WithGtsTypeV1::<OrderTopicConfigV1> {
            gts_type: GtsSchemaId::new("gts.x.core.events.topic.v1~"),
            name: "orders".to_string(),
            description: None,
            config: OrderTopicConfigV1,
        };

        let json_value = serde_json::to_value(&topic).unwrap();
        assert!(
            json_value.get("gts_type").is_some(),
            "gts_type should be present when listed in properties"
        );
        assert_eq!(json_value["gts_type"], "gts.x.core.events.topic.v1~");
    }

    #[test]
    fn test_id_uuid_field_not_treated_as_gts_id() {
        // BaseEventV1 has `id: Uuid` which should NOT be treated as a GTS ID field.
        // It should compile and serialize normally without serde skip.
        let event = BaseEventV1::<SimplePayloadV1> {
            event_type: SimplePayloadV1::gts_schema_id().clone(),
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            sequence_id: 1,
            payload: SimplePayloadV1 {
                message: "test".to_string(),
                severity: 5,
            },
        };

        let json_value = serde_json::to_value(&event).unwrap();
        // id should be present (it's a regular Uuid field, not GtsInstanceId)
        assert!(
            json_value.get("id").is_some(),
            "id: Uuid should be serialized normally"
        );
    }
}
