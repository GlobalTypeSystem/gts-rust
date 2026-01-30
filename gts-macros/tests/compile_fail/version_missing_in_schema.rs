//! Test: Struct has version suffix but schema_id doesn't have version
//! BaseEventV1 should not work with schema without version

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type~",
    description = "Base event type",
    properties = "id"
)]
pub struct BaseEventV1 {
    pub id: GtsInstanceId,
}

fn main() {}
