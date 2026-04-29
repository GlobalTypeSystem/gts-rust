//! Test: `gts_instance_raw!` rejects a body that has no top-level `"id"`
//! key. The macro requires the validated id to be present in the JSON
//! object literal.

use gts_macros::gts_instance_raw;

fn main() {
    let _ = gts_instance_raw!({
        "name": "audit",
        "description": "missing id",
    });
}
