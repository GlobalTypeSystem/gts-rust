//! Test: typed `gts_instance!` rejects an `id:` literal whose prefix
//! doesn't match the type's `<S as GtsSchema>::SCHEMA_ID`. The check
//! fires at compile time via the const-assert in the macro expansion.
//! Fixture is built via `#[struct_to_gts_schema]` to match the canonical
//! usage pattern.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.acme.core.test.perm.v1~",
    description = "Test permission type for compile_fail/instance_id_prefix_mismatch",
    properties = "id,action"
)]
#[derive(Debug)]
pub struct PermV1 {
    pub id: GtsInstanceId,
    pub action: String,
}

fn main() {
    // Format-valid id, but the prefix does not match `PermV1::SCHEMA_ID`.
    let _ = gts_instance!(PermV1 {
        id: "gts.zzz.core.other.thing.v1~vendor.app.test.x.v1",
        action: "read".to_owned(),
    });
}
