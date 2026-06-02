// Golden case: BOTH a base and a derived type declare `x-gts-traits-schema`.
// The base declares `priority` as an open string; the derived **narrows** it to
// an enum. The macro emits each type's own trait-schema at the document top
// level; the registry composes them via `allOf` along the chain. The
// behavioural counterpart (accept/reject) lives in `traits_tests.rs`.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct BasePriorityTraits {
    #[schemars(extend("default" = "medium"))]
    pub priority: String,
}

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct NarrowPriorityTraits {
    #[schemars(extend("enum" = ["low", "medium", "high", "critical"]))]
    pub priority: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.golden.narrow.v1~",
    description = "Base declaring an open-string priority trait",
    properties = "id,payload",
    traits_schema = inline(BasePriorityTraits),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct NarrowBaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = NarrowBaseV1,
    type_id = "gts.x.test.golden.narrow.v1~x.test.urgent.event.v1~",
    description = "Derived narrowing priority to an enum and resolving it",
    properties = "order_id",
    traits_schema = inline(NarrowPriorityTraits),
    traits = serde_json::json!({ "priority": "high" }),
    gts_final = true,
)]
#[derive(Debug)]
pub struct UrgentEventV1 {
    pub order_id: String,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            NarrowBaseV1::<()>::TYPE_ID.to_owned(),
            NarrowBaseV1::<()>::gts_schema_with_refs(),
        ),
        (
            UrgentEventV1::TYPE_ID.to_owned(),
            UrgentEventV1::gts_schema_with_refs(),
        ),
    ]
}
