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
    properties = "name,description"
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
The macro automatically generates:
- GTS_SCHEMA_JSON constants with proper allOf inheritance
- GTS_SCHEMA_ID constants
- gts_make_instance_id() methods

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
        assert!(PlaceOrderDataV1::gts_schema_id().clone().into_string() == "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~");

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
        // Create an instance to test against schema
        let event = BaseEventV1 {
            event_type: gts::gts::GtsSchemaId::new("test.event"),
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
        // This test shows what the CORRECT behavior should be

        // Create an actual instance to see the real structure
        let event = BaseEventV1 {
            event_type: gts::gts::GtsSchemaId::new("test.event"),
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
        let json = serde_json::to_value(&topic).unwrap();
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
        if let Some(config_obj) = instance_json.get_mut("config") {
            if *config_obj == serde_json::Value::Null {
                *config_obj = serde_json::json!({});
            }
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
        if let Some(config_obj) = topic_json.get_mut("config") {
            if *config_obj == serde_json::Value::Null {
                *config_obj = serde_json::json!({});
            }
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
    }

    #[test]
    fn test_base_struct_with_gts_id_field_compiles() {
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
    }

    #[test]
    fn test_base_struct_with_gts_id_camel_field_compiles() {
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
    }

    #[test]
    fn test_base_struct_with_gts_type_field_compiles() {
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
    }

    #[test]
    fn test_base_struct_with_gts_type_camel_field_compiles() {
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
}
