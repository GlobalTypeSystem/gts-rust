//! Test: Nested structs should NOT be directly serializable
//! This test verifies Issue #24 - nested structs can only be serialized through their base struct

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

// Base type with generic payload field
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.base.v1~",
    description = "Base event type",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct BaseEventV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

// Nested type that extends BaseEventV1
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.core.events.base.v1~x.app.audit.event.v1~",
    description = "Audit event with user context",
    properties = "user_id"
)]
#[derive(Debug)]
pub struct AuditEventV1 {
    pub user_id: String,
}

fn main() {
    // This should NOT compile: direct serialization of nested struct
    let nested = AuditEventV1 {
        user_id: "user1".to_string(),
    };
    let _ = serde_json::to_value(&nested);
}
