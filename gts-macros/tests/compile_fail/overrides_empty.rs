//! An empty `overrides()` is rejected.

use gts::{GtsInstanceId, GtsTypeId};
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.app.ovr.event.v1~",
    description = "Event envelope",
    properties = "id,subject_id,subject_type,payload",
)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    pub subject_id: String,
    pub subject_type: GtsTypeId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.app.ovr.event.v1~x.app.ovr.order.v1~",
    description = "Order event",
    properties = "region,details",
)]
pub struct OrderEventV1<D> {
    pub region: String,
    pub details: D,
}

#[derive(schemars::JsonSchema)]
pub struct SubjectIdRules {
    pub subject_id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OrderEventV1,
    type_id = "gts.x.app.ovr.event.v1~x.app.ovr.order.v1~x.app.ovr.placed.v1~",
    description = "Order placed",
    properties = "",
    overrides(),
)]
pub struct OrderPlacedV1;

fn main() {}
