//! Test: Multiple type generic parameters are prohibited (GTS schemas assume nested segments)

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.entities.base_event.v1~",
    description = "Base event with two payload types (invalid)",
    properties = "payload1,payload2"
)]
pub struct BaseEvent<P, T> {
    pub payload1: P,
    pub payload2: T,
}

fn main() {}
