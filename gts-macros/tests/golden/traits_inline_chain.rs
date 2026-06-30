// Golden case: inline trait-shape on an abstract base + a final leaf that
// resolves values. Exercises `traits_schema = inline(T)`, schemars `extend`
// for `x-gts-ref`/`const`, serde `default`, top-level keyword placement, and
// `gts_abstract`/`gts_final`.
//
// Both `severity` and `escalation` are non-primitive named types. schemars
// would normally reference them via `$ref: "#/$defs/<Name>"`, but the embedded
// `x-gts-traits-schema` fragment carries no `$defs`, so they must be inlined in
// full. `escalation` is a *nested struct that itself contains a named type*
// (`Severity`), so its inlining is transitive — exercising what a field-level
// `#[schemars(inline)]` cannot do, only generator-wide `inline_subschemas` can.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::{gts_id, struct_to_gts_schema, GtsTraitsSchema};
use schemars::JsonSchema;

const TOPIC_REF: &str = gts_id!("x.core.events.topic.v1~");

fn default_retention() -> String {
    "P30D".to_owned()
}

#[derive(JsonSchema, serde::Serialize)]
#[allow(dead_code)] // variants are exercised only through the JsonSchema derive
pub enum Severity {
    Low,
    High,
}

#[derive(JsonSchema, serde::Serialize)]
#[allow(dead_code)] // fields are exercised only through the JsonSchema derive
pub struct Escalation {
    pub after: String,
    pub to: Severity,
}

#[derive(JsonSchema, serde::Serialize, GtsTraitsSchema)]
pub struct EventTraits {
    #[schemars(extend("x-gts-ref" = TOPIC_REF))]
    pub topic_ref: String,
    #[serde(default = "default_retention")]
    pub retention: String,
    #[schemars(extend("const" = true))]
    pub indexed: bool,
    pub severity: Severity,
    pub escalation: Escalation,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.golden.event.v1~"),
    description = "Base event",
    properties = "id,payload",
    traits_schema = inline(EventTraits),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = gts_id!("x.test.golden.event.v1~x.test.order.placed.v1~"),
    description = "Order placed",
    properties = "order_id",
    traits = serde_json::json!({
        "topic_ref": gts_id!("x.core.events.topic.v1~x.test._.orders.v1"),
        "indexed": true,
        "severity": "High",
        "escalation": { "after": "PT5M", "to": "Low" }
    }),
    gts_final = true,
)]
#[derive(Debug)]
pub struct OrderPlacedV1 {
    pub order_id: String,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            EventV1::<()>::TYPE_ID.to_owned(),
            EventV1::<()>::gts_schema_with_refs(),
        ),
        (
            OrderPlacedV1::TYPE_ID.to_owned(),
            OrderPlacedV1::gts_schema_with_refs(),
        ),
    ]
}
