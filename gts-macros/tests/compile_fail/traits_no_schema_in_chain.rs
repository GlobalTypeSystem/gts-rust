//! A derived type providing `traits` when neither it nor any ancestor declares
//! a `traits_schema` must fail to compile: there is no trait shape in the chain
//! for the values to validate against (chain-aggregated guard, `Absent` state).

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.thing.v1~",
    description = "Base without any traits-schema",
    properties = "id,payload",
)]
pub struct BaseV1<P> {
    pub id: gts::GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseV1,
    type_id = "gts.x.app.entities.thing.v1~x.app.child.thing.v1~",
    description = "Derived supplying traits with no schema anywhere in the chain",
    properties = "name",
    traits = serde_json::json!({ "retention": "P30D" }),
)]
pub struct ChildV1 {
    pub name: String,
}

fn main() {}
