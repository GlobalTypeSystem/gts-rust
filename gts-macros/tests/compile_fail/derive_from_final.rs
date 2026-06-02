//! Deriving from a `gts_final = true` type must be a compile error: a final
//! type is not inheritable. Guard 1 reads the parent's `GtsSchema::GTS_FINAL`
//! const in a `const _` block.

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.leaf.v1~",
    description = "Final base",
    properties = "id,payload",
    gts_final = true,
)]
pub struct FinalBaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = FinalBaseV1,
    type_id = "gts.x.app.entities.leaf.v1~x.app.child.thing.v1~",
    description = "Illegal child of a final type",
    properties = "name",
)]
pub struct ChildV1 {
    pub name: String,
}

fn main() {}
