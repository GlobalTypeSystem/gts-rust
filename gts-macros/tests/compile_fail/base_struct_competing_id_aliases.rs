use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.renamed.id.v1~",
    description = "Two fields competing to identify an instance",
    properties = "instance,external"
)]
pub struct ConflictingV1 {
    #[serde(rename = "id")]
    pub instance: gts::GtsInstanceId,
    #[serde(rename = "$id")]
    pub external: String,
}

fn main() {}
