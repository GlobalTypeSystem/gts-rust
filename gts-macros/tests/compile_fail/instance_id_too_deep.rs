//! Test: typed `gts_instance!` rejects an `id:` literal whose segment
//! contains additional `~` separators (i.e. the literal denotes a deeper
//! schema chain than the type's `SCHEMA_ID`). For chained schemas the
//! caller must use a generic carrier with the conforming type as a
//! turbofish parameter (`BaseV1::<LeafV1> { ... }`); a non-generic
//! carrier like `PermV1` cannot legitimately produce a deeper id.
//! Fixture is built via `#[struct_to_gts_schema]` to match the canonical
//! usage pattern.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.acme.core.test.perm.v1~",
    description = "Test permission type for compile_fail/instance_id_too_deep",
    properties = "id,action"
)]
#[derive(Debug)]
pub struct PermV1 {
    pub id: GtsInstanceId,
    pub action: String,
}

fn main() {
    // Prefix matches SCHEMA_ID, but the literal continues with another
    // `~`-separated segment, implying a deeper schema. The const-assert
    // rejects (no turbofish path is available — `PermV1` is non-generic).
    let _ = gts_instance!(PermV1 {
        id: "gts.acme.core.test.perm.v1~vendor.app.test.deeper.v1~vendor.app.test.leaf.v1",
        action: "read".to_owned(),
    });
}
