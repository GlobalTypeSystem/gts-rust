//! Test: `gts_id!` macro rejects invalid GTS ID patterns at compile time.

use gts_macros::gts_id;

fn main() {
    let _ = gts_id!("invalid..bad");
}
