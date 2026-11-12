// Golden case: three levels. L2 narrows the root `id` (declared by L1)
// from 1..=1024 to 1..=64 chars. L3 narrows it again to a 36-char UUID and,
// at the nested generic path `/payload`, replaces L2's `region` with an
// enum of EU regions. Each replacement is checked against every inherited
// declaration, including L2's override of L1's field.

use gts::GtsSchema;
use gts::gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.ovr3.subject.v1~"),
    description = "Subject",
    properties = "subject_type,id,payload"
)]
#[derive(Debug)]
pub struct SubjectV1<P> {
    #[serde(rename = "type")]
    pub subject_type: GtsTypeId,
    #[schemars(length(min = 1, max = 1024))]
    pub id: String,
    pub payload: P,
}

/// `/id` is at most 64 chars, narrowing the root's 1024-char limit.
#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct RegionalIdOverride {
    #[schemars(length(min = 1, max = 64))]
    pub id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = gts_id!("x.test.ovr3.subject.v1~x.test.ovr3.regional.v1~"),
    description = "Subject bound to a region",
    properties = "region,details",
    overrides(SubjectV1 = RegionalIdOverride)
)]
#[derive(Debug)]
pub struct RegionalSubjectV1<D> {
    #[schemars(length(max = 32))]
    pub region: String,
    pub details: D,
}

/// `/id` is a canonical UUID. The inclusion checker compares length bounds
/// explicitly; it does not infer the UUID's 36-char length from `format`.
#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct TenantIdOverride {
    #[schemars(length(min = 36, max = 36), extend("format" = "uuid"))]
    pub id: String,
}

#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub enum EuRegion {
    #[serde(rename = "eu-west")]
    EuWest,
    #[serde(rename = "eu-central")]
    EuCentral,
}

/// `/payload/region` is an EU region. The finite enum proves inclusion
/// in the inherited `maxLength` without restating it.
#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct EuRegionOverride {
    pub region: EuRegion,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = gts_id!("x.test.ovr3.subject.v1~x.test.ovr3.regional.v1~x.test.ovr3.eu_tenant.v1~"),
    description = "EU tenant subject",
    properties = "tenant",
    overrides(SubjectV1 = TenantIdOverride, RegionalSubjectV1 = EuRegionOverride)
)]
#[derive(Debug)]
pub struct EuTenantSubjectV1 {
    pub tenant: String,
}

#[test]
fn leaf_override_cannot_widen_intermediate_id() {
    use gts::schema_overrides::{AncestorOverride, validate_ancestor_overrides};

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct WideIdOverride {
        #[schemars(length(min = 1, max = 128))]
        id: String,
    }

    let overrides = [AncestorOverride::of::<SubjectV1<()>, WideIdOverride>()];
    validate_ancestor_overrides(
        &SubjectV1::<()>::gts_resolved_schema(),
        EuTenantSubjectV1::TYPE_ID,
        &overrides,
    )
    .expect("128 chars are within the root's limit of 1024");

    let error = validate_ancestor_overrides(
        &RegionalSubjectV1::<()>::gts_resolved_schema(),
        EuTenantSubjectV1::TYPE_ID,
        &overrides,
    )
    .expect_err("a root-anchored override must respect the intermediate limit of 64");
    assert_eq!(error.path, "/id");
    assert!(error.message.contains("widens the inherited declaration"));
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            SubjectV1::<()>::TYPE_ID.to_owned(),
            SubjectV1::<()>::gts_schema_with_refs(),
        ),
        (
            RegionalSubjectV1::<()>::TYPE_ID.to_owned(),
            RegionalSubjectV1::<()>::gts_schema_with_refs(),
        ),
        (
            EuTenantSubjectV1::TYPE_ID.to_owned(),
            EuTenantSubjectV1::gts_schema_with_refs(),
        ),
    ]
}
