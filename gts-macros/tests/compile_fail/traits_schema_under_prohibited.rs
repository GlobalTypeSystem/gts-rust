//! Declaring a `traits_schema` under an ancestor that prohibits traits
//! (`traits_schema = false`) must fail to compile — even when this type
//! supplies no `traits` values. The composed chain is `allOf[T, false]`,
//! which is unsatisfiable; the symmetric guard to `traits_prohibited_by_chain`.

use gts_macros::{struct_to_gts_schema, GtsTraitsSchema};
use schemars::JsonSchema;

#[derive(JsonSchema, serde::Serialize, GtsTraitsSchema)]
pub struct ChildTraits {
    pub retention: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.sealed.v1~",
    description = "Base prohibiting traits",
    properties = "id,payload",
    traits_schema = false,
)]
pub struct SealedBaseV1<P> {
    pub id: gts::GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SealedBaseV1,
    type_id = "gts.x.app.entities.sealed.v1~x.app.child.thing.v1~",
    description = "Derived declaring a traits_schema under a prohibiting base",
    properties = "name",
    traits_schema = inline(ChildTraits),
)]
pub struct ChildV1 {
    pub name: String,
}

fn main() {}
