//! Test: extends = Parent with single-segment schema_id

use gts_macros::GtsSchema;

struct ParentV1;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.child.v1~",
    description = "Should fail",
    extends = ParentV1
)]
pub struct ChildV1 {
    pub data: String,
}

fn main() {}
