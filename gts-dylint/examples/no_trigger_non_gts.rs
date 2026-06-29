// Should NOT trigger: string does not start with "gts." or "gts://"
fn main() {
    let _s = "not a gts id";
    let _other = "gts_config.json";
}
