// Golden case: an `Option<P>` generic slot on the root type. The base renders
// the slot as `["object", "null"]` and leaves it out of `required`; a leaf with
// a payload requires and narrows the slot; a body-less leaf (`properties = ""`)
// keeps it optional and nullable.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.optional.event.v1~"),
    description = "Base event whose payload may be absent",
    properties = "id,data"
)]
#[derive(Debug)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    pub data: Option<P>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = gts_id!("x.test.optional.event.v1~x.test.optional.placed.v1~"),
    description = "Leaf with a payload",
    properties = "order_id"
)]
#[derive(Debug)]
pub struct PlacedV1 {
    pub order_id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = gts_id!("x.test.optional.event.v1~x.test.optional.cancelled.v1~"),
    description = "Body-less leaf",
    properties = ""
)]
#[derive(Debug)]
pub struct CancelledV1;

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (EventV1::<()>::TYPE_ID.to_owned(), EventV1::<()>::gts_schema_with_refs()),
        (PlacedV1::TYPE_ID.to_owned(), PlacedV1::gts_schema_with_refs()),
        (CancelledV1::TYPE_ID.to_owned(), CancelledV1::gts_schema_with_refs()),
    ]
}
