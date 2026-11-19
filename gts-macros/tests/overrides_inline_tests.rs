#![allow(clippy::expect_used)]

// Inline and named helpers must emit identical overlays, including serde
// attributes, Option, enums and nested structs. `inline_leaf` denies warnings
// to verify hidden helpers need no lint allowances in user code.

use gts::{GtsInstanceId, GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;

use schemars::JsonSchema;

const ORDER_TYPE: &str = gts_macros::gts_id!("x.core.orders.order.v1~");

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct ShippingAddress {
    #[schemars(length(max = 100))]
    pub city: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.ovr_inline.event.v1~"),
    description = "Event envelope",
    properties = "id,subject_id,subject_type,event_source,trace_id,payload"
)]
#[derive(Debug)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    #[schemars(length(min = 1, max = 1024))]
    pub subject_id: String,
    pub subject_type: GtsTypeId,
    #[serde(rename = "source")]
    #[schemars(length(max = 128))]
    pub event_source: Option<String>,
    #[schemars(length(max = 64))]
    pub trace_id: Option<String>,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = gts_id!("x.test.ovr_inline.event.v1~x.test.ovr_inline.order.v1~"),
    description = "Order event with shipping details",
    properties = "region,shipping_address,details",
    overrides(EventV1 = {
        #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
        subject_type: GtsTypeId,
    })
)]
#[derive(Debug)]
pub struct OrderEventV1<D> {
    #[schemars(length(max = 32))]
    pub region: String,
    pub shipping_address: ShippingAddress,
    pub details: D,
}

#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only type, never constructed
pub enum EuRegion {
    #[serde(rename = "eu-west")]
    EuWest,
    #[serde(rename = "eu-central")]
    EuCentral,
}

/// The inherited `ShippingAddress` is closed, so its replacement must be too.
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // schema-only type, never constructed
pub struct EuShippingAddress {
    #[schemars(length(max = 32))]
    pub city: String,
}

pub use inline_leaf::OrderPlacedV1;

#[deny(warnings)]
mod inline_leaf {
    use super::{EuRegion, EuShippingAddress, EventV1, ORDER_TYPE, OrderEventV1};
    use gts::GtsTypeId;
    use gts_macros::struct_to_gts_schema;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = OrderEventV1,
        type_id = gts_id!("x.test.ovr_inline.event.v1~x.test.ovr_inline.order.v1~x.test.ovr_inline.placed.v1~"),
        description = "Order placed in the EU",
        properties = "order_id",
        overrides(
            EventV1 = {
                #[schemars(length(min = 36, max = 36), extend("format" = "uuid"))]
                subject_id: String,
                #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
                subject_type: GtsTypeId,
                /// Optional and non-null; the doc comment is an annotation and is dropped.
                #[serde(rename = "source", default)]
                #[schemars(length(max = 64))]
                event_source: String,
                #[schemars(length(max = 32))]
                trace_id: Option<String>,
            },
            OrderEventV1 = { region: EuRegion, shipping_address: EuShippingAddress },
        )
    )]
    #[derive(Debug)]
    pub struct OrderPlacedV1 {
        pub order_id: String,
    }

    // Regression: a raw identifier names the hidden helper without its `r#`.
    use super::EventV1 as r#type;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = OrderEventV1,
        type_id = gts_id!("x.test.ovr_inline.event.v1~x.test.ovr_inline.order.v1~x.test.ovr_inline.raw_alias.v1~"),
        description = "Raw identifier ancestor alias",
        properties = "",
        overrides(r#type = { #[schemars(length(min = 1, max = 8))] subject_id: String })
    )]
    pub struct RawAliasV1;

    // Paths in inline field types and string attributes resolve in this
    // module, as for a named helper declared here; the host is generic.
    type Text = String;

    fn default_trace_id() -> Text {
        String::new()
    }

    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)] // schema-only helper, never constructed
    struct TraceRules {
        #[serde(default = "self::default_trace_id")]
        #[schemars(schema_with = "super::short_text")]
        trace_id: self::Text,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = OrderEventV1,
        type_id = gts_id!("x.test.ovr_inline.event.v1~x.test.ovr_inline.order.v1~x.test.ovr_inline.traced.v1~"),
        description = "Order event with a short trace ID",
        properties = "fulfillment",
        overrides(EventV1 = {
            #[serde(default = "self::default_trace_id")]
            #[schemars(schema_with = "super::short_text")]
            trace_id: self::Text,
        })
    )]
    pub struct TracedOrderEventV1<E> {
        pub fulfillment: E,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = OrderEventV1,
        type_id = gts_id!("x.test.ovr_inline.event.v1~x.test.ovr_inline.order.v1~x.test.ovr_inline.named_traced.v1~"),
        description = "Order event with a short trace ID",
        properties = "fulfillment",
        overrides(EventV1 = TraceRules)
    )]
    pub struct NamedTracedOrderEventV1<E> {
        pub fulfillment: E,
    }

    #[test]
    fn inline_paths_resolve_in_the_type_module() {
        use gts::GtsSchema;
        TracedOrderEventV1::<()>::validate_overrides().expect("scoped inline override is admitted");
        let inline = TracedOrderEventV1::<()>::gts_schema_with_refs();
        assert_eq!(
            inline["allOf"][1],
            NamedTracedOrderEventV1::<()>::gts_schema_with_refs()["allOf"][1]
        );
        assert_eq!(inline["allOf"][1]["properties"]["trace_id"]["maxLength"], 8);
    }
}

fn short_text(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": "string", "maxLength": 8 })
}

#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct EventRules {
    #[schemars(length(min = 36, max = 36), extend("format" = "uuid"))]
    subject_id: String,
    #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
    subject_type: GtsTypeId,
    /// Optional and non-null; the doc comment is an annotation and is dropped.
    #[serde(rename = "source", default)]
    #[schemars(length(max = 64))]
    event_source: String,
    #[schemars(length(max = 32))]
    trace_id: Option<String>,
}

#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct OrderRules {
    region: EuRegion,
    shipping_address: EuShippingAddress,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OrderEventV1,
    type_id = gts_id!("x.test.ovr_inline.event.v1~x.test.ovr_inline.order.v1~x.test.ovr_inline.named_placed.v1~"),
    description = "Order placed in the EU",
    properties = "order_id",
    overrides(EventV1 = EventRules, OrderEventV1 = OrderRules)
)]
#[derive(Debug)]
pub struct NamedOrderPlacedV1 {
    pub order_id: String,
}

#[test]
fn inline_fields_mean_the_same_as_a_named_helper() {
    OrderPlacedV1::validate_overrides().expect("inline overrides are admitted");
    NamedOrderPlacedV1::validate_overrides().expect("named overrides are admitted");
    for (inline, named) in [
        (
            OrderPlacedV1::gts_schema_with_refs(),
            NamedOrderPlacedV1::gts_schema_with_refs(),
        ),
        (
            OrderPlacedV1::gts_schema_with_refs_allof(),
            NamedOrderPlacedV1::gts_schema_with_refs_allof(),
        ),
    ] {
        assert_eq!(inline["allOf"][1], named["allOf"][1]);
    }
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.ovr_inline.membership.v1~"),
    description = "Membership link",
    properties = "id,resource_type"
)]
pub struct MembershipV1 {
    pub id: GtsInstanceId,
    #[schemars(extend("x-gts-type-ref" = "gts.*"))]
    pub resource_type: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = MembershipV1,
    type_id = gts_id!("x.test.ovr_inline.membership.v1~x.test.ovr_inline.order_membership.v1~"),
    description = "Membership link for orders",
    properties = "",
    overrides(MembershipV1 = {
        #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
        resource_type: String,
    })
)]
pub struct OrderMembershipV1;

#[test]
fn override_only_child_narrows_a_fixed_membership_schema() {
    OrderMembershipV1::validate_overrides().expect("membership reference can narrow to orders");
    let base = MembershipV1::gts_schema_with_refs();
    let resolved = OrderMembershipV1::gts_resolved_schema();
    let validator = jsonschema::validator_for(&resolved).expect("resolved membership schema");
    let mut link = serde_json::json!({
        "id": format!("{}x.test.orders.first.v1", OrderMembershipV1::TYPE_ID),
        "resource_type": ORDER_TYPE,
    });
    assert!(validator.is_valid(&link));
    link["extra"] = serde_json::json!(true);
    assert!(!validator.is_valid(&link));
    for schema in [
        OrderMembershipV1::gts_schema_with_refs(),
        OrderMembershipV1::gts_schema_with_refs_allof(),
    ] {
        assert_eq!(schema["allOf"][0]["$ref"], base["$id"]);
        assert_eq!(
            schema["allOf"][1]["properties"],
            serde_json::json!({"resource_type": {"type": "string", "x-gts-type-ref": ORDER_TYPE}})
        );
    }
}

mod directional_names {
    use super::*;

    #[struct_to_gts_schema(dir_path = "schemas", base = true,
        type_id = gts_id!("x.test.ovr_names.parent.v1~"), description = "Parent",
        properties = "subject,payload")]
    pub struct ParentV1<P> {
        #[serde(rename(serialize = "subjectOut", deserialize = "subject_in"), default)]
        pub subject: String,
        pub payload: P,
    }

    #[struct_to_gts_schema(dir_path = "schemas", base = ParentV1,
        type_id = gts_id!("x.test.ovr_names.parent.v1~x.test.ovr_names.inline.v1~"),
        description = "Inline", properties = "value",
        overrides(ParentV1 = {
            #[serde(rename(serialize = "subjectOut", deserialize = "subject_in"), default)]
            #[schemars(length(min = 2))]
            subject: String,
        }))]
    pub struct InlineV1 {
        pub value: u32,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct Rules {
        #[serde(rename(serialize = "subjectOut", deserialize = "subject_in"), default)]
        #[schemars(length(min = 2))]
        subject: String,
    }

    #[struct_to_gts_schema(dir_path = "schemas", base = ParentV1,
        type_id = gts_id!("x.test.ovr_names.parent.v1~x.test.ovr_names.named.v1~"),
        description = "Named", properties = "value", overrides(ParentV1 = Rules))]
    pub struct NamedV1 {
        pub value: u32,
    }

    #[struct_to_gts_schema(dir_path = "schemas", base = ParentV1,
        type_id = gts_id!("x.test.ovr_names.parent.v1~x.test.ovr_names.legacy.v1~"),
        description = "Existing override in the ancestor's input namespace", properties = "value",
        overrides(ParentV1 = {
            #[serde(default)]
            #[schemars(length(min = 2))]
            subject_in: String,
        }))]
    pub struct LegacyV1 {
        pub value: u32,
    }

    #[test]
    fn overrides_use_the_wire_name_and_preserve_input_optionality() {
        InlineV1::validate_overrides().expect("inline override");
        NamedV1::validate_overrides().expect("named override");
        let inline = InlineV1::gts_schema_with_refs();
        let named = NamedV1::gts_schema_with_refs();
        assert_eq!(inline["allOf"][1], named["allOf"][1]);
        LegacyV1::validate_overrides().expect("unambiguous input spelling");
        assert_eq!(
            inline["allOf"][1],
            LegacyV1::gts_schema_with_refs()["allOf"][1]
        );
        let schema = NamedV1::gts_resolved_schema();
        let validator = jsonschema::validator_for(&schema).expect("resolved schema");
        for (subject, expected) in [(None, true), (Some("ok"), true), (Some("x"), false)] {
            let mut instance = serde_json::json!({"payload":{"value":1}});
            if let Some(subject) = subject {
                instance["subjectOut"] = subject.into();
            }
            assert_eq!(validator.is_valid(&instance), expected, "{instance}");
        }
    }
}

mod overlapping_names {
    use super::*;

    #[struct_to_gts_schema(dir_path = "schemas", base = true,
        type_id = gts_id!("x.test.ovr_names.swap.v1~"), description = "Swapped parent",
        properties = "a,b,payload")]
    pub struct SwapV1<P> {
        #[serde(rename(serialize = "b", deserialize = "a"))]
        pub a: String,
        #[serde(rename(serialize = "a", deserialize = "b"), default)]
        pub b: u32,
        pub payload: P,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct Swapped {
        #[serde(rename(serialize = "b", deserialize = "a"))]
        #[schemars(length(min = 2))]
        a: String,
        #[serde(rename(serialize = "a", deserialize = "b"), default)]
        #[schemars(range(max = 9))]
        b: u32,
    }

    #[struct_to_gts_schema(dir_path = "schemas", base = SwapV1,
        type_id = gts_id!("x.test.ovr_names.swap.v1~x.test.ovr_names.swap_child.v1~"),
        description = "Swap child", properties = "", overrides(SwapV1 = Swapped))]
    pub struct SwapChildV1;

    #[test]
    fn swapped_helper_names_move_their_constraints_and_optionality_together() {
        SwapChildV1::validate_overrides().expect("override under swapped wire names");
        let s = SwapChildV1::gts_resolved_schema();
        let v = jsonschema::validator_for(&s).expect("schema");
        assert!(v.is_valid(&serde_json::json!({"b":"ok","payload":{}})));
        assert!(!v.is_valid(&serde_json::json!({"b":"x","payload":{}})));
        assert!(!v.is_valid(&serde_json::json!({"a":10,"b":"ok","payload":{}})));
    }
}
