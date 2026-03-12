//! Test: gts_well_known_instance rejects static items (only fn items are supported)

use gts_macros::gts_well_known_instance;

#[gts_well_known_instance(
    dir_path = "instances",
    id = "gts.x.core.events.topic.v1~x.commerce._.orders.v1.0"
)]
static ORDERS_TOPIC: &str = r#"{"name": "orders"}"#;

fn main() {}
