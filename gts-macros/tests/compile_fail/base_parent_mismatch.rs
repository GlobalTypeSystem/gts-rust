//! Test: base = ParentStruct where parent's GTS_SCHEMA_ID doesn't match
//! the parent segment in schema_id should fail at compile time

use gts_macros::struct_to_gts_schema;

// Define a base type with one schema ID (must be generic)
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

// This should fail: parent schema_id doesn't match the parent segment
// Parent's ID is "gts.x.core.events.type.v1~" but schema_id's parent
// segment is "gts.x.wrong.parent.v1~"
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.wrong.parent.v1~x.core.audit.event.v1~",
    description = "This should fail",
    properties = "user_id"
)]
#[derive(Debug)]
pub struct AuditEventV1 {
    pub user_id: String,
}

fn main() {}
