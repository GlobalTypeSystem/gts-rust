// Golden case: three levels. The leaf replaces the root `id` (declared by L1)
// with a 36-char UUID and, at the nested generic path `/payload`, the
// `region` declared by L2 with an enum of EU regions. Each replacement is
// emitted as the helper declares it; nothing inherited is merged in.

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

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = gts_id!("x.test.ovr3.subject.v1~x.test.ovr3.regional.v1~"),
    description = "Subject bound to a region",
    properties = "region,details"
)]
#[derive(Debug)]
pub struct RegionalSubjectV1<D> {
    #[schemars(length(max = 32))]
    pub region: String,
    pub details: D,
}

/// `/id` is a canonical UUID.
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

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (SubjectV1::<()>::TYPE_ID.to_owned(), SubjectV1::<()>::gts_schema_with_refs()),
        (
            RegionalSubjectV1::<()>::TYPE_ID.to_owned(),
            RegionalSubjectV1::<()>::gts_schema_with_refs(),
        ),
        (EuTenantSubjectV1::TYPE_ID.to_owned(), EuTenantSubjectV1::gts_schema_with_refs()),
    ]
}
