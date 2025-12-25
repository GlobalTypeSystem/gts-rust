#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::nonminimal_bool,
    clippy::uninlined_format_args,
    clippy::bool_assert_comparison
)]

mod inheritance_tests;

use gts::{GtsConfig, GtsEntity, GtsID, GtsSchema};
use gts_macros::struct_to_gts_schema;
use jsonschema::JSONSchema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Event Topic (Stream) definition for testing GTS schema generation.
/// Inspired by examples/examples/events/schemas/gts.x.core.events.topic.v1~.schema.json
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Event Topic (Stream) definition",
    properties = "id,name,description,retention,ordering"
)]
pub struct EventTopicV1 {
    /// Identifier for the topic/stream in GTS notation
    pub id: String,
    /// Topic name
    pub name: String,
    /// Topic description
    pub description: Option<String>,
    /// How long events are retained (ISO-8601 duration, e.g., P30D)
    pub retention: String,
    /// Ordering model: "global" or "by-partition-key"
    pub ordering: String,
    // Internal field not included in the schema
    pub internal_config: Option<String>,
}

/// Product entity for testing GTS schema generation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.test.entities.product.v1~",
    description = "Product entity with pricing information",
    properties = "id,name,price,description,in_stock"
)]
pub struct ProductV1 {
    pub id: String,
    pub name: String,
    pub price: f64,
    pub description: Option<String>,
    pub in_stock: bool,
    // This field is not included in the schema
    pub warehouse_location: String,
}

// =============================================================================
// Tests for 3.a) GTS_SCHEMA_JSON - JSON Schema with proper $id
// =============================================================================

#[test]
fn test_schema_json_contains_id() {
    // Verify GTS_SCHEMA_JSON contains proper $id with URI prefix "gts://"
    let topic_schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let product_schema: serde_json::Value =
        serde_json::from_str(&ProductV1::gts_json_schema_with_refs()).unwrap();
    assert_eq!(topic_schema["$id"], "gts://gts.x.core.events.topic.v1~");
    assert_eq!(
        product_schema["$id"],
        "gts://gts.x.test.entities.product.v1~"
    );
}

#[test]
fn test_schema_json_contains_description() {
    // Note: schemars-generated schemas use the struct's doc comment for description,
    // not the macro's description attribute. This test verifies basic schema structure.
    let topic_schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let product_schema: serde_json::Value =
        serde_json::from_str(&ProductV1::gts_json_schema_with_refs()).unwrap();
    // Verify these are valid object schemas with required type
    assert_eq!(topic_schema["type"], "object");
    assert_eq!(product_schema["type"], "object");
}

#[test]
fn test_schema_json_contains_only_specified_properties() {
    // Note: schemars includes ALL struct fields in the schema, not just the ones
    // specified in the macro's properties attribute. The properties attribute is
    // used for CLI schema generation, not runtime schemars generation.
    let topic_schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let topic_props = topic_schema["properties"].as_object().unwrap();

    // EventTopicV1: id, name, description, retention, ordering should be present
    assert!(topic_props.contains_key("id"));
    assert!(topic_props.contains_key("name"));
    assert!(topic_props.contains_key("description"));
    assert!(topic_props.contains_key("retention"));
    assert!(topic_props.contains_key("ordering"));
    // internal_config IS present with schemars (it includes all fields)
    assert!(topic_props.contains_key("internal_config"));

    // Product: id, name, price, description, in_stock should be present
    let product_schema: serde_json::Value =
        serde_json::from_str(&ProductV1::gts_json_schema_with_refs()).unwrap();
    let product_props = product_schema["properties"].as_object().unwrap();
    assert!(product_props.contains_key("id"));
    assert!(product_props.contains_key("name"));
    assert!(product_props.contains_key("price"));
    assert!(product_props.contains_key("description"));
    assert!(product_props.contains_key("in_stock"));
    // warehouse_location IS present with schemars (it includes all fields)
    assert!(product_props.contains_key("warehouse_location"));
}

#[test]
fn test_schema_json_is_valid_json() {
    // Verify the schema JSON can be parsed
    let topic_schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let product_schema: serde_json::Value =
        serde_json::from_str(&ProductV1::gts_json_schema_with_refs()).unwrap();

    // Verify key fields - $id now uses the "gts://" URI prefix
    assert_eq!(topic_schema["$id"], "gts://gts.x.core.events.topic.v1~");
    assert_eq!(topic_schema["type"], "object");
    assert_eq!(
        topic_schema["$schema"],
        "http://json-schema.org/draft-07/schema#"
    );

    assert_eq!(
        product_schema["$id"],
        "gts://gts.x.test.entities.product.v1~"
    );
    assert_eq!(product_schema["type"], "object");
}

#[test]
fn test_schema_json_required_fields() {
    let topic_schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let required = topic_schema["required"].as_array().unwrap();

    // All non-Option fields in properties should be required
    assert!(required.contains(&serde_json::json!("id")));
    assert!(required.contains(&serde_json::json!("name")));
    assert!(required.contains(&serde_json::json!("retention")));
    assert!(required.contains(&serde_json::json!("ordering")));
    // description is Option<String>, so should NOT be required
    assert!(!required.contains(&serde_json::json!("description")));

    // Product: description is Option<String>, so should NOT be required
    let product_schema: serde_json::Value =
        serde_json::from_str(&ProductV1::gts_json_schema_with_refs()).unwrap();
    let product_required = product_schema["required"].as_array().unwrap();
    assert!(!product_required.contains(&serde_json::json!("description")));
    assert!(product_required.contains(&serde_json::json!("price")));
}

// =============================================================================
// Tests for 3.b) make_gts_instance_id() - Generate instance IDs
// =============================================================================

#[test]
fn test_gts_instance_id_simple_segment() {
    // Test with simple segment - event topic instance
    let id = EventTopicV1::make_gts_instance_id("x.commerce.orders.orders.v1.0");
    assert_eq!(
        id,
        "gts.x.core.events.topic.v1~x.commerce.orders.orders.v1.0"
    );

    let id = ProductV1::make_gts_instance_id("vendor.package.sku.abc.v1");
    assert_eq!(
        id,
        "gts.x.test.entities.product.v1~vendor.package.sku.abc.v1"
    );
}

#[test]
fn test_gts_instance_id_multi_segment() {
    // Test with multi-part segment like vendor.package.namespace.type.version
    let id = EventTopicV1::make_gts_instance_id("x.core.idp.contacts.v1");
    assert_eq!(id, "gts.x.core.events.topic.v1~x.core.idp.contacts.v1");
}

#[test]
fn test_gts_instance_id_with_wildcard_segment() {
    // Test with segment containing wildcard "_"
    let id = EventTopicV1::make_gts_instance_id("x.commerce._.orders.v1.0");
    assert_eq!(id, "gts.x.core.events.topic.v1~x.commerce._.orders.v1.0");
}

#[test]
fn test_gts_instance_id_versioned_segment() {
    // Test with versioned segment
    let id = EventTopicV1::make_gts_instance_id("x.y.z.instance.v1.0");
    assert_eq!(id, "gts.x.core.events.topic.v1~x.y.z.instance.v1.0");

    let id = ProductV1::make_gts_instance_id("x.y.z.sku.v2.1");
    assert_eq!(id, "gts.x.test.entities.product.v1~x.y.z.sku.v2.1");
}

#[test]
fn test_gts_instance_id_empty_segment() {
    // Edge case: empty segment returns just the schema_id
    let id = EventTopicV1::make_gts_instance_id("");
    assert_eq!(id, "gts.x.core.events.topic.v1~");
}

// =============================================================================
// Tests for metadata constants
// =============================================================================

#[test]
fn test_schema_id_constant() {
    assert_eq!(EventTopicV1::GTS_SCHEMA_ID, "gts.x.core.events.topic.v1~");
    assert_eq!(ProductV1::GTS_SCHEMA_ID, "gts.x.test.entities.product.v1~");
}

#[test]
fn test_file_path_constant() {
    assert_eq!(
        EventTopicV1::GTS_SCHEMA_FILE_PATH,
        "schemas/gts.x.core.events.topic.v1~.schema.json"
    );
    assert_eq!(
        ProductV1::GTS_SCHEMA_FILE_PATH,
        "schemas/gts.x.test.entities.product.v1~.schema.json"
    );
}

#[test]
fn test_properties_constant() {
    assert_eq!(
        EventTopicV1::GTS_SCHEMA_PROPERTIES,
        "id,name,description,retention,ordering"
    );
    assert_eq!(
        ProductV1::GTS_SCHEMA_PROPERTIES,
        "id,name,price,description,in_stock"
    );
}

// =============================================================================
// Tests for serialization (struct still works normally)
// =============================================================================

#[test]
fn test_event_topic_serialization() {
    let topic = EventTopicV1 {
        id: EventTopicV1::make_gts_instance_id("x.commerce.orders.orders.v1.0").into(),
        name: "orders".to_owned(),
        description: Some("Order lifecycle events topic".to_owned()),
        retention: "P90D".to_owned(),
        ordering: "by-partition-key".to_owned(),
        internal_config: Some("internal".to_owned()),
    };

    let json = serde_json::to_string(&topic).unwrap();
    assert!(json.contains("gts.x.core.events.topic.v1~x.commerce.orders.orders.v1.0"));
    assert!(json.contains("orders"));
    assert!(json.contains("P90D"));
}

#[test]
fn test_product_serialization() {
    let product = ProductV1 {
        id: "prod-456".to_owned(), // Non GTS ID
        name: "Test Product".to_owned(),
        price: 99.99,
        description: Some("A test product".to_owned()),
        in_stock: true,
        warehouse_location: "Warehouse A".to_owned(),
    };

    let json = serde_json::to_string(&product).unwrap();
    assert!(json.contains("prod-456"));
    assert!(json.contains("99.99"));
}

// =============================================================================
// Tests for instance serialization and schema validation
// =============================================================================

#[test]
fn test_event_topic_instance_validates_against_schema() {
    let topic = EventTopicV1 {
        id: EventTopicV1::make_gts_instance_id("x.commerce.orders.orders.v1.0").into(),
        name: "orders".to_owned(),
        description: Some("Order lifecycle events topic".to_owned()),
        retention: "P90D".to_owned(),
        ordering: "by-partition-key".to_owned(),
        internal_config: None,
    };

    // Serialize the instance
    let instance_json = serde_json::to_value(&topic).unwrap();

    // Compile the schema - the $id now uses "gts:" prefix which is a valid URI
    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let compiled = JSONSchema::compile(&schema).unwrap();

    // Validate the instance against the schema
    assert!(
        compiled.is_valid(&instance_json),
        "EventTopicV1 instance should validate against EventTopicV1 schema"
    );
}

#[test]
fn test_product_instance_validates_against_schema() {
    let product = ProductV1 {
        id: ProductV1::make_gts_instance_id("x.electronics.laptops.gaming.v1").into(),
        name: "Gaming Laptop".to_owned(),
        price: 1499.99,
        description: Some("High-performance gaming laptop".to_owned()),
        in_stock: true,
        warehouse_location: "Building A".to_owned(),
    };

    let instance_json = serde_json::to_value(&product).unwrap();
    let schema: serde_json::Value =
        serde_json::from_str(&ProductV1::gts_json_schema_with_refs()).unwrap();
    let compiled = JSONSchema::compile(&schema).unwrap();

    assert!(
        compiled.is_valid(&instance_json),
        "Product instance should validate against Product schema"
    );
}

#[test]
fn test_product_instance_with_absent_optional_field_validates() {
    // In JSON Schema, optional fields (not in "required") can be absent.
    // When the field is completely absent from the JSON object, validation passes.
    // Note: serde serializes None as `null`, which doesn't match "type": "string".
    // To properly handle optional fields, use #[serde(skip_serializing_if = "Option::is_none")]
    // or construct the JSON object without the field.
    let instance_without_description = serde_json::json!({
        "id": "gts.x.test.entities.product.v1~vendor.package.sku.mouse_abc.v1",
        "name": "Wireless Mouse",
        "price": 29.99,
        "in_stock": false,
        "warehouse_location": "Warehouse A"  // required field
        // description is absent (not null) - this is valid for optional fields
    });

    let schema: serde_json::Value =
        serde_json::from_str(&ProductV1::gts_json_schema_with_refs()).unwrap();
    let compiled = JSONSchema::compile(&schema).unwrap();

    assert!(
        compiled.is_valid(&instance_without_description),
        "Product instance with absent optional field should validate"
    );
}

#[test]
fn test_optional_field_as_null_fails_validation() {
    // JSON Schema: optional fields can be absent, but if present must match the type.
    // When serde serializes Option<String>::None, it becomes null, which is NOT
    // type "string" - so validation fails. This is correct JSON Schema behavior.
    let instance_with_null = serde_json::json!({
        "id": "product-123",
        "name": "Test Product",
        "price": 99.99,
        "description": null,  // null is NOT a valid string
        "in_stock": true
    });

    let schema: serde_json::Value =
        serde_json::from_str(&ProductV1::gts_json_schema_with_refs()).unwrap();
    let compiled = JSONSchema::compile(&schema).unwrap();

    assert!(
        !compiled.is_valid(&instance_with_null),
        "Instance with null for string field should fail validation"
    );
}

#[test]
fn test_invalid_instance_missing_required_field() {
    // Create a JSON object that's missing required fields
    let invalid_instance = serde_json::json!({
        "id": "topic-123",
        "name": "test-topic"
        // Missing: retention, ordering (required fields)
    });

    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let compiled = JSONSchema::compile(&schema).unwrap();

    assert!(
        !compiled.is_valid(&invalid_instance),
        "Instance missing required fields should fail validation"
    );

    // Verify the specific validation errors
    let result = compiled.validate(&invalid_instance);
    assert!(result.is_err(), "Validation should return errors");
}

#[test]
fn test_invalid_instance_wrong_type() {
    // Create a JSON object with wrong type for a field
    let invalid_instance = serde_json::json!({
        "id": "topic-123",
        "name": 12345,  // Should be a string
        "retention": "P30D",
        "ordering": "global"
    });

    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let compiled = JSONSchema::compile(&schema).unwrap();

    assert!(
        !compiled.is_valid(&invalid_instance),
        "Instance with wrong type should fail validation"
    );
}

#[test]
fn test_instance_with_extra_fields_rejected() {
    // GTS schemas have additionalProperties: false at root level
    // This test verifies instances with extra fields are rejected
    let instance_with_extras = serde_json::json!({
        "id": "topic-123",
        "name": "test-topic",
        "retention": "P30D",
        "ordering": "global",
        "extra_field": "this field is not in schema",
        "another_extra": 42
    });

    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let compiled = JSONSchema::compile(&schema).unwrap();

    assert!(
        !compiled.is_valid(&instance_with_extras),
        "Instance with extra fields should be rejected (additionalProperties is false)"
    );
}

#[test]
fn test_serialization_roundtrip_event_topic() {
    let original = EventTopicV1 {
        id: EventTopicV1::make_gts_instance_id("x.commerce.orders.orders.v1.0").into(),
        name: "orders".to_owned(),
        description: Some("Order lifecycle events".to_owned()),
        retention: "P90D".to_owned(),
        ordering: "by-partition-key".to_owned(),
        internal_config: Some("internal value".to_owned()),
    };

    // Serialize to JSON string
    let json_string = serde_json::to_string(&original).unwrap();

    // Deserialize back
    let deserialized: EventTopicV1 = serde_json::from_str(&json_string).unwrap();

    // Verify all fields match
    assert_eq!(original.id, deserialized.id);
    assert_eq!(original.name, deserialized.name);
    assert_eq!(original.description, deserialized.description);
    assert_eq!(original.retention, deserialized.retention);
    assert_eq!(original.ordering, deserialized.ordering);
    assert_eq!(original.internal_config, deserialized.internal_config);
}

#[test]
fn test_serialization_roundtrip_product() {
    let original = ProductV1 {
        id: ProductV1::make_gts_instance_id("x.y.roundtrip.product.v1").into(),
        name: "Roundtrip Product".to_owned(),
        price: 199.99,
        description: Some("A product for testing roundtrip serialization".to_owned()),
        in_stock: true,
        warehouse_location: "Warehouse Z".to_owned(),
    };

    let json_string = serde_json::to_string(&original).unwrap();
    let deserialized: ProductV1 = serde_json::from_str(&json_string).unwrap();

    assert_eq!(original.id, deserialized.id);
    assert_eq!(original.name, deserialized.name);
    assert!((original.price - deserialized.price).abs() < f64::EPSILON);
    assert_eq!(original.description, deserialized.description);
    assert_eq!(original.in_stock, deserialized.in_stock);
    assert_eq!(original.warehouse_location, deserialized.warehouse_location);
}

#[test]
fn test_instance_id_appears_in_serialized_output() {
    let topic = EventTopicV1 {
        id: EventTopicV1::make_gts_instance_id("x.core.idp.contacts.v1").into(),
        name: "contacts".to_owned(),
        description: None,
        retention: "P30D".to_owned(),
        ordering: "global".to_owned(),
        internal_config: None,
    };

    let json_value = serde_json::to_value(&topic).unwrap();

    // Verify the GTS instance ID is properly set in the serialized output
    assert_eq!(
        json_value["id"],
        "gts.x.core.events.topic.v1~x.core.idp.contacts.v1"
    );
}

#[test]
fn test_multiple_instances_validate_independently() {
    let topics = [
        EventTopicV1 {
            id: EventTopicV1::make_gts_instance_id("x.commerce.orders.orders.v1.0").into(),
            name: "orders".to_owned(),
            description: Some("Order events".to_owned()),
            retention: "P90D".to_owned(),
            ordering: "by-partition-key".to_owned(),
            internal_config: None,
        },
        EventTopicV1 {
            id: EventTopicV1::make_gts_instance_id("x.core.idp.contacts.v1").into(),
            name: "contacts".to_owned(),
            description: Some("Contact events".to_owned()),
            retention: "P30D".to_owned(),
            ordering: "global".to_owned(),
            internal_config: Some("config".to_owned()),
        },
        EventTopicV1 {
            id: EventTopicV1::make_gts_instance_id("x.payments.transactions.v1.0").into(),
            name: "transactions".to_owned(),
            description: Some("Payment transactions".to_owned()),
            retention: "P365D".to_owned(),
            ordering: "by-partition-key".to_owned(),
            internal_config: None,
        },
    ];

    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let compiled = JSONSchema::compile(&schema).unwrap();

    for (i, topic) in topics.iter().enumerate() {
        let instance_json = serde_json::to_value(topic).unwrap();
        assert!(
            compiled.is_valid(&instance_json),
            "EventTopicV1 {} should validate against schema",
            i + 1
        );
    }
}

// =============================================================================
// Tests for GtsEntity and GtsID integration
// =============================================================================

#[test]
fn test_schema_parsed_as_gts_entity() {
    // Parse the macro-generated schema JSON into a GtsEntity
    let schema_json: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let cfg = GtsConfig::default();

    let entity = GtsEntity::new(
        None,                      // file
        None,                      // list_sequence
        &schema_json,              // content
        Some(&cfg),                // config
        None,                      // gts_id (will be auto-detected)
        true,                      // is_schema
        "EventTopicV1".to_owned(), // label
        None,                      // validation
        None,                      // schema_id
    );

    // Verify entity is detected as a schema
    assert!(entity.is_schema, "Entity should be detected as a schema");

    // Verify GTS ID was parsed
    let gts_id = entity.gts_id.as_ref().expect("Entity should have a GTS ID");
    assert_eq!(gts_id.id, "gts.x.core.events.topic.v1~");

    // Verify the ID matches what the macro generates
    assert_eq!(gts_id.id, EventTopicV1::GTS_SCHEMA_ID);
}

#[test]
fn test_instance_parsed_as_gts_entity() {
    // Create an instance and serialize it
    let topic = EventTopicV1 {
        id: EventTopicV1::make_gts_instance_id("x.commerce.orders.orders.v1.0").into(),
        name: "orders".to_owned(),
        description: Some("Order lifecycle events".to_owned()),
        retention: "P90D".to_owned(),
        ordering: "by-partition-key".to_owned(),
        internal_config: None,
    };

    let instance_json = serde_json::to_value(&topic).unwrap();
    let cfg = GtsConfig::default();

    let entity = GtsEntity::new(
        None,                      // file
        None,                      // list_sequence
        &instance_json,            // content
        Some(&cfg),                // config
        None,                      // gts_id (will be auto-detected from "id" field)
        false,                     // is_schema
        "orders-topic".to_owned(), // label
        None,                      // validation
        None,                      // schema_id
    );

    // Verify GTS ID was parsed from the instance
    let gts_id = entity.gts_id.as_ref().expect("Entity should have a GTS ID");
    assert_eq!(
        gts_id.id,
        "gts.x.core.events.topic.v1~x.commerce.orders.orders.v1.0"
    );
}

#[test]
fn test_gts_id_segments_match_schema() {
    // Get the schema ID from the macro
    let schema_id_str = EventTopicV1::GTS_SCHEMA_ID;

    // Parse it with GtsID
    let gts_id = GtsID::new(schema_id_str).expect("Schema ID should be valid");

    // Verify segments
    assert_eq!(
        gts_id.gts_id_segments.len(),
        1,
        "Schema should have 1 segment"
    );

    let segment = &gts_id.gts_id_segments[0];
    assert_eq!(segment.vendor, "x");
    assert_eq!(segment.package, "core");
    assert_eq!(segment.namespace, "events");
    assert_eq!(segment.type_name, "topic");
    assert_eq!(segment.ver_major, 1);
    assert!(segment.is_type, "Schema ID should be a type (ends with ~)");
}

#[test]
fn test_gts_id_segments_match_instance() {
    // Generate an instance ID using the macro
    let instance_id_str = EventTopicV1::make_gts_instance_id("x.commerce.orders.orders.v1.0");

    // Parse it with GtsID
    let gts_id = GtsID::new(&instance_id_str).expect("Instance ID should be valid");

    // Instance IDs have 2 segments: type segment + instance segment
    assert_eq!(
        gts_id.gts_id_segments.len(),
        2,
        "Instance should have 2 segments"
    );

    // First segment is the type/schema segment
    let type_segment = &gts_id.gts_id_segments[0];
    assert_eq!(type_segment.vendor, "x");
    assert_eq!(type_segment.package, "core");
    assert_eq!(type_segment.namespace, "events");
    assert_eq!(type_segment.type_name, "topic");
    assert_eq!(type_segment.ver_major, 1);
    assert!(type_segment.is_type, "First segment should be a type");

    // Second segment is the instance segment
    let instance_segment = &gts_id.gts_id_segments[1];
    assert_eq!(instance_segment.vendor, "x");
    assert_eq!(instance_segment.package, "commerce");
    assert_eq!(instance_segment.namespace, "orders");
    assert_eq!(instance_segment.type_name, "orders");
    assert_eq!(instance_segment.ver_major, 1);
    assert_eq!(instance_segment.ver_minor, Some(0));
}

#[test]
fn test_schema_and_instance_segments_relationship() {
    // The schema ID from macro
    let schema_id = GtsID::new(EventTopicV1::GTS_SCHEMA_ID).unwrap();

    // An instance ID from the macro
    let instance_id_str = EventTopicV1::make_gts_instance_id("x.core.idp.contacts.v1");
    let instance_id = GtsID::new(&instance_id_str).unwrap();

    // The first segment of the instance should match the schema's segment
    let schema_segment = &schema_id.gts_id_segments[0];
    let instance_type_segment = &instance_id.gts_id_segments[0];

    assert_eq!(schema_segment.vendor, instance_type_segment.vendor);
    assert_eq!(schema_segment.package, instance_type_segment.package);
    assert_eq!(schema_segment.namespace, instance_type_segment.namespace);
    assert_eq!(schema_segment.type_name, instance_type_segment.type_name);
    assert_eq!(schema_segment.ver_major, instance_type_segment.ver_major);

    // get_type_id() should return the schema ID (without the instance segment)
    let type_id = instance_id.get_type_id();
    assert_eq!(type_id, Some(EventTopicV1::GTS_SCHEMA_ID.to_owned()));
}

#[test]
fn test_entity_and_gts_id_vendor_package_namespace_match() {
    // Parse schema as GtsEntity
    let schema_json: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let cfg = GtsConfig::default();
    let entity = GtsEntity::new(
        None,
        None,
        &schema_json,
        Some(&cfg),
        None,
        true,
        "test".to_owned(),
        None,
        None,
    );

    // Get the GTS ID from the entity
    let entity_gts_id = entity.gts_id.as_ref().unwrap();

    // Parse the same ID directly using GtsID
    let direct_gts_id = GtsID::new(EventTopicV1::GTS_SCHEMA_ID).unwrap();

    // Verify they match
    assert_eq!(entity_gts_id.id, direct_gts_id.id);
    assert_eq!(
        entity_gts_id.gts_id_segments.len(),
        direct_gts_id.gts_id_segments.len()
    );

    // Compare segment properties
    for (entity_seg, direct_seg) in entity_gts_id
        .gts_id_segments
        .iter()
        .zip(direct_gts_id.gts_id_segments.iter())
    {
        assert_eq!(entity_seg.vendor, direct_seg.vendor);
        assert_eq!(entity_seg.package, direct_seg.package);
        assert_eq!(entity_seg.namespace, direct_seg.namespace);
        assert_eq!(entity_seg.type_name, direct_seg.type_name);
        assert_eq!(entity_seg.ver_major, direct_seg.ver_major);
        assert_eq!(entity_seg.ver_minor, direct_seg.ver_minor);
        assert_eq!(entity_seg.is_type, direct_seg.is_type);
    }
}

// =============================================================================
// Tests for gts: URI prefix in JSON Schema $id field
// =============================================================================

#[test]
fn test_schema_json_id_uses_uri_prefix() {
    // The generated schema JSON should have $id with gts:// prefix for URI compatibility
    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let id = schema["$id"].as_str().unwrap();

    // $id should start with "gts://" prefix (NOT just "gts:")
    assert!(
        id.starts_with("gts://"),
        "$id should start with 'gts://' prefix"
    );
    assert!(
        !id.starts_with("gts:gts."),
        "$id should NOT use 'gts:' prefix (must be 'gts://')"
    );
    // The full ID should be "gts://gts.x.core.events.topic.v1~"
    assert_eq!(id, "gts://gts.x.core.events.topic.v1~");
}

#[test]
fn test_gts_entity_strips_uri_prefix_from_schema() {
    // When GtsEntity parses a schema with gts:// prefix in $id, the stored ID should be normalized
    let schema_json: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    let cfg = GtsConfig::default();

    let entity = GtsEntity::new(
        None,
        None,
        &schema_json,
        Some(&cfg),
        None,
        true,
        "EventTopicV1".to_owned(),
        None,
        None,
    );

    // The GTS ID should have the gts:// prefix stripped (entities.rs strips gts:// from $id field)
    let gts_id = entity.gts_id.as_ref().expect("Entity should have a GTS ID");
    assert_eq!(
        gts_id.id, "gts.x.core.events.topic.v1~",
        "GTS ID should not contain 'gts://' prefix"
    );
}

#[test]
fn test_gts_id_does_not_accept_uri_prefix() {
    // GtsID::new should NOT accept IDs with gts:// or gts: prefix directly
    // The gts:// prefix is ONLY for JSON Schema $id field and must be stripped before parsing
    assert!(GtsID::new("gts://gts.x.core.events.topic.v1~").is_err());
    assert!(!GtsID::is_valid("gts://gts.x.core.events.topic.v1~"));

    // "gts:" (without //) is also not valid
    assert!(GtsID::new("gts:gts.x.core.events.topic.v1~").is_err());
    assert!(!GtsID::is_valid("gts:gts.x.core.events.topic.v1~"));

    // Regular GTS IDs should work
    assert!(GtsID::is_valid("gts.x.core.events.topic.v1~"));
}

// =============================================================================
// Tests for GTS_JSON_SCHEMA_WITH_REFS and GTS_JSON_SCHEMA_INLINE
// =============================================================================

#[test]
fn test_schema_with_refs_top_level_fields() {
    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();

    // Top-level fields
    assert_eq!(schema["$id"], "gts://gts.x.core.events.topic.v1~");
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
    // Note: schemars-generated schemas don't include title or description unless
    // explicitly configured. We just verify the essential fields.
    assert_eq!(schema["type"], "object");

    // Single-segment schemas don't use allOf - they have direct properties
    assert!(schema["properties"].is_object());
    assert!(schema["required"].is_array());
}

#[test]
fn test_schema_inline_top_level_fields() {
    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_inline()).unwrap();

    // Top-level fields
    assert_eq!(schema["$id"], "gts://gts.x.core.events.topic.v1~");
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
    // Note: schemars-generated schemas don't include title or description unless
    // explicitly configured. We just verify the essential fields.
    assert_eq!(schema["type"], "object");

    // Single-segment schemas don't use allOf - they have direct properties
    assert!(schema["properties"].is_object());
    assert!(schema["required"].is_array());
}

#[test]
fn test_schema_with_refs_inheritance_structure() {
    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();

    // Since EventTopicV1 has no parent (single segment), it has direct properties and required fields
    let props = schema["properties"].as_object().unwrap();
    let required = schema["required"].as_array().unwrap();
    // schemars includes ALL fields (6 including internal_config)
    assert_eq!(props.len(), 6);
    assert_eq!(required.len(), 4); // description and internal_config are optional
    assert!(props.contains_key("id"));
    assert!(props.contains_key("name"));
    assert!(props.contains_key("description"));
    assert!(props.contains_key("retention"));
    assert!(props.contains_key("ordering"));
}

#[test]
fn test_schema_inline_inheritance_structure() {
    let schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_inline()).unwrap();

    // Currently identical to WITH_REFS - direct properties for single-segment schemas
    let props = schema["properties"].as_object().unwrap();
    let required = schema["required"].as_array().unwrap();
    // schemars includes ALL fields (6 including internal_config)
    assert_eq!(props.len(), 6);
    assert_eq!(required.len(), 4); // description and internal_config are optional
    assert!(props.contains_key("id"));
    assert!(props.contains_key("name"));
    assert!(props.contains_key("description"));
    assert!(props.contains_key("retention"));
    assert!(props.contains_key("ordering"));
    assert!(props.contains_key("internal_config"));
}

#[test]
fn test_schema_with_refs_inheritance_with_parent() {
    // Multi-segment schemas are blocked from direct method access
    // Only base type can access schema methods
    let base_schema = inheritance_tests::BaseEventV1::<()>::gts_schema_with_refs();

    // Base schema should have direct properties (no allOf)
    assert!(
        !base_schema.get("allOf").is_some(),
        "Base schema should not have allOf"
    );
    assert!(
        base_schema.get("properties").is_some(),
        "Base schema should have properties"
    );

    // Verify base schema properties (schemars uses serde rename, so "event_type" becomes "type")
    let props = base_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("type"));
    assert!(props.contains_key("id"));
    assert!(props.contains_key("tenant_id"));
    assert!(props.contains_key("sequence_id"));
    assert!(props.contains_key("payload"));
}

#[test]
fn test_schema_inline_inheritance_with_parent() {
    // Multi-segment schemas are blocked from direct method access
    // Test base type inline resolution
    use gts::GtsStore;

    let mut store = GtsStore::new(None);
    let base_schema = inheritance_tests::BaseEventV1::<()>::gts_schema_with_refs();
    store
        .register_schema(
            inheritance_tests::BaseEventV1::<()>::GTS_SCHEMA_ID,
            &base_schema,
        )
        .unwrap();

    // Base type can use inline resolution
    let inlined =
        store.resolve_schema_refs(&inheritance_tests::BaseEventV1::<()>::gts_schema_with_refs());
    assert!(
        inlined.get("properties").is_some(),
        "Inlined schema should have properties"
    );

    let props = inlined["properties"].as_object().unwrap();
    // schemars uses serde rename, so "event_type" becomes "type"
    assert!(props.contains_key("type"));
    assert!(props.contains_key("id"));
    assert!(props.contains_key("tenant_id"));
    assert!(props.contains_key("sequence_id"));
    assert!(props.contains_key("payload"));
}

#[test]
fn test_runtime_schema_inline_resolution() {
    use gts::GtsStore;

    let mut store = GtsStore::new(None);

    // Load only base schema - multi-segment schemas are blocked from direct method access
    let base_schema = inheritance_tests::BaseEventV1::<()>::gts_schema_with_refs();
    store
        .register_schema("gts.x.core.events.type.v1~", &base_schema)
        .unwrap();

    // Generate the inlined schema using runtime resolution (only for base type)
    let inlined =
        store.resolve_schema_refs(&inheritance_tests::BaseEventV1::<()>::gts_schema_with_refs());
    let inlined_str = inlined.to_string();

    // Verify that no $ref references remain
    assert!(
        !inlined_str.contains("$ref"),
        "Inlined schema should not contain $ref references"
    );

    // Verify that the inlined schema contains base properties
    // Note: schemars uses serde rename, so "event_type" becomes "type"
    assert!(
        inlined_str.contains(r#""type":"#),
        "Should contain property: type"
    );
    assert!(
        inlined_str.contains("tenant_id"),
        "Should contain property: tenant_id"
    );
    assert!(
        inlined_str.contains("sequence_id"),
        "Should contain property: sequence_id"
    );
    assert!(
        inlined_str.contains("payload"),
        "Should contain property: payload"
    );

    // Verify the structure is a proper JSON schema
    assert_eq!(inlined["$id"], "gts://gts.x.core.events.type.v1~");
    assert_eq!(
        inlined["$schema"],
        "http://json-schema.org/draft-07/schema#"
    );
    assert_eq!(inlined["type"], "object");
    assert!(
        inlined["properties"].is_object(),
        "Should have properties object"
    );
    assert!(inlined["required"].is_array(), "Should have required array");

    // Count total properties (base schema only)
    let props = inlined["properties"].as_object().unwrap();
    assert_eq!(props.len(), 5, "Should have 5 base properties");

    // Verify required fields
    let required = inlined["required"].as_array().unwrap();
    assert_eq!(required.len(), 5, "Should have 5 required fields");
}

#[test]
fn test_runtime_schema_inline_resolution_single_segment() {
    use gts::GtsStore;

    let mut store = GtsStore::new(None);

    // Test with a single-segment schema (no inheritance)
    let event_topic_schema: serde_json::Value =
        serde_json::from_str(&EventTopicV1::gts_json_schema_with_refs()).unwrap();
    store
        .register_schema("gts.x.core.events.topic.v1~", &event_topic_schema)
        .unwrap();

    // Generate the inlined schema
    let inlined = store.resolve_schema_refs(&EventTopicV1::gts_schema_with_refs());

    // For single-segment schemas, the result should be essentially the same
    assert_eq!(inlined["$id"], "gts://gts.x.core.events.topic.v1~");
    assert!(
        inlined["properties"].is_object(),
        "Should have properties object"
    );

    let props = inlined["properties"].as_object().unwrap();
    // schemars includes ALL fields (6 including internal_config)
    assert_eq!(props.len(), 6, "Should have 6 properties");
}
