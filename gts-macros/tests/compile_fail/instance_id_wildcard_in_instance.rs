//! Test: typed `gts_instance!` rejects an id literal containing a
//! wildcard (`*`). Wildcards are valid only in pattern-matching contexts
//! (`<schema>~*` for "any instance of this type"), never in concrete
//! instance ids — `parse_gts_string(_, allow_wildcards=false)` enforces
//! this at proc-macro time.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.acme.core.test.perm.v1~",
    description = "Test permission type for compile_fail/instance_id_wildcard_in_instance",
    properties = "id,action"
)]
#[derive(Debug)]
pub struct PermV1 {
    pub id: GtsInstanceId,
    pub action: String,
}

fn main() {
    let _ = gts_instance!(PermV1 {
        id: "gts.acme.core.test.perm.v1~vendor.*.test.x.v1",
        action: "read".to_owned(),
    });
}
