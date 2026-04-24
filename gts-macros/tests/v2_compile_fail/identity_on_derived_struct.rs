//! Test: derived struct (extends = Parent) must not carry an identity annotation.

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
    pub payload: P,
}

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event with user context",
    extends = BaseEventV1
)]
pub struct AuditPayloadV1 {
    #[gts(type_field)]
    pub event_type: GtsSchemaId,
    pub user_agent: String,
}

fn main() {}
