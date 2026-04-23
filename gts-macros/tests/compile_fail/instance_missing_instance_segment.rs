//! Test: Unknown attribute 'instance_segment' in gts_well_known_instance (no longer valid)

use gts_macros::gts_well_known_instance;

#[gts_well_known_instance(
    dir_path = "instances",
    instance_segment = "x.commerce._.orders.v1.0"
)]
fn get_instance_orders_v1() -> () {}

fn main() {}
