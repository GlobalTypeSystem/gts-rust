//! `traits_schema = inline(T)` where `T` implements `schemars::JsonSchema` but
//! is NOT marked with `#[gts_traits_schema]` must fail: the inline form is
//! opt-in via the `gts::GtsTraitsSchema` marker, mirroring the `$ref` form's
//! `gts::GtsSchema` gate.

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

// Derives JsonSchema but is missing the `#[gts_traits_schema]` marker.
#[derive(JsonSchema)]
pub struct UnmarkedTraits {
    pub retention: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.thing.v1~",
    description = "Thing",
    properties = "id",
    traits_schema = inline(UnmarkedTraits),
)]
pub struct ThingV1 {
    pub id: GtsInstanceId,
}

fn main() {}
