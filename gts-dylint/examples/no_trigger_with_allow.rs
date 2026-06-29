// The allow attribute suppresses the lint on specific items.
#[allow(unknown_lints, gts_id_hardcoded_prefix)]
pub const MY_PREFIX: &str = "gts.";

fn main() {
    let _ = MY_PREFIX;
}
