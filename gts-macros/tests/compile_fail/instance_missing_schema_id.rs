//! Test: Missing required attribute id in gts_well_known_instance

use gts_macros::gts_well_known_instance;

#[gts_well_known_instance(
    dir_path = "instances"
)]
fn get_instance_orders_v1() -> () {}

fn main() {}
