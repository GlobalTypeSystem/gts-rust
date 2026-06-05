//! `traits_schema = inline(T)` where `T` is neither marked with
//! `#[gts_traits_schema]` nor implements `schemars::JsonSchema` must fail: the
//! `gts::GtsTraitsSchema` gate fires first, and the JsonSchema-bounded inline
//! value builder fires second. (The clean single-error cases are
//! `traits_inline_not_marked` and `traits_marked_not_jsonschema`.)

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
