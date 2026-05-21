//! Verifies that the deprecated `schema_id` macro attribute is still accepted
//! as an alias for `type_id`. New code should use `type_id`; this test
//! pins backward compatibility for projects still using the old keyword.
//!
//! `#[allow(deprecated)]` silences the deprecation warning the macro emits at
//! the call site so the test compiles cleanly under `-D warnings`.

#![allow(deprecated, clippy::unwrap_used)]

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;

#[derive(Debug, Clone)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.test.deprecated.alias.v1~",
    description = "Verifies the deprecated `schema_id` alias still parses",
    properties = "id"
)]
pub struct DeprecatedAliasV1 {
    pub id: GtsInstanceId,
}

#[test]
fn deprecated_schema_id_alias_still_works() {
    assert_eq!(
        DeprecatedAliasV1::gts_schema_id().as_ref(),
        "gts.x.test.deprecated.alias.v1~"
    );
    assert_eq!(
        <DeprecatedAliasV1 as GtsSchema>::SCHEMA_ID,
        "gts.x.test.deprecated.alias.v1~"
    );
}
