//! Reference overrides use the same scope inclusion rules as derivation.
//! Invalid fixtures must fail their generated tests in the outer harness.

#![allow(dead_code)]

use gts::GtsSchema;
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

const ROOT: &str = "gts.x.test.ovr_refs.target.v1~";
const CHILD: &str = "gts.x.test.ovr_refs.target.v1~x.test.ovr_refs.child.v1~";

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_refs.holder.v1~",
    description = "Holder of a type reference",
    properties = "subject_type,target,payload"
)]
pub struct HolderV1<P> {
    #[serde(rename = "type")]
    pub subject_type: gts::GtsTypeId,
    #[schemars(extend("x-gts-type-ref" = ROOT))]
    pub target: String,
    pub payload: P,
}

#[derive(JsonSchema)]
struct NarrowRules {
    #[schemars(extend("x-gts-type-ref" = CHILD))]
    target: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = HolderV1,
    type_id = "gts.x.test.ovr_refs.holder.v1~x.test.ovr_refs.narrow.v1~",
    description = "Only references to the derived target",
    properties = "details",
    overrides(HolderV1 = NarrowRules)
)]
pub struct NarrowV1<D> {
    pub details: D,
}

#[derive(JsonSchema)]
struct WideRules {
    #[schemars(extend("x-gts-type-ref" = "gts.*"))]
    target: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = HolderV1,
    type_id = "gts.x.test.ovr_refs.holder.v1~x.test.ovr_refs.wide.v1~",
    description = "Invalid widening to every GTS type",
    properties = "",
    overrides(HolderV1 = WideRules)
)]
pub struct WideV1;

#[derive(JsonSchema)]
struct UnconstrainedRules {
    target: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = HolderV1,
    type_id = "gts.x.test.ovr_refs.holder.v1~x.test.ovr_refs.unconstrained.v1~",
    description = "Invalid removal of the reference constraint",
    properties = "",
    overrides(HolderV1 = UnconstrainedRules)
)]
pub struct UnconstrainedV1;

#[derive(JsonSchema)]
struct BaseRules {
    #[schemars(extend("x-gts-type-ref" = ROOT))]
    target: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = NarrowV1,
    type_id = "gts.x.test.ovr_refs.holder.v1~x.test.ovr_refs.narrow.v1~x.test.ovr_refs.back_to_base.v1~",
    description = "Invalid undoing of an intermediate narrowing",
    properties = "",
    overrides(HolderV1 = BaseRules)
)]
pub struct BackToBaseV1;

#[test]
fn narrower_type_reference_is_emitted_and_admitted() {
    let base = HolderV1::<()>::gts_schema_with_refs_allof();
    assert_eq!(base["properties"]["target"]["x-gts-type-ref"], ROOT);

    let narrow = NarrowV1::<()>::gts_schema_with_refs_allof();
    assert_eq!(
        narrow["allOf"][1]["properties"]["target"]["x-gts-type-ref"],
        CHILD
    );
    NarrowV1::<()>::validate_overrides().unwrap();
}

#[test]
fn broader_or_missing_type_reference_is_rejected() {
    for error in [
        WideV1::validate_overrides().unwrap_err(),
        UnconstrainedV1::validate_overrides().unwrap_err(),
    ] {
        assert_eq!(error.path, "/target");
        assert_eq!(error.ancestor, HolderV1::<()>::TYPE_ID);
        assert!(error.message.contains("widens"), "{error}");
        assert!(error.message.contains("x-gts-type-ref"), "{error}");
    }
}

#[test]
fn an_older_anchor_cannot_undo_an_intermediate_reference_narrowing() {
    let error = BackToBaseV1::validate_overrides().unwrap_err();
    assert_eq!(error.path, "/target");
    assert_eq!(error.ancestor, HolderV1::<()>::TYPE_ID);
    assert!(error.message.contains(CHILD), "{error}");
    assert!(error.message.contains("widens"), "{error}");
}
