//! Test: `gts_instance_raw!` rejects a top-level `"id"` whose value is
//! not a string literal. The macro needs to validate the GTS id shape
//! at compile time, so a runtime expression can't be used.

use gts_macros::gts_instance_raw;

fn main() {
    let runtime_id = "gts.acme.core.events.topic.v1~vendor.app.x.v1".to_owned();
    let _ = gts_instance_raw!({
        "id": runtime_id,
        "name": "audit",
    });
}
