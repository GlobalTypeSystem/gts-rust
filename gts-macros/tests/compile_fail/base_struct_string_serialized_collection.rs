use gts_macros::struct_to_gts_schema;
use serde::Serializer;

fn serialize_tags<S: Serializer>(tags: &[String], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&tags.join(","))
}

#[struct_to_gts_schema(
    dir_path = "schemas", base = true, type_id = "gts.x.test.identity.tags.v1~",
    description = "A collection serialized as a string", properties = "id,tags"
)]
pub struct TagsV1 {
    pub id: gts::GtsInstanceId,
    #[serde(rename = "gtsId", serialize_with = "serialize_tags")]
    pub tags: Vec<String>,
}

fn main() {}
