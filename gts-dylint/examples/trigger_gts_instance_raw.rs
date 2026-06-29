// Should trigger: hardcoded "gts." prefix inside gts_instance_raw! id field
use gts_macros::gts_instance_raw;

fn main() {
    let _ = gts_instance_raw!({
        "id": "gts.x.core.events.type.v1~demo.app.events.test.v1",
        "name": "test"
    });
}
