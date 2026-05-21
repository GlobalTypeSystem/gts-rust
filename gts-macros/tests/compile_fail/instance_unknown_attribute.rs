//! Test: typed `gts_instance!` rejects an attribute on the struct
//! literal that isn't `#[gts_static(...)]`. The macro owns its modifier
//! surface; arbitrary attributes shouldn't silently pass through.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.acme.core.test.perm.v1~",
    description = "Test permission type for compile_fail/instance_unknown_attribute",
    properties = "id,action"
)]
#[derive(Debug)]
pub struct PermV1 {
    pub id: GtsInstanceId,
    pub action: String,
}

fn main() {
    let _ = gts_instance! {
        #[gts_unknown(foo)]
        PermV1 {
            id: "gts.acme.core.test.perm.v1~vendor.app.test.x.v1",
            action: "read".to_owned(),
        }
    };
}
