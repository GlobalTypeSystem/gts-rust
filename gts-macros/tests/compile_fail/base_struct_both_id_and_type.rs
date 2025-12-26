//! Test: Base struct with both ID and GTS Type fields should fail compilation

use gts_macros::struct_to_gts_schema;
use gts::gts::{GtsInstanceId, GtsSchemaId};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base topic type definition with both ID and GTS Type - should fail",
    properties = "id,r#type,name,description"
)]
#[derive(Debug)]
pub struct TopicV1BothIdAndTypeV1<P> {
    pub id: GtsInstanceId,        // ID field
    pub r#type: GtsSchemaId,      // GTS Type field - this should cause failure
    pub name: String,
    pub description: Option<String>,
    pub config: P,
}

fn main() {}
