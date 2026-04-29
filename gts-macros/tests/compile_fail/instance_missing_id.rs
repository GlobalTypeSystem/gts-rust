//! Test: typed `gts_instance!` rejects a struct literal that has no
//! GTS instance-id field at all. The macro requires exactly one of
//! `id` / `gts_id` / `gtsId` to be present with a string-literal value.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.acme.core.test.perm.v1~",
    description = "Test permission type for compile_fail/instance_missing_id",
    properties = "id,action"
)]
#[derive(Debug)]
pub struct PermV1 {
    pub id: GtsInstanceId,
    pub action: String,
}

fn main() {
    let _ = gts_instance!(PermV1 {
        action: "read".to_owned(),
    });
}
