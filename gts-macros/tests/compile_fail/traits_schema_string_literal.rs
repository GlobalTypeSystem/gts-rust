//! `traits_schema = "..."` (a bare string literal) must be rejected at macro
//! expansion: a trait shape can only be `true`, `false`, an `inline(T)` object
//! subschema, or a `$ref` to a GTS type - never an arbitrary string.

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.thing.v1~",
    description = "Thing",
    properties = "id",
    traits_schema = "P30D",
)]
pub struct ThingV1 {
    pub id: gts::GtsInstanceId,
}

fn main() {}
