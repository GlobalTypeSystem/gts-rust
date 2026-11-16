// L4 overrides one field per ancestor: `/priority` (L1), `/payload/region`
// (L2), and `/payload/details/order_id` (L3). Generated tests check every
// inherited declaration, including L3's earlier region override.

use gts::{GtsInstanceId, GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;

use schemars::JsonSchema;

const ORDER_TYPE: &str = gts_macros::gts_id!("x.core.orders.order.v1~");

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.ovr4.event.v1~"),
    description = "Event envelope",
    properties = "id,subject_id,subject_type,priority,payload"
)]
#[derive(Debug)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    #[schemars(length(min = 1, max = 1024))]
    pub subject_id: String,
    pub subject_type: GtsTypeId,
    #[schemars(range(max = 100))]
    pub priority: u32,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = gts_id!("x.test.ovr4.event.v1~x.test.ovr4.order.v1~"),
    description = "Order event with a fulfillment region",
    properties = "region,details",
    overrides(EventV1 = {
        #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
        subject_type: GtsTypeId,
    })
)]
#[derive(Debug)]
pub struct OrderEventV1<D> {
    #[schemars(length(max = 32))]
    pub region: String,
    pub details: D,
}

#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub enum EuRegion {
    #[serde(rename = "eu-west")]
    EuWest,
    #[serde(rename = "eu-central")]
    EuCentral,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OrderEventV1,
    type_id = gts_id!("x.test.ovr4.event.v1~x.test.ovr4.order.v1~x.test.ovr4.placed.v1~"),
    description = "Order placed in the EU",
    properties = "order_id,delivery",
    overrides(
        EventV1 = {
            #[schemars(length(min = 36, max = 36), extend("format" = "uuid"))]
            subject_id: String,
            #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
            subject_type: GtsTypeId,
        },
        OrderEventV1 = { region: EuRegion },
    )
)]
#[derive(Debug)]
pub struct OrderPlacedV1<E> {
    #[schemars(length(max = 64))]
    pub order_id: String,
    pub delivery: E,
}

#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub enum EuWestRegion {
    #[serde(rename = "eu-west")]
    EuWest,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OrderPlacedV1,
    type_id = gts_id!("x.test.ovr4.event.v1~x.test.ovr4.order.v1~x.test.ovr4.placed.v1~x.test.ovr4.express.v1~"),
    description = "Express order placed in EU-west",
    properties = "estimated_days",
    overrides(
        EventV1 = {
            #[schemars(range(min = 50, max = 100))]
            priority: u32,
        },
        OrderEventV1 = { region: EuWestRegion },
        OrderPlacedV1 = {
            #[schemars(length(max = 16))]
            order_id: String,
        },
    )
)]
#[derive(Debug)]
pub struct ExpressOrderPlacedV1 {
    pub estimated_days: u32,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            EventV1::<()>::TYPE_ID.to_owned(),
            EventV1::<()>::gts_schema_with_refs(),
        ),
        (
            OrderEventV1::<()>::TYPE_ID.to_owned(),
            OrderEventV1::<()>::gts_schema_with_refs(),
        ),
        (
            OrderPlacedV1::<()>::TYPE_ID.to_owned(),
            OrderPlacedV1::<()>::gts_schema_with_refs(),
        ),
        (
            ExpressOrderPlacedV1::TYPE_ID.to_owned(),
            ExpressOrderPlacedV1::gts_schema_with_refs(),
        ),
    ]
}
