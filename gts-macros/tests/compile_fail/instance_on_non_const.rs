//! Test: gts_well_known_instance applied to a non-const item (static)

use gts_macros::gts_well_known_instance;

#[gts_well_known_instance(
    dir_path = "instances",
    schema_id = "gts.x.core.events.topic.v1~",
    instance_segment = "x.commerce._.orders.v1.0"
)]
static ORDERS_TOPIC: &str = r#"{"name": "orders"}"#;

fn main() {}
