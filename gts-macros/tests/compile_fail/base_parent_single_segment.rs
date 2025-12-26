//! Test: base = ParentStruct with single-segment schema_id should fail
//! A child type must have at least 2 segments

use gts_macros::struct_to_gts_schema;

// Define a valid base type first (must be generic)
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct BaseEventV1<P> {
    pub id: String,
    pub payload: P,
}

// This should fail: base = ParentStruct but schema_id has only 1 segment
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.core.audit.event.v1~",
    description = "This should fail",
    properties = "user_id"
)]
#[derive(Debug)]
pub struct AuditEventV1 {
    pub user_id: String,
}

fn main() {}
