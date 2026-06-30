// Should NOT trigger: gts_instance_raw! with gts_id! for id field
use gts_macros::{gts_id, gts_instance_raw};

fn main() {
    let _ = gts_instance_raw!({
        "id": gts_id!("x.core.events.type.v1~demo.app.events.test.v1"),
        "name": "test"
    });
}
