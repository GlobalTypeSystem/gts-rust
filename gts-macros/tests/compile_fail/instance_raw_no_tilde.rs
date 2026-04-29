//! Test: `gts_instance_raw!` rejects an `id` without `~` (i.e. a
//! single-segment id, which by spec denotes neither a schema nor an
//! instance — instance ids must be chained: `<type>~<segment>`).

use gts_macros::gts_instance_raw;

fn main() {
    // Single, format-valid GTS segment — passes the gts-id format
    // validator but fails the `~`-rule check.
    let _ = gts_instance_raw!({
        "id": "gts.acme.core.events.topic.v1",
        "name": "x",
    });
}
