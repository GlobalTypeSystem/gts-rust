//! Test: Schema has version but struct name doesn't have version suffix
//! BaseEvent should not work with v1~ schema

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type",
    properties = "id"
)]
pub struct BaseEvent {
    pub id: GtsInstanceId,
}

fn main() {}
