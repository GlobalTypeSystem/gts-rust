//! Edge case tests for struct_to_gts_schema macro

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gts_macros::struct_to_gts_schema;
use gts::{GtsInstanceId, GtsSchemaId};

/// Test version extraction from struct names with underscore separator
#[struct_to_gts_schema(
    dir_path = "test_schemas",
    schema_id = "gts.x.test.entities.versioned.v2.1~",
    description = "Test minor version with underscore in struct name",
    properties = "value",
    base = true
)]
#[derive(Debug)]
struct VersionedStructV2_1 {
    pub gts_type: GtsSchemaId,
    pub value: String,
}

/// Test with maximum segment length
#[struct_to_gts_schema(
    dir_path = "test_schemas",
    schema_id = "gts.vendor_name.package_name.namespace_name.type_name.v1~",
    description = "Test with underscores in all segments",
    properties = "id,name",
    base = true
)]
#[derive(Debug)]
struct UnderscoreSegmentsV1 {
    pub id: GtsInstanceId,
    pub name: String,
}

/// Test unit struct (no fields, empty properties)
#[struct_to_gts_schema(
    dir_path = "test_schemas",
    schema_id = "gts.x.test.entities.empty.v1~",
    description = "Unit struct with no fields",
    properties = "",
    base = true
)]
#[derive(Debug)]
struct UnitStructV1;

/// Test child unit struct extending parent
#[struct_to_gts_schema(
    dir_path = "test_schemas",
    base = true,
    schema_id = "gts.x.test.entities.parent.v1~",
    description = "Parent with generic field",
    properties = "id,data"
)]
#[derive(Debug)]
struct ParentWithGenericV1<T> {
    pub id: GtsInstanceId,
    pub data: T,
}

#[struct_to_gts_schema(
    dir_path = "test_schemas",
    base = ParentWithGenericV1,
    schema_id = "gts.x.test.entities.parent.v1~x.test.entities.child.v1~",
    description = "Child unit struct extending parent",
    properties = ""
)]
#[derive(Debug)]
struct ChildUnitStructV1;

/// Test serde rename on type field
#[struct_to_gts_schema(
    dir_path = "test_schemas",
    base = true,
    schema_id = "gts.x.test.entities.renamed.v1~",
    description = "Test serde rename on type field",
    properties = "renamed_type,value"
)]
#[derive(Debug)]
struct RenamedTypeFieldV1 {
    #[serde(rename = "type")]
    pub renamed_type: GtsSchemaId,
    pub value: String,
}

/// Test with r#type (raw identifier)
#[struct_to_gts_schema(
    dir_path = "test_schemas",
    base = true,
    schema_id = "gts.x.test.entities.raw_type.v1~",
    description = "Test with raw type identifier",
    properties = "r#type,value"
)]
#[derive(Debug)]
struct RawTypeFieldV1 {
    pub r#type: GtsSchemaId,
    pub value: String,
}

/// Test multiple underscores in version
#[struct_to_gts_schema(
    dir_path = "test_schemas",
    base = true,
    schema_id = "gts.x.test.entities.multiversion.v10.20~",
    description = "Test multi-digit versions",
    properties = "id"
)]
#[derive(Debug)]
struct MultiVersionV10_20 {
    pub id: GtsInstanceId,
}

/// Test with Option<T> fields
#[struct_to_gts_schema(
    dir_path = "test_schemas",
    base = true,
    schema_id = "gts.x.test.entities.optional.v1~",
    description = "Test with optional fields",
    properties = "required,optional"
)]
#[derive(Debug)]
struct OptionalFieldsV1 {
    pub gts_type: GtsSchemaId,
    pub required: String,
    pub optional: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_versioned_struct_v2_1_schema_id() {
        let schema_id = VersionedStructV2_1::gts_schema_id();
        assert_eq!(schema_id.as_ref(), "gts.x.test.entities.versioned.v2.1~");
    }

    #[test]
    fn test_versioned_struct_v2_1_base_schema_id() {
        let base_id = VersionedStructV2_1::gts_base_schema_id();
        assert!(base_id.is_none());
    }

    #[test]
    fn test_underscore_segments_schema_id() {
        let schema_id = UnderscoreSegmentsV1::gts_schema_id();
        assert_eq!(
            schema_id.as_ref(),
            "gts.vendor_name.package_name.namespace_name.type_name.v1~"
        );
    }

    #[test]
    fn test_underscore_segments_make_instance_id() {
        let instance_id = UnderscoreSegmentsV1::gts_make_instance_id("test.app.instance.v1");
        assert_eq!(
            instance_id.as_ref(),
            "gts.vendor_name.package_name.namespace_name.type_name.v1~test.app.instance.v1"
        );
    }

    #[test]
    fn test_unit_struct_schema_id() {
        let schema_id = UnitStructV1::gts_schema_id();
        assert_eq!(schema_id.as_ref(), "gts.x.test.entities.empty.v1~");
    }

    #[test]
    fn test_unit_struct_serialization() {
        let instance = UnitStructV1;
        let json = serde_json::to_value(&instance).unwrap();
        // Unit structs should serialize as empty objects {}
        assert!(json.is_object());
        assert_eq!(json.as_object().unwrap().len(), 0);
    }

    #[test]
    fn test_unit_struct_deserialization() {
        // Unit structs should deserialize from empty object
        let json = serde_json::json!({});
        let _instance: UnitStructV1 = serde_json::from_value(json).unwrap();
    }

    #[test]
    fn test_unit_struct_deserialization_from_null() {
        // Unit structs should also deserialize from null for backward compatibility
        let json = serde_json::json!(null);
        let _instance: UnitStructV1 = serde_json::from_value(json).unwrap();
    }

    #[test]
    fn test_child_unit_struct_schema_id() {
        let schema_id = ChildUnitStructV1::gts_schema_id();
        assert_eq!(
            schema_id.as_ref(),
            "gts.x.test.entities.parent.v1~x.test.entities.child.v1~"
        );
    }

    #[test]
    fn test_child_unit_struct_base_schema_id() {
        let base_id = ChildUnitStructV1::gts_base_schema_id();
        assert!(base_id.is_some());
        assert_eq!(base_id.unwrap().as_ref(), "gts.x.test.entities.parent.v1~");
    }

    #[test]
    fn test_renamed_type_field_schema() {
        let schema = RenamedTypeFieldV1::gts_schema_with_refs();
        let props = schema.get("properties").unwrap().as_object().unwrap();
        // Should use the serde rename "type", not the field name "renamed_type"
        assert!(props.contains_key("type"));
        assert!(!props.contains_key("renamed_type"));
    }

    #[test]
    fn test_raw_type_field_schema() {
        let schema = RawTypeFieldV1::gts_schema_with_refs();
        let props = schema.get("properties").unwrap().as_object().unwrap();
        // Raw identifiers should be handled correctly
        assert!(props.contains_key("type"));
    }

    #[test]
    fn test_multi_version_schema_id() {
        let schema_id = MultiVersionV10_20::gts_schema_id();
        assert_eq!(
            schema_id.as_ref(),
            "gts.x.test.entities.multiversion.v10.20~"
        );
    }

    #[test]
    fn test_optional_fields_schema() {
        let schema = OptionalFieldsV1::gts_schema_with_refs();
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("required"));
        assert!(props.contains_key("optional"));
    }

    #[test]
    fn test_schema_with_refs_as_string_is_valid_json() {
        let json_str = VersionedStructV2_1::gts_schema_with_refs_as_string();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.is_object());
        assert!(parsed.get("$id").is_some());
        assert!(parsed.get("$schema").is_some());
    }

    #[test]
    fn test_schema_with_refs_as_string_pretty_formatting() {
        let pretty = VersionedStructV2_1::gts_schema_with_refs_as_string_pretty();
        let compact = VersionedStructV2_1::gts_schema_with_refs_as_string();
        // Pretty version should be longer due to whitespace
        assert!(pretty.len() > compact.len());
        assert!(pretty.contains('\n'));
    }

    #[test]
    fn test_instance_json_serialization() {
        let instance = OptionalFieldsV1 {
            gts_type: OptionalFieldsV1::gts_schema_id().clone(),
            required: "test".to_string(),
            optional: Some("value".to_string()),
        };
        let json = instance.gts_instance_json();
        assert!(json.is_object());
        assert_eq!(json["required"], "test");
        assert_eq!(json["optional"], "value");
    }

    #[test]
    fn test_instance_json_as_string() {
        let instance = OptionalFieldsV1 {
            gts_type: OptionalFieldsV1::gts_schema_id().clone(),
            required: "test".to_string(),
            optional: None,
        };
        let json_str = instance.gts_instance_json_as_string();
        assert!(json_str.contains("required"));
        assert!(json_str.contains("test"));
    }

    #[test]
    fn test_instance_json_as_string_pretty() {
        let instance = OptionalFieldsV1 {
            gts_type: OptionalFieldsV1::gts_schema_id().clone(),
            required: "test".to_string(),
            optional: None,
        };
        let pretty = instance.gts_instance_json_as_string_pretty();
        let compact = instance.gts_instance_json_as_string();
        assert!(pretty.len() > compact.len());
        assert!(pretty.contains('\n'));
    }

    #[test]
    fn test_parent_with_generic_schema_generic_field() {
        use gts::GtsSchema;
        let generic_field = <ParentWithGenericV1<()> as GtsSchema>::GENERIC_FIELD;
        assert!(generic_field.is_some());
        assert_eq!(generic_field.unwrap(), "data");
    }

    #[test]
    fn test_child_unit_struct_no_generic_field() {
        use gts::GtsSchema;
        let generic_field = <ChildUnitStructV1 as GtsSchema>::GENERIC_FIELD;
        assert!(generic_field.is_none());
    }
}