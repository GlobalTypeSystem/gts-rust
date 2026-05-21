//! Test: typed `gts_instance!` rejects an `id:` field whose value isn't
//! a string literal. The macro needs to validate the GTS id shape and
//! split it at compile time, so a runtime expression can't be used.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.acme.core.test.perm.v1~",
    description = "Test permission type for compile_fail/instance_id_not_literal",
    properties = "id,action"
)]
#[derive(Debug)]
pub struct PermV1 {
    pub id: GtsInstanceId,
    pub action: String,
}

fn main() {
    let runtime_id = "gts.acme.core.test.perm.v1~vendor.app.test.x.v1".to_owned();
    let _ = gts_instance!(PermV1 {
        id: runtime_id,
        action: "read".to_owned(),
    });
}
