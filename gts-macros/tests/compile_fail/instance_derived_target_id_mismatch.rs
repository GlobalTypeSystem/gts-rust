//! Test: typed `gts_instance!` rejects a chained generic carrier whose
//! turbofish-derived target's `TYPE_ID` is not the direct parent of
//! the `id:` literal. Here the carrier is `BaseV1::<LeafV1>`, so the
//! macro derives `LeafV1` as the const-assert target — but the literal
//! is a base-level id (only one segment past `BaseV1::TYPE_ID`), not
//! a leaf-level id. `<LeafV1 as GtsSchema>::TYPE_ID` is therefore not
//! a prefix of the literal, and the const-assert rejects.
//! Fixture is built via `#[struct_to_gts_schema]` to match the canonical
//! usage pattern.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.acme.core.test.base.v1~",
    description = "Generic base type for compile_fail/instance_derived_target_id_mismatch",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct BaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseV1,
    type_id = "gts.acme.core.test.base.v1~acme.core.test.leaf.v1~",
    description = "Derived leaf type for compile_fail/instance_derived_target_id_mismatch",
    properties = "name"
)]
#[derive(Debug)]
pub struct LeafV1 {
    pub name: String,
}

fn main() {
    // Turbofish carrier `BaseV1::<LeafV1>` makes the macro derive the
    // const-assert target as `LeafV1`. The literal below is a base-level
    // instance id (single segment past `BaseV1::TYPE_ID`), so the
    // derived target's `TYPE_ID` is not a direct parent of the
    // literal — the const-assert prefix check rejects.
    let _v: BaseV1<LeafV1> = gts_instance!(BaseV1::<LeafV1> {
        id: "gts.acme.core.test.base.v1~vendor.app.things.bare.v1",
        payload: LeafV1 {
            name: "ex".to_owned()
        },
    });
}
