//! Test: Serde rename attribute handling for GTS Type fields

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::nonminimal_bool,
    clippy::uninlined_format_args,
    clippy::bool_assert_comparison
)]

use gts::gts::GtsTypeId;
use gts_macros::{gts_id, struct_to_gts_schema};
use uuid::Uuid;

/* ============================================================
Serde rename tests - event_type field with serde(rename = "type")
============================================================ */

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.core.events.type.v1~"),
    description = "Base event type with serde(rename = \"type\")",
    properties = "event_type,id,tenant_id,sequence_id,payload"
)]
#[derive(Debug)]
pub struct BaseEventV1SerdeRenameV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsTypeId, // This should be recognized as a valid GTS Type field
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.core.events.gts_type.v1~"),
    description = "Base event type with serde(rename = \"gts_type\")",
    properties = "event_type,id,tenant_id,sequence_id,payload"
)]
#[derive(Debug)]
pub struct BaseEventV1GtsTypeRenameV1<P> {
    #[serde(rename = "gts_type")]
    pub event_type: GtsTypeId, // This should be recognized as a valid GTS Type field
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.core.events.schema.v1~"),
    description = "Base event type with serde(rename = \"schema\")",
    properties = "event_type,id,tenant_id,sequence_id,payload"
)]
#[derive(Debug)]
pub struct BaseEventV1SchemaRenameV1<P> {
    #[serde(rename = "schema")]
    pub event_type: GtsTypeId, // This should be recognized as a valid GTS Type field
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

/* ============================================================
Tests
============================================================ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_identity_uses_serialized_field_names() {
        #[struct_to_gts_schema(
            dir_path = "schemas",
            base = true,
            type_id = gts_id!("x.test.renamed.id.v1~"),
            description = "Base with separate local and GTS IDs",
            properties = "local,id"
        )]
        struct RenamedIdV1 {
            #[serde(rename(serialize = "$id", deserialize = "instance"))]
            local: gts::GtsInstanceId,
            #[serde(rename = "local_number")]
            id: u32,
        }

        let instance = RenamedIdV1 {
            local: RenamedIdV1::gts_make_instance_id("x.test.renamed.first.v1"),
            id: 42,
        };
        let json = instance.gts_instance_json();
        assert_eq!(
            json,
            serde_json::json!({ "$id": instance.local, "local_number": 42 })
        );
        let input = serde_json::json!({"instance": instance.local, "local_number": 42});
        let decoded: RenamedIdV1 = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(decoded.gts_instance_json(), json);
        let input_schema = serde_json::to_value(schemars::schema_for!(RenamedIdV1)).unwrap();
        jsonschema::validator_for(&input_schema)
            .unwrap()
            .validate(&input)
            .unwrap();

        let schema = gts::gts_schema_for!(RenamedIdV1);
        jsonschema::validator_for(&schema)
            .unwrap()
            .validate(&json)
            .unwrap();
    }

    #[test]
    fn gts_id_precedes_local_id_with_swapped_json_names() {
        #[struct_to_gts_schema(
            dir_path = "schemas",
            base = true,
            type_id = gts_id!("x.test.renamed.swapped.v1~"),
            description = "A GTS instance with a secondary local ID",
            properties = "instance,local"
        )]
        struct SwappedV1 {
            #[serde(rename(serialize = "id", deserialize = "$id"))]
            local: Option<u32>,
            #[serde(rename(serialize = "$id", deserialize = "id"))]
            instance: gts::GtsInstanceId,
        }

        let value = SwappedV1 {
            local: Some(7),
            instance: SwappedV1::gts_make_instance_id("x.test.renamed.first.v1"),
        };
        let output = value.gts_instance_json();
        let schema = gts::gts_schema_for!(SwappedV1);
        assert_eq!(schema["required"], serde_json::json!(["$id"]));
        jsonschema::validator_for(&schema)
            .unwrap()
            .validate(&output)
            .unwrap();
        let entity = gts::GtsEntity::new(
            None,
            None,
            &output,
            Some(&gts::GtsConfig::default()),
            None,
            false,
            String::new(),
            None,
            None,
        );
        assert_eq!(entity.selected_entity_field.as_deref(), Some("$id"));
        assert_eq!(entity.instance_id.as_deref(), output["$id"].as_str());
    }

    #[test]
    fn renamed_payload_validates_against_derived_schema() {
        use gts::GtsSchema;
        use serde_json::json;

        #[struct_to_gts_schema(
            dir_path = "schemas",
            base = true,
            type_id = gts_id!("x.test.renamed.envelope.v1~"),
            description = "Envelope with a renamed payload",
            properties = "payload"
        )]
        struct EnvelopeV1<P> {
            #[serde(rename(serialize = "body", deserialize = "input"))]
            payload: P,
        }

        #[struct_to_gts_schema(
            dir_path = "schemas",
            base = EnvelopeV1,
            type_id = gts_id!("x.test.renamed.envelope.v1~x.test.renamed.details.v1~"),
            description = "Details with directional field names",
            properties = "item_count"
        )]
        #[serde(rename_all(serialize = "camelCase", deserialize = "SCREAMING_SNAKE_CASE"))]
        struct DetailsV1 {
            #[serde(default = "Self::default_count", alias = "oldCount")]
            item_count: u32,
        }

        impl DetailsV1 {
            fn default_count() -> u32 {
                7
            }
        }

        let instance: EnvelopeV1<DetailsV1> =
            serde_json::from_value(json!({"input": {"ITEM_COUNT": 2}})).unwrap();
        let json = instance.gts_instance_json();
        assert_eq!(json, json!({"body": {"itemCount": 2}}));
        let legacy: EnvelopeV1<DetailsV1> =
            serde_json::from_value(json!({"input": {"item_count": 2}})).unwrap();
        assert_eq!(legacy.gts_instance_json(), json);
        let defaulted: EnvelopeV1<DetailsV1> =
            serde_json::from_value(json!({"input": {}})).unwrap();
        assert_eq!(defaulted.payload.item_count, 7);
        let aliased: EnvelopeV1<DetailsV1> =
            serde_json::from_value(json!({"input": {"oldCount": 2}})).unwrap();
        assert_eq!(aliased.gts_instance_json(), json);
        let schema = DetailsV1::gts_resolved_schema();
        jsonschema::validator_for(&schema)
            .unwrap()
            .validate(&json)
            .unwrap();
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = gts_id!("x.test.serde.envelope.v1~"),
        description = "Envelope with derived data",
        properties = "payload"
    )]
    #[serde(rename_all = "camelCase")]
    struct WireEnvelopeV1<P> {
        payload: P,
    }

    #[test]
    fn generated_alias_does_not_claim_another_fields_output_name() {
        #[struct_to_gts_schema(
            dir_path = "schemas",
            base = WireEnvelopeV1,
            type_id = gts_id!("x.test.serde.envelope.v1~x.test.serde.names.v1~"),
            description = "A legacy name also used for output",
            properties = "b,a"
        )]
        struct NamesV1 {
            #[serde(rename = "c")]
            b: u32,
            #[serde(rename(serialize = "b", deserialize = "a_in"))]
            a: Option<u32>,
        }

        let json = WireEnvelopeV1 {
            payload: NamesV1 { b: 2, a: Some(1) },
        }
        .gts_instance_json();
        let decoded: WireEnvelopeV1<NamesV1> = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.payload.b, 2);
        assert_eq!(decoded.payload.a, None);
    }

    #[test]
    fn derived_field_requires_only_an_explicit_default() {
        #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        struct NoDefault(u32);

        #[struct_to_gts_schema(
            dir_path = "schemas",
            base = WireEnvelopeV1,
            type_id = gts_id!("x.test.serde.envelope.v1~x.test.serde.defaults.v1~"),
            description = "Defaults in a derived reader",
            properties = "hard,fallback"
        )]
        struct DefaultsV1 {
            #[serde(skip)]
            hard: NoDefault,
            #[serde(skip, default = "Self::fallback")]
            fallback: NoDefault,
        }

        impl DefaultsV1 {
            fn fallback() -> NoDefault {
                NoDefault(3)
            }
        }
        let decoded: WireEnvelopeV1<DefaultsV1> =
            serde_json::from_value(serde_json::json!({"payload":{"hard":2}})).unwrap();
        assert_eq!(decoded.payload.hard.0, 2);
        assert_eq!(decoded.payload.fallback.0, 3);
        assert!(
            serde_json::from_value::<WireEnvelopeV1<DefaultsV1>>(serde_json::json!({"payload":{}}))
                .is_err()
        );
    }

    #[test]
    fn configured_fields_drive_the_schema_and_derived_reader() {
        use gts::GtsSchema;
        use serde_json::json;

        #[struct_to_gts_schema(
            dir_path = "schemas", base = true, type_id = gts_id!("x.test.cfg.host.v1~"),
            description = "Configured envelope", properties = "payload"
        )]
        struct HostV1<P> {
            #[cfg_attr(all(), serde(rename(serialize = "body", deserialize = "input")))]
            payload: P,
        }

        #[struct_to_gts_schema(
            dir_path = "schemas", base = HostV1,
            type_id = gts_id!("x.test.cfg.host.v1~x.test.cfg.data.v1~"),
            description = "Configured data", properties = "value"
        )]
        struct DataV1 {
            #[cfg(any())]
            #[serde(rename(serialize = "unused", deserialize = "live"))]
            disabled: String,
            #[cfg_attr(all(), serde(rename(serialize = "valueOut", deserialize = "live")))]
            value: u32,
        }

        let value: HostV1<DataV1> = serde_json::from_value(json!({"input":{"live": 4}})).unwrap();
        let output = value.gts_instance_json();
        assert_eq!(output, json!({"body":{"valueOut":4}}));
        assert_eq!(<HostV1<()> as GtsSchema>::GENERIC_FIELD, Some("body"));
        jsonschema::validator_for(&DataV1::gts_resolved_schema())
            .unwrap()
            .validate(&output)
            .unwrap();
    }

    #[test]
    fn ordinary_nested_type_can_use_a_separate_wire_schema() {
        use serde_json::json;

        #[derive(serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
        struct Contact {
            #[serde(rename(serialize = "displayName", deserialize = "name"))]
            name: String,
            phone: Option<String>,
        }

        #[derive(serde::Deserialize, schemars::JsonSchema)]
        struct ContactWire {
            #[serde(rename = "displayName")]
            name: String,
            phone: Option<String>,
        }

        #[struct_to_gts_schema(
            dir_path = "schemas", base = true, type_id = gts_id!("x.test.contact.record.v1~"),
            description = "Contact record on the wire", properties = "contact"
        )]
        struct ContactRecordV1 {
            #[schemars(with = "ContactWire")]
            contact: Contact,
        }

        let input = json!({"name":"Anna"});
        let input_schema = schemars::schema_for!(Contact);
        jsonschema::validator_for(input_schema.as_value())
            .unwrap()
            .validate(&input)
            .unwrap();
        let record: ContactRecordV1 = serde_json::from_value(json!({"contact":input})).unwrap();
        let wire_schema = gts::gts_schema_for!(ContactRecordV1);
        let validator = jsonschema::validator_for(&wire_schema).unwrap();
        let output = record.gts_instance_json();
        validator.validate(&output).unwrap();
        let contact: ContactWire = serde_json::from_value(output["contact"].clone()).unwrap();
        assert_eq!(contact.name, "Anna");
        assert_eq!(contact.phone, None);
        validator
            .validate(&json!({"contact":{"displayName":"Anna"}}))
            .unwrap();
        assert!(!validator.is_valid(&json!({"contact":{"name":"Anna"}})));
    }

    #[test]
    fn base_unit_preserves_explicit_derives_and_helper_attributes() {
        #[struct_to_gts_schema(dir_path = "schemas", base = true,
            type_id = gts_id!("x.test.serde.unit.v1~"), description = "Unit", properties = "")]
        #[derive(Debug, schemars::JsonSchema)]
        #[serde(rename = "unit")]
        struct UnitV1;
        assert_eq!(format!("{UnitV1:?}"), "UnitV1");
        assert_eq!(UnitV1.gts_instance_json(), serde_json::json!({}));
    }

    #[test]
    fn test_serde_rename_type_compiles() {
        // This should compile because event_type is renamed to "type" and has correct type
        let event = BaseEventV1SerdeRenameV1::<()> {
            event_type: GtsTypeId::new(gts_id!("x.core.events.type.v1~")),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        assert_eq!(
            event.event_type.to_string(),
            gts_id!("x.core.events.type.v1~")
        );
    }

    #[test]
    fn test_serde_rename_gts_type_compiles() {
        // This should compile because event_type is renamed to "gts_type" and has correct type
        let event = BaseEventV1GtsTypeRenameV1::<()> {
            event_type: GtsTypeId::new(gts_id!("x.core.events.gts_type.v1~")),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        assert_eq!(
            event.event_type.to_string(),
            gts_id!("x.core.events.gts_type.v1~")
        );
    }

    #[test]
    fn test_serde_rename_schema_compiles() {
        // This should compile because event_type is renamed to "schema" and has correct type
        let event = BaseEventV1SchemaRenameV1::<()> {
            event_type: GtsTypeId::new(gts_id!("x.core.events.schema.v1~")),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        assert_eq!(
            event.event_type.to_string(),
            gts_id!("x.core.events.schema.v1~")
        );
    }

    #[test]
    fn test_serde_rename_schema_constants() {
        // Verify that schema constants are generated correctly
        assert_eq!(
            BaseEventV1SerdeRenameV1::<()>::gts_type_id()
                .clone()
                .into_string(),
            gts_id!("x.core.events.type.v1~")
        );
        assert_eq!(
            BaseEventV1GtsTypeRenameV1::<()>::gts_type_id()
                .clone()
                .into_string(),
            gts_id!("x.core.events.gts_type.v1~")
        );
        assert_eq!(
            BaseEventV1SchemaRenameV1::<()>::gts_type_id()
                .clone()
                .into_string(),
            gts_id!("x.core.events.schema.v1~")
        );
    }

    #[test]
    fn test_serde_rename_serialization() {
        // Test that serialization works correctly with serde rename
        let event = BaseEventV1SerdeRenameV1::<()> {
            event_type: GtsTypeId::new(gts_id!("x.core.events.type.v1~")),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // The field should be serialized as "type" (not "event_type")
        assert!(parsed.get("type").is_some());
        assert!(parsed.get("event_type").is_none());
        assert_eq!(parsed["type"], gts_id!("x.core.events.type.v1~"));
    }

    #[test]
    fn test_serde_rename_gts_type_serialization() {
        // Test that serialization works correctly with serde rename to gts_type
        let event = BaseEventV1GtsTypeRenameV1::<()> {
            event_type: GtsTypeId::new(gts_id!("x.core.events.gts_type.v1~")),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // The field should be serialized as "gts_type" (not "event_type")
        assert!(parsed.get("gts_type").is_some());
        assert!(parsed.get("event_type").is_none());
        assert_eq!(parsed["gts_type"], gts_id!("x.core.events.gts_type.v1~"));
    }

    #[test]
    fn test_serde_rename_schema_serialization() {
        // Test that serialization works correctly with serde rename to schema
        let event = BaseEventV1SchemaRenameV1::<()> {
            event_type: GtsTypeId::new(gts_id!("x.core.events.schema.v1~")),
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            sequence_id: 12345,
            payload: (),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // The field should be serialized as "schema" (not "event_type")
        assert!(parsed.get("schema").is_some());
        assert!(parsed.get("event_type").is_none());
        assert_eq!(parsed["schema"], gts_id!("x.core.events.schema.v1~"));
    }
}
