//! A derived type providing `traits` under an ancestor that declares
//! `traits_schema = false` must fail to compile: the chain prohibits all traits
//! (chain-aggregated guard, `Prohibited` state).

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.closed.v1~",
    description = "Base prohibiting traits",
    properties = "id,payload",
    traits_schema = false,
)]
pub struct ClosedBaseV1<P> {
    pub id: gts::GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = ClosedBaseV1,
    type_id = "gts.x.app.entities.closed.v1~x.app.child.thing.v1~",
    description = "Derived supplying traits under a prohibiting base",
    properties = "name",
    traits = serde_json::json!({ "retention": "P30D" }),
)]
pub struct ChildV1 {
    pub name: String,
}

fn main() {}
