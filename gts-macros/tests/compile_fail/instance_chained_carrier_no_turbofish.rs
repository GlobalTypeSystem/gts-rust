//! Test: typed `gts_instance!` rejects a struct literal that omits the
//! turbofish on a generic carrier (`BaseV1 { ... }` instead of
//! `BaseV1::<LeafV1> { ... }`). The macro emits
//! `<BaseV1 as GtsSchema>::TYPE_ID`, which fails because `BaseV1` is
//! generic and Rust requires explicit generics in trait position. The
//! turbofish is the macro's only signal for deriving the conforming
//! schema in chained carriers, so it is mandatory.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.acme.core.test.base.v1~",
    description = "Generic base type for compile_fail/instance_chained_carrier_no_turbofish",
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
    description = "Derived leaf type for compile_fail/instance_chained_carrier_no_turbofish",
    properties = "name"
)]
#[derive(Debug)]
pub struct LeafV1 {
    pub name: String,
}

fn main() {
    let _v: BaseV1<LeafV1> = gts_instance! {
        BaseV1 {
            id: "gts.acme.core.test.base.v1~vendor.app.things.bare.v1",
            payload: LeafV1 { name: "ex".to_owned() },
        }
    };
}
