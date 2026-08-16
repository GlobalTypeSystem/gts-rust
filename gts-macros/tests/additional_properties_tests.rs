//! Behavioural assertions for the `additionalProperties` content model the
//! macro emits, one pointer per level under test.
//!
//! These share the `additional_properties_*` fixtures with `golden_tests.rs`,
//! which snapshot-matches whole documents. The snapshot cannot express *which*
//! value is correct — `GTS_GOLDEN=overwrite` blesses whatever the macro emits —
//! so the rule each fixture exists to prove is stated here instead. The
//! fixtures are `include!`d rather than imported because every file under
//! `tests/` is its own crate root, so a module declared in `golden_tests.rs` is
//! not reachable from here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

/// Declares the fixture modules this file asserts over.
macro_rules! fixtures {
    ($($name:ident),+ $(,)?) => {
        $(
            mod $name {
                include!(concat!("golden/", stringify!($name), ".rs"));
            }
        )+
    };
}

fixtures!(
    additional_properties_nested_closed,
    additional_properties_explicit_open,
    additional_properties_flattened_map,
    additional_properties_content_models,
    additional_properties_gts_root_open,
    additional_properties_gts_derived_open,
);

/// The `additionalProperties` value at `pointer` in the schema generated for
/// `type_id`, or `"<absent>"` when the level states no content model.
///
/// `"<absent>"` is a distinct expectation, not a fallback: on a combinator
/// branch or on a derived type's `allOf` wrapper, both `false` and `true` would
/// be wrong and stating nothing is the correct output.
#[track_caller]
fn at(schemas: &[(String, serde_json::Value)], type_id: &str, pointer: &str) -> String {
    let (_, schema) = schemas
        .iter()
        .find(|(id, _)| id == type_id)
        .unwrap_or_else(|| panic!("case does not generate '{type_id}'"));
    schema
        .pointer(pointer)
        .map_or_else(|| "<absent>".to_owned(), ToString::to_string)
}

/// Ordinary nested structs are closed at every level, including the Draft-07
/// `definitions` the nested types land in.
#[test]
fn nested_structs_are_closed_at_every_level() {
    let nested = additional_properties_nested_closed::schemas();
    let nested_id = "gts.x.test.golden.nestedclosed.v1~";
    for pointer in [
        "/additionalProperties",
        "/definitions/Profile/additionalProperties",
        "/definitions/Contact/additionalProperties",
    ] {
        assert_eq!(at(&nested, nested_id, pointer), "false", "{pointer}");
    }
}

/// An explicitly open nested struct keeps its own model while the GTS root stays
/// closed - the opt-out applies where it is declared, not upward.
#[test]
fn an_explicit_open_level_does_not_open_the_root() {
    let open = additional_properties_explicit_open::schemas();
    let open_id = "gts.x.test.golden.explicitopen.v1~";
    assert_eq!(at(&open, open_id, "/additionalProperties"), "false");
    assert_eq!(
        at(
            &open,
            open_id,
            "/definitions/ExtensionPoint/additionalProperties"
        ),
        "true"
    );
}

/// A flattened map is the same thing spelled through Serde.
#[test]
fn a_flattened_map_level_stays_open() {
    let map = additional_properties_flattened_map::schemas();
    let map_id = "gts.x.test.golden.flattenedmap.v1~";
    assert_eq!(at(&map, map_id, "/additionalProperties"), "false");
    assert_eq!(
        at(
            &map,
            map_id,
            "/definitions/ExtensibleMetadata/additionalProperties"
        ),
        "true"
    );
}

/// A schema-valued map level keeps its value constraint, and combinator branches
/// are left alone - closing them would reject instances a sibling branch accepts.
#[test]
fn map_value_constraints_and_combinator_branches_are_preserved() {
    let models = additional_properties_content_models::schemas();
    let models_id = "gts.x.test.golden.contentmodels.v1~";
    assert_eq!(at(&models, models_id, "/additionalProperties"), "false");
    assert_eq!(
        at(
            &models,
            models_id,
            "/properties/labels/additionalProperties"
        ),
        r#"{"type":"string"}"#
    );
    for branch in ["0", "1"] {
        assert_eq!(
            at(
                &models,
                models_id,
                &format!("/definitions/Choice/anyOf/{branch}/additionalProperties"),
            ),
            "<absent>",
        );
    }
}

/// On a GTS base type the explicit open model applies to the root itself.
#[test]
fn a_gts_base_type_can_declare_its_root_open() {
    assert_eq!(
        at(
            &additional_properties_gts_root_open::schemas(),
            "gts.x.test.golden.rootopen.v1~",
            "/additionalProperties",
        ),
        "true"
    );
}

/// On a derived type it applies to the level carrying that type's own fields,
/// not to the `allOf` wrapper - a model on the wrapper would fight the parent's.
#[test]
fn a_derived_type_opens_its_own_field_level_not_the_wrapper() {
    let derived = additional_properties_gts_derived_open::schemas();
    let derived_id = "gts.x.test.golden.openchain.v1~x.test.audit.payload.v1~";
    assert_eq!(
        at(&derived, derived_id, "/additionalProperties"),
        "<absent>"
    );
    assert_eq!(
        at(
            &derived,
            derived_id,
            "/allOf/1/properties/payload/additionalProperties"
        ),
        "true"
    );
}
