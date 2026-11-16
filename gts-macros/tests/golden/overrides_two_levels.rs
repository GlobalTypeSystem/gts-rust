// An order-placed event uses UUID subject IDs, while the event envelope
// permits any non-empty subject ID and any GTS subject type. Order events
// restrict the subject type to orders. The GTS instance ID is independent.

use gts::{GtsInstanceId, GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;

const ORDER_TYPE: &str = gts_macros::gts_id!("x.core.orders.order.v1~");

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.ovr2.event.v1~"),
    description = "Event envelope",
    properties = "id,subject_id,subject_type,payload"
)]
#[derive(Debug)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    #[schemars(length(min = 1, max = 1024))]
    pub subject_id: String,
    pub subject_type: GtsTypeId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = gts_id!("x.test.ovr2.event.v1~x.test.order.placed.v1~"),
    description = "Order placed with a UUID subject ID",
    properties = "order_id",
    overrides(EventV1 = {
        #[schemars(length(min = 36, max = 36), extend("format" = "uuid"))]
        subject_id: String,
        #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
        subject_type: GtsTypeId,
    })
)]
#[derive(Debug)]
pub struct OrderPlacedV1 {
    pub order_id: String,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            EventV1::<()>::TYPE_ID.to_owned(),
            EventV1::<()>::gts_schema_with_refs(),
        ),
        (
            OrderPlacedV1::TYPE_ID.to_owned(),
            OrderPlacedV1::gts_schema_with_refs(),
        ),
    ]
}
