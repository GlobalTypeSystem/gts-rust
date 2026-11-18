//! Adding a field still requires a parent extension slot, even with overrides
//! and an empty properties list.

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

// Parent struct with NO generic field (leaf/terminal type)
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.leaf.v1~",
    description = "Leaf type with no generic field",
    properties = "id,name"
)]
pub struct LeafTypeV1 {
    pub id: GtsInstanceId,
    pub name: String,
}

// This should fail: trying to extend a parent with no generic field
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = LeafTypeV1,
    type_id = "gts.x.app.entities.leaf.v1~x.app.entities.child.v1~",
    description = "Child trying to extend leaf type (invalid)",
    properties = "",
    overrides(LeafTypeV1 = { #[schemars(length(max = 8))] name: String })
)]
pub struct ChildOfLeafV1 {
    pub extra_field: String,
}

fn main() {}
