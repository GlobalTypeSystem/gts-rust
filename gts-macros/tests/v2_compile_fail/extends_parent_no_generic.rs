//! Test: extending a non-generic parent is rejected.
//!
//! The macro's base-assertion emits `<Parent<()> as GtsSchema>::SCHEMA_ID`, which triggers
//! an arity mismatch (E0107) when `Parent` has no type params — catching the "parent must
//! be generic" rule before the `PARENT_GENERIC_FIELD` runtime check would ever be reached.

use gts::GtsSchemaId;
use gts_macros::GtsSchema;
use schemars::JsonSchema;

// Parent struct with NO generic field (leaf/terminal type)
#[derive(Debug, serde::Serialize, serde::Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.leaf.v1~",
    description = "Leaf type with no generic field"
)]
pub struct LeafTypeV1 {
    #[gts(type_field)]
    pub gts_type: GtsSchemaId,
    pub name: String,
}

// This should fail: trying to extend a parent with no generic field
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.leaf.v1~x.app.entities.child.v1~",
    description = "Child trying to extend leaf type (invalid)",
    extends = LeafTypeV1
)]
pub struct ChildOfLeafV1 {
    pub extra_field: String,
}

fn main() {}
