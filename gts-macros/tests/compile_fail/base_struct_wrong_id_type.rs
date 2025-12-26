//! Test: Base struct with wrong ID field type should fail compilation

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition with wrong ID type",
    properties = "id,name,description"
)]
#[derive(Debug)]
pub struct TopicV1WrongIdType<P> {
    pub id: String, // This should be GtsInstanceId, not String
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

fn main() {}
