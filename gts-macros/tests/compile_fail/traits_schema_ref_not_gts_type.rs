//! Bare `traits_schema = T` ($ref form) where `T` is not a `#[struct_to_gts_schema]`
//! type must fail with a clear `GtsSchema` trait-bound error.

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

// Plain struct: not a registered GTS type, so it has no TYPE_ID / GtsSchema impl.
pub struct PlainType {
    pub retention: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.thing.v1~",
    description = "Thing",
    properties = "id",
    traits_schema = PlainType,
)]
pub struct ThingV1 {
    pub id: GtsInstanceId,
}

fn main() {}
