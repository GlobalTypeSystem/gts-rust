//! A descendant of the overriding type is not a valid ancestor anchor.

use gts::{GtsInstanceId, GtsTypeId};
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.ovr.event.v1~",
    description = "Event envelope",
    properties = "id,subject_id,subject_type,payload"
)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    pub subject_id: String,
    pub subject_type: GtsTypeId,
    pub payload: P,
}

#[derive(schemars::JsonSchema)]
pub struct OrderIdRules {
    pub order_id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.app.ovr.event.v1~x.app.ovr.order.v1~",
    description = "Incorrectly overrides its descendant",
    properties = "region,details",
    overrides(OrderPlacedV1 = OrderIdRules),
)]
pub struct OrderEventV1<D> {
    pub region: String,
    pub details: D,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OrderEventV1,
    type_id = "gts.x.app.ovr.event.v1~x.app.ovr.order.v1~x.app.ovr.placed.v1~",
    description = "Actual descendant of OrderEventV1",
    properties = "order_id,extra",
)]
pub struct OrderPlacedV1<E> {
    pub order_id: String,
    pub extra: E,
}

fn main() {}
