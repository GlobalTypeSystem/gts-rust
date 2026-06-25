// Golden case: trait *values* supplied as a typed Rust struct literal rather
// than `serde_json::json!`. The macro accepts any expression for `traits = …`
// and serializes it via `serde_json::to_value`, so a `#[derive(Serialize)]`
// struct literal is the type-checked, idiomatic way to resolve several trait
// fields at once. The same `OrderTraits` type is reused as the base's inline
// `x-gts-traits-schema`, so the leaf's values are an instance of that shape.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::{struct_to_gts_schema, GtsTraitsSchema};
use schemars::JsonSchema;

#[derive(JsonSchema, serde::Serialize, GtsTraitsSchema)]
pub struct OrderTraits {
    #[schemars(extend("x-gts-ref" = "gts.x.core.events.topic.v1~"))]
    pub topic_ref: String,
    pub retention: String,
    pub indexed: bool,
    pub partition_count: u32,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.golden.litevent.v1~"),
    description = "Abstract base declaring the order trait shape",
    properties = "id,payload",
    traits_schema = inline(OrderTraits),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct LitEventV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = LitEventV1,
    type_id = gts_id!("x.test.golden.litevent.v1~x.test.order.placed.v1~"),
    description = "Leaf resolving every trait via a struct literal",
    properties = "order_id",
    traits = OrderTraits {
        topic_ref: "gts.x.core.events.topic.v1~x.test._.orders.v1".to_owned(),
        retention: "P90D".to_owned(),
        indexed: true,
        partition_count: 8,
    },
    gts_final = true,
)]
#[derive(Debug)]
pub struct LitOrderPlacedV1 {
    pub order_id: String,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            LitEventV1::<()>::TYPE_ID.to_owned(),
            LitEventV1::<()>::gts_schema_with_refs(),
        ),
        (
            LitOrderPlacedV1::TYPE_ID.to_owned(),
            LitOrderPlacedV1::gts_schema_with_refs(),
        ),
    ]
}
