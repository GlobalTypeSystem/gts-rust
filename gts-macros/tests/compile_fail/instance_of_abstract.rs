//! Instantiating a `gts_abstract = true` type via `gts_instance!` must be a
//! compile error: an abstract type is not directly instantiable. Guard 2 reads
//! the target's `GtsSchema::GTS_ABSTRACT` const in a `const _` block alongside
//! the id-prefix assertion.

use gts::GtsInstanceId;
use gts_macros::{gts_instance, struct_to_gts_schema};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.events.thing.v1~",
    description = "Abstract thing",
    properties = "id,name",
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct AbstractThingV1 {
    pub id: GtsInstanceId,
    pub name: String,
}

fn main() {
    let _ = gts_instance!(AbstractThingV1 {
        id: "gts.x.app.events.thing.v1~vendor.app.x.thing.v1",
        name: "x".to_owned(),
    });
}
