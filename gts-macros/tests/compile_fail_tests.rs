//! Compile-fail tests for `struct_to_gts_schema` macro validation.
//!
//! These tests verify that the macro produces appropriate compile errors
//! for invalid inputs.

#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
