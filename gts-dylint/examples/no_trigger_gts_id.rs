// Should NOT trigger: gts_id! macro applies the configured prefix
use gts_macros::gts_id;

fn main() {
    let _id = gts_id!("x.core.events.topic.v1~");
}
