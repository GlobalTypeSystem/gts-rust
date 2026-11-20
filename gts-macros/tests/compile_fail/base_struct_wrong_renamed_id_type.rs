use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.renamed.id.v1~",
    description = "Base with a numeric ID",
    properties = "local"
)]
pub struct RenamedIdV1 {
    #[serde(rename = "$id")]
    pub local: u32,
}

fn main() {}
