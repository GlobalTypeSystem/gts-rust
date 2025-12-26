//! Test: Using a non-GTS struct as generic argument should fail
//!
//! This tests that only types with struct_to_gts_schema applied (or ())
//! can be used as generic parameters in GTS structs.

use gts_macros::struct_to_gts_schema;

// Define a GTS base struct with generic parameter
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

// This is a regular struct that does NOT have struct_to_gts_schema applied
#[derive(Debug)]
pub struct MyStruct {
    pub some_id: String,
}

fn main() {
    // This should fail: MyStruct does not implement GtsSchema
    // Only types with struct_to_gts_schema applied (or ()) can be used
    let _event: BaseEventV1<MyStruct> = BaseEventV1 {
        id: "test".to_string(),
        payload: MyStruct {
            some_id: "123".to_string(),
        },
    };
}
