//! `traits_schema = inline(T)` where `T` does not implement
//! `schemars::JsonSchema` must fail with a clear trait-bound error.

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

// Plain struct without #[derive(JsonSchema)].
pub struct PlainTraits {
    pub retention: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.thing.v1~",
    description = "Thing",
    properties = "id",
    traits_schema = inline(PlainTraits),
)]
pub struct ThingV1 {
    pub id: GtsInstanceId,
}

fn main() {}
