//! Test: #[gts(instance_id)] on a Uuid field (must be GtsInstanceId)

use gts_macros::GtsSchema;
use uuid::Uuid;

#[derive(GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.app.entities.topic.v1~",
    description = "Topic"
)]
pub struct TopicV1 {
    #[gts(instance_id)]
    pub id: Uuid,
    pub name: String,
}

fn main() {}
