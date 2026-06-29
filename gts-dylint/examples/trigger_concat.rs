// Should trigger: concat! producing a hardcoded "gts." prefix
fn main() {
    const GTS_PREFIX: &str = concat!("gts.", "");
    let _ = GTS_PREFIX;
    let _x = concat!("gts.", "x");
}
