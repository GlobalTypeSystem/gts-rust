//! Test: extends + Serialize via cfg_attr fails. Direct serde on nested structs is unconditionally blocked.

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

#[cfg_attr(all(), derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event with user context",
    extends = BaseEventV1
)]
pub struct AuditEventV1 {
    pub user_id: String,
}

fn main() {}
