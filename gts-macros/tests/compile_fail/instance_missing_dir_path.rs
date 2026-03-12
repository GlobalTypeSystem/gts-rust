//! Test: Missing required attribute dir_path in gts_well_known_instance

use gts_macros::gts_well_known_instance;

#[gts_well_known_instance(
    id = "gts.x.core.events.topic.v1~x.commerce._.orders.v1.0"
)]
fn get_instance_orders_v1() -> () {}

fn main() {}
