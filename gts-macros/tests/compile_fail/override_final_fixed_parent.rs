use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.fixed.v1~",
    description = "Final fixed-shape base",
    properties = "id,name",
    gts_final = true,
)]
pub struct FinalFixedV1 {
    pub id: GtsInstanceId,
    pub name: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = FinalFixedV1,
    type_id = "gts.x.app.entities.fixed.v1~x.app.entities.short.v1~",
    description = "Forbidden override on a final base",
    properties = "",
    overrides(FinalFixedV1 = { #[schemars(length(max = 8))] name: String })
)]
pub struct ShortV1;

fn main() {}
