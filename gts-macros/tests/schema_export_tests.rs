// gts:ignore - holds function-local and bounded types that cannot be exported on purpose.
//! The `gts_schema_export_<Type>` tests the macro emits for `gts generate-from-rust`
//! must compile cleanly wherever the macro is accepted, and do nothing on their own.

#![deny(warnings, unreachable_pub, missing_docs)]

use gts::GtsSchema;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.export.event.v1~",
    description = "Event",
    properties = "id,payload"
)]
/// Base event.
#[derive(Debug)]
pub struct EventV1<P> {
    /// Instance ID.
    pub id: gts::GtsInstanceId,
    /// Extension slot.
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.export.event.v1~x.test.export.placed.v1~",
    description = "Placed",
    properties = "order_id"
)]
/// Derived event.
#[derive(Debug)]
pub struct PlacedV1 {
    /// Order ID.
    pub order_id: String,
}

/// A bound `()` does not implement.
pub trait Marker {}

/// `Bounded<()>` would not satisfy `P: Marker`, so no export test is emitted.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.export.bounded.v1~",
    description = "Bounded",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct BoundedV1<P: Marker> {
    /// Instance ID.
    pub id: gts::GtsInstanceId,
    /// Extension slot.
    pub payload: P,
}

/// A default is not a bound: `DefaultedV1<()>` still gets an export test.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.export.defaulted.v1~",
    description = "Defaulted",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct DefaultedV1<P = ()> {
    /// Instance ID.
    pub id: gts::GtsInstanceId,
    /// Extension slot.
    pub payload: P,
}

#[test]
fn export_tests_are_noops_without_cli_env() {
    // No `GTS_SCHEMA_EXPORT_DIR` in a plain test run: nothing is validated or written.
    use gts::schema_export::Exporter;
    assert_eq!(
        <__gts_schema_export_EventV1::Marker as Exporter>::export(),
        Ok(())
    );
    assert_eq!(
        <__gts_schema_export_PlacedV1::Marker as Exporter>::export(),
        Ok(())
    );
    assert_eq!(
        <__gts_schema_export_DefaultedV1::Marker as Exporter>::export(),
        Ok(())
    );
    assert_eq!(
        PlacedV1::TYPE_ID,
        "gts.x.test.export.event.v1~x.test.export.placed.v1~"
    );
}

#[test]
fn function_local_types_compile_under_deny_warnings() {
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = "gts.x.test.export.local.v1~",
        description = "Local",
        properties = "id"
    )]
    #[derive(Debug)]
    struct LocalV1 {
        id: gts::GtsInstanceId,
    }
    assert_eq!(LocalV1::TYPE_ID, "gts.x.test.export.local.v1~");
}

/// The emitted export code must not depend on the caller's std names.
pub mod shadowed_std_names {
    #![allow(dead_code, unused_macros)]
    use gts_macros::struct_to_gts_schema;

    type Result<T> = ::core::result::Result<T, ()>;
    struct Err;
    macro_rules! panic {
        ($($t:tt)*) => {};
    }
    macro_rules! file {
        () => {
            0
        };
    }

    /// Declared next to the shadowing names.
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = "gts.x.test.export.shadowed.v1~",
        description = "Shadowed",
        properties = "id"
    )]
    #[derive(Debug)]
    pub struct ShadowedV1 {
        /// Instance ID.
        pub id: gts::GtsInstanceId,
    }
}
