//! Test: Nested structs must not derive Serialize/Deserialize via cfg_attr

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.base.v1~",
    description = "Base event type",
    properties = "id,payload"
)]
pub struct BaseEventV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[cfg_attr(all(), derive(serde::Serialize, serde::Deserialize))]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.core.events.base.v1~x.app.audit.event.v1~",
    description = "Audit event with user context",
    properties = "user_id"
)]
pub struct AuditEventV1 {
    pub user_id: String,
}

fn main() {}
