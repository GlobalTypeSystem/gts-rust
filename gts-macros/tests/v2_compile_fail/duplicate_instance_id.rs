//! Test: Two fields with #[gts(instance_id)]

use gts::GtsInstanceId;
use gts_macros::GtsSchema;
use schemars::JsonSchema;

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.topic.v1~",
    description = "Topic"
)]
pub struct TopicV1 {
    #[gts(instance_id)]
    pub id: GtsInstanceId,
    #[gts(instance_id)]
    pub alt_id: GtsInstanceId,
}

fn main() {}
