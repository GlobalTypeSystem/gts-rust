//! Test: Missing required attribute dir_path in gts_well_known_instance

use gts_macros::gts_well_known_instance;

#[gts_well_known_instance(
    schema_id = "gts.x.core.events.topic.v1~",
    instance_segment = "x.commerce._.orders.v1.0"
)]
const ORDERS_TOPIC: &str = r#"{"name": "orders"}"#;

fn main() {}
