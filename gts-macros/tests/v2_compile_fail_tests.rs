//! Compile-fail tests for `#[derive(GtsSchema)]` macro validation.

#[test]
fn v2_compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/v2_compile_fail/*.rs");
}
