//! Test: `gts_id!` macro rejects a suffix that produces an invalid GTS ID
//! (too few tokens in the segment).

use gts_macros::gts_id;

fn main() {
    let _ = gts_id!("x.foo");
}
