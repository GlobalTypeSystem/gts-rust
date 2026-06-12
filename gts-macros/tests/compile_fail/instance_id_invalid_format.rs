//! Test: typed `gts_instance!` rejects a malformed id literal at
//! proc-macro time via the shared `gts_id::parse_gts_id`. Catches
//! issues like a missing `<vendor>.<package>.<namespace>.<type>.v<N>`
//! segment shape before any further checks.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.acme.core.test.perm.v1~",
    description = "Test permission type for compile_fail/instance_id_invalid_format",
    properties = "id,action"
)]
#[derive(Debug)]
pub struct PermV1 {
    pub id: GtsInstanceId,
    pub action: String,
}

fn main() {
    // Segment after `~` is missing a version suffix and has fewer
    // dot-tokens than the GTS spec requires.
    let _ = gts_instance!(PermV1 {
        id: "gts.acme.core.test.perm.v1~not.a.valid.segment",
        action: "read".to_owned(),
    });
}
