//! Test: extends = Parent where parent's SCHEMA_ID doesn't match
//! the parent segment in schema_id should fail at compile time

use gts::{GtsSchema, GtsSchemaId};
use gts_macros::GtsSchema;
use schemars::JsonSchema;

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type"
)]
pub struct BaseEventV1<P: GtsSchema + JsonSchema> {
    #[gts(type_field)]
    pub event_type: GtsSchemaId,
    pub id: String,
    pub payload: P,
}

// This should fail: parent schema_id doesn't match the parent segment
// Parent's ID is "gts.x.core.events.type.v1~" but schema_id's parent
// segment is "gts.x.wrong.parent.type.v1~"
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.wrong.parent.type.v1~x.core.audit.event.v1~",
    description = "This should fail",
    extends = BaseEventV1
)]
pub struct AuditEventV1 {
    pub user_id: String,
}

fn main() {}
