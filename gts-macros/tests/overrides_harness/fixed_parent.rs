// gts:ignore - fixture crate built by overrides_harness_tests.rs, not by this package; holds invalid overrides on purpose.
//! Override-only children of fixed (slotless) parents, written with bare
//! names whether the parent is macro-generated or hand-written (issue #156).
//! Invalid fixtures must fail their generated tests in the outer harness.

#![allow(dead_code)]

use gts::GtsSchema;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_fixed.account.v1~",
    description = "Fixed-shape account",
    properties = "id,name,kind"
)]
pub struct AccountV1 {
    pub id: gts::GtsInstanceId,
    pub name: String,
    pub kind: String,
}

/// Two override-only levels; re-narrows the grandparent's field.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = ShortV1,
    type_id = "gts.x.test.ovr_fixed.account.v1~x.test.ovr_fixed.short.v1~x.test.ovr_fixed.shorter.v1~",
    description = "Shorter account name",
    properties = "",
    overrides(AccountV1 = { #[schemars(length(max = 4))] name: String })
)]
pub struct ShorterV1;

/// Hand-written fixed `GtsSchema`: no macro-emitted constant to resolve.
pub struct HandAccountV1;

impl GtsSchema for HandAccountV1 {
    const TYPE_ID: &'static str = "gts.x.test.ovr_fixed.hand.v1~";
    const GENERIC_FIELD: Option<&'static str> = None;

    fn gts_schema_with_refs() -> serde_json::Value {
        serde_json::json!({
            "$id": "gts://gts.x.test.ovr_fixed.hand.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "name": { "type": "string" }
            },
            "required": ["id", "name"],
            "additionalProperties": false
        })
    }
}

pub use strict::{HandShortV1, ShortV1};

/// A strict consumer: the generated code must not `allow` the lint on either
/// the inherent (`AccountV1`) or the fallback (`HandAccountV1`) path. Generic
/// fixtures stay outside: schemars' generic `JsonSchema` derive allows it.
mod strict {
    #![forbid(unused_imports)]

    use super::{AccountV1, HandAccountV1};
    use gts_macros::struct_to_gts_schema;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = AccountV1,
        type_id = "gts.x.test.ovr_fixed.account.v1~x.test.ovr_fixed.short.v1~",
        description = "Short account name",
        properties = "",
        overrides(AccountV1 = { #[schemars(length(max = 8))] name: String })
    )]
    pub struct ShortV1;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = HandAccountV1,
        type_id = "gts.x.test.ovr_fixed.hand.v1~x.test.ovr_fixed.hand_short.v1~",
        description = "Short name on a hand-written parent",
        properties = "",
        overrides(HandAccountV1 = { #[schemars(length(max = 8))] name: String })
    )]
    pub struct HandShortV1;
}

/// Fixed level nested in a generic ancestor's slot.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_fixed.event.v1~",
    description = "Event",
    properties = "id,source,payload"
)]
pub struct EventV1<P> {
    pub id: gts::GtsInstanceId,
    pub source: String,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_fixed.event.v1~x.test.ovr_fixed.order.v1~",
    description = "Order event without a slot",
    properties = "order_id"
)]
pub struct OrderV1 {
    pub order_id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OrderV1,
    type_id = "gts.x.test.ovr_fixed.event.v1~x.test.ovr_fixed.order.v1~x.test.ovr_fixed.short_order.v1~",
    description = "Short ids on both levels",
    properties = "",
    overrides(
        OrderV1 = { #[schemars(length(max = 8))] order_id: String },
        EventV1 = { #[schemars(length(max = 8))] source: String },
    )
)]
pub struct ShortOrderV1;

// Invalid: each fails its generated test.

/// Widens the parent's `maxLength: 8`.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = ShortV1,
    type_id = "gts.x.test.ovr_fixed.account.v1~x.test.ovr_fixed.short.v1~x.test.ovr_fixed.wide.v1~",
    description = "Widened name",
    properties = "",
    overrides(AccountV1 = { #[schemars(length(max = 16))] name: String })
)]
pub struct WideV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AccountV1,
    type_id = "gts.x.test.ovr_fixed.account.v1~x.test.ovr_fixed.numeric.v1~",
    description = "Retyped name",
    properties = "",
    overrides(AccountV1 = { name: u32 })
)]
pub struct NumericV1;

/// Overrides cannot add a field to a slotless parent.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AccountV1,
    type_id = "gts.x.test.ovr_fixed.account.v1~x.test.ovr_fixed.extra.v1~",
    description = "Adds a field through overrides",
    properties = "",
    overrides(AccountV1 = { extra: String })
)]
pub struct ExtraV1;

fn assert_override_only(schema: &serde_json::Value, parent: &str, properties: &serde_json::Value) {
    assert_eq!(schema["allOf"][0]["$ref"], format!("gts://{parent}"));
    assert_eq!(&schema["allOf"][1]["properties"], properties);
    assert!(schema["allOf"][1].get("additionalProperties").is_none());
}

#[test]
fn override_only_children_narrow_fixed_parents() {
    let short = serde_json::json!({ "name": { "type": "string", "maxLength": 8 } });
    assert_override_only(
        &ShortV1::gts_schema_with_refs_allof(),
        AccountV1::TYPE_ID,
        &short,
    );
    assert_override_only(
        &HandShortV1::gts_schema_with_refs_allof(),
        HandAccountV1::TYPE_ID,
        &short,
    );
    assert_override_only(
        &ShorterV1::gts_schema_with_refs_allof(),
        ShortV1::TYPE_ID,
        &serde_json::json!({ "name": { "type": "string", "maxLength": 4 } }),
    );
    // The slotless level is nested at `payload`; the ancestor's at the root.
    assert_override_only(
        &ShortOrderV1::gts_schema_with_refs_allof(),
        OrderV1::TYPE_ID,
        &serde_json::json!({
            "payload": {
                "type": "object",
                "properties": { "order_id": { "type": "string", "maxLength": 8 } },
                "required": ["order_id"]
            },
            "source": { "type": "string", "maxLength": 8 }
        }),
    );

    for validate in [
        ShortV1::validate_overrides,
        ShorterV1::validate_overrides,
        HandShortV1::validate_overrides,
        ShortOrderV1::validate_overrides,
    ] {
        validate().unwrap();
    }
    assert_eq!(ShorterV1::gts_schema_chain().len(), 3);
    assert_eq!(ShorterV1::GENERIC_FIELD, None);
    assert!(ShorterV1::GTS_CHAIN_OVERRIDES);
}

#[test]
fn resolved_schemas_enforce_the_narrowed_fields() {
    let id = format!("{}x.test.ovr_fixed.first.v1", ShorterV1::TYPE_ID);
    let resolved = ShorterV1::gts_resolved_schema();
    let validator = jsonschema::validator_for(&resolved).unwrap();
    let mut account = serde_json::json!({ "id": id, "name": "abcd", "kind": "k" });
    assert!(validator.is_valid(&account));
    account["name"] = "abcde".into();
    assert!(!validator.is_valid(&account));
    account["name"] = "abcd".into();
    account["extra"] = true.into();
    assert!(!validator.is_valid(&account));

    let resolved = HandShortV1::gts_resolved_schema();
    let validator = jsonschema::validator_for(&resolved).unwrap();
    assert!(validator.is_valid(&serde_json::json!({ "id": "x", "name": "abcdefgh" })));
    assert!(!validator.is_valid(&serde_json::json!({ "id": "x", "name": "abcdefghi" })));
}

#[test]
fn the_registry_admits_override_only_children_of_fixed_parents() {
    let schemas = [
        AccountV1::gts_schema_with_refs_allof(),
        ShortV1::gts_schema_with_refs_allof(),
        ShorterV1::gts_schema_with_refs_allof(),
        HandAccountV1::gts_schema_with_refs_allof(),
        HandShortV1::gts_schema_with_refs_allof(),
        EventV1::<()>::gts_schema_with_refs_allof(),
        OrderV1::gts_schema_with_refs_allof(),
        ShortOrderV1::gts_schema_with_refs_allof(),
    ];
    let mut ops = gts::GtsOps::new(None, None, 0);
    let added = ops.add_schemas_with(&schemas, true, gts::GtsRefValidation::default());
    assert!(
        added.ok,
        "{}",
        serde_json::to_string(&added.results).unwrap()
    );
    for id in [
        ShorterV1::TYPE_ID,
        HandShortV1::TYPE_ID,
        ShortOrderV1::TYPE_ID,
    ] {
        let result = ops.validate_schema(id);
        assert!(result.ok, "{id}: {}", result.error);
    }
}

#[test]
fn invalid_overrides_on_fixed_parents_are_rejected() {
    let error = WideV1::validate_overrides().unwrap_err().to_string();
    assert!(
        error.contains("field '/name'") && error.contains("widens"),
        "{error}"
    );
    let error = NumericV1::validate_overrides().unwrap_err().to_string();
    assert!(
        error.contains("field '/name'") && error.contains("type"),
        "{error}"
    );
    let error = ExtraV1::validate_overrides().unwrap_err().to_string();
    assert!(
        error.contains("field '/extra': is not declared by the ancestor"),
        "{error}"
    );
}
