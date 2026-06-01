//! `traits = ...` on a `base = true` host without `traits_schema = ...` must be
//! a compile error - a base host has no parent to carry trait values against.

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.thing.v1~",
    description = "Thing",
    properties = "id",
    traits = serde_json::json!({ "retention": "P30D" }),
)]
pub struct Thing {
    pub id: GtsInstanceId,
}

fn main() {}
