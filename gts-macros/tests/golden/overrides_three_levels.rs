// L3 narrows `/subject_id` to a UUID and `/payload/region` to EU regions.
// Validation must also respect L2's earlier narrowing of L1's subject ID.

use gts::{GtsInstanceId, GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;

use schemars::JsonSchema;

const ORDER_TYPE: &str = gts_macros::gts_id!("x.core.orders.order.v1~");

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.ovr3.event.v1~"),
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
    type_id = gts_id!("x.test.ovr3.event.v1~x.test.ovr3.order.v1~"),
    description = "Order event with a fulfillment region",
    properties = "region,details",
    overrides(EventV1 = {
        #[schemars(length(min = 1, max = 64))]
        subject_id: String,
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

/// Shared by order lifecycle events whose subject is an order with a UUID ID.
#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct UuidOrderSubject {
    #[schemars(length(min = 36, max = 36), extend("format" = "uuid"))]
    pub subject_id: String,
    #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
    pub subject_type: GtsTypeId,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OrderEventV1,
    type_id = gts_id!("x.test.ovr3.event.v1~x.test.ovr3.order.v1~x.test.ovr3.placed.v1~"),
    description = "Order placed in the EU",
    properties = "order_id",
    overrides(
        EventV1 = UuidOrderSubject,
        OrderEventV1 = { region: EuRegion },
    )
)]
#[derive(Debug)]
pub struct OrderPlacedV1 {
    pub order_id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OrderEventV1,
    type_id = gts_id!("x.test.ovr3.event.v1~x.test.ovr3.order.v1~x.test.ovr3.cancelled.v1~"),
    description = "Order cancelled",
    properties = "reason",
    overrides(EventV1 = UuidOrderSubject)
)]
#[derive(Debug)]
pub struct OrderCancelledV1 {
    pub reason: String,
}

#[test]
fn leaf_override_cannot_widen_intermediate_subject_id() {
    use gts::schema_overrides::{AncestorOverride, validate_ancestor_overrides};

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct WideSubjectIdOverride {
        #[schemars(length(min = 1, max = 128))]
        subject_id: String,
    }

    let overrides = [AncestorOverride::of::<EventV1<()>, WideSubjectIdOverride>()];
    validate_ancestor_overrides(
        &EventV1::<()>::gts_resolved_schema(),
        OrderPlacedV1::TYPE_ID,
        &overrides,
    )
    .expect("128 chars are within the root's limit of 1024");

    let error = validate_ancestor_overrides(
        &OrderEventV1::<()>::gts_resolved_schema(),
        OrderPlacedV1::TYPE_ID,
        &overrides,
    )
    .expect_err("a root-anchored override must respect the intermediate limit of 64");
    assert_eq!(error.path, "/subject_id");
    assert!(error.message.contains("widens the inherited declaration"));
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
            OrderPlacedV1::TYPE_ID.to_owned(),
            OrderPlacedV1::gts_schema_with_refs(),
        ),
        (
            OrderCancelledV1::TYPE_ID.to_owned(),
            OrderCancelledV1::gts_schema_with_refs(),
        ),
    ]
}
