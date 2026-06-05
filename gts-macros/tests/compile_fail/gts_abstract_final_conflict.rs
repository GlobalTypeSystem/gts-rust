//! `gts_abstract = true` together with `gts_final = true` must be rejected
//! (mutual exclusion).

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.thing.v1~",
    description = "Thing",
    properties = "id",
    gts_abstract = true,
    gts_final = true,
)]
pub struct Thing {
    pub id: gts::GtsInstanceId,
}

fn main() {}
