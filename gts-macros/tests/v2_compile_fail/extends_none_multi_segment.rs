//! Test: `extends = None` with multi-segment schema_id fails identically to the absent case.
//! `extends = None` is an explicit root marker and must reject non-root schema IDs.

use gts::GtsSchemaId;
use gts_macros::GtsSchema;
use schemars::JsonSchema;

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Should fail",
    extends = None
)]
pub struct AuditV1 {
    #[gts(type_field)]
    pub gts_type: GtsSchemaId,
    pub data: String,
}

fn main() {}
