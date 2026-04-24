//! Test: extends + Serialize fails. Direct serde on nested structs is unconditionally blocked.

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

#[derive(Debug, serde::Serialize, serde::Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Nested",
    extends = BaseEventV1
)]
pub struct AuditPayloadV1 {
    pub user_agent: String,
}

fn main() {}
