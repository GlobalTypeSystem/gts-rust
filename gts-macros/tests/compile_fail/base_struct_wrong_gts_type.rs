//! Test: Base struct with wrong GTS Type field type should fail compilation

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition with wrong GTS Type",
    properties = "r#type,name,description"
)]
#[derive(Debug)]
pub struct TopicV1WrongGtsType<P> {
    pub r#type: String, // This should be GtsSchemaId, not String
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

fn main() {}
