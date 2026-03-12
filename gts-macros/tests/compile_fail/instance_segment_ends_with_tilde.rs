//! Test: id ends with ~ in gts_well_known_instance (schema/type, not instance)

use gts_macros::gts_well_known_instance;

#[gts_well_known_instance(
    dir_path = "instances",
    id = "gts.x.core.events.topic.v1~"
)]
fn get_instance_orders_v1() -> () {}

fn main() {}
