// Golden case: four levels. L3 replaces `/id` (L1) with a UUID and
// `/payload/region` (L2) with an enum of EU regions. L4 narrows further,
// one field per ancestor level: `/level` (L1) to 50..=100 (restating L1's
// maximum), `/payload/region` (L2) to `eu-west`, and
// `/payload/details/tenant` (L3's own field) from 64 to 16 chars. Each
// replacement is checked by its generated test against every inherited declaration
// of its field, L3's overrides included.

use gts::GtsSchema;
use gts::gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.ovr4.subject.v1~"),
    description = "Subject",
    properties = "subject_type,id,level,payload"
)]
#[derive(Debug)]
pub struct SubjectV1<P> {
    #[serde(rename = "type")]
    pub subject_type: GtsTypeId,
    #[schemars(length(min = 1, max = 1024))]
    pub id: String,
    #[schemars(range(max = 100))]
    pub level: u32,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = gts_id!("x.test.ovr4.subject.v1~x.test.ovr4.regional.v1~"),
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
    type_id = gts_id!("x.test.ovr4.subject.v1~x.test.ovr4.regional.v1~x.test.ovr4.eu_tenant.v1~"),
    description = "EU tenant subject",
    properties = "tenant,plan",
    overrides(SubjectV1 = TenantIdOverride, RegionalSubjectV1 = EuRegionOverride)
)]
#[derive(Debug)]
pub struct EuTenantSubjectV1<E> {
    #[schemars(length(max = 64))]
    pub tenant: String,
    pub plan: E,
}

/// `/level` is at least 50.
#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct PremiumLevelOverride {
    #[schemars(range(min = 50, max = 100))]
    pub level: u32,
}

#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub enum EuWestRegion {
    #[serde(rename = "eu-west")]
    EuWest,
}

/// `/payload/region` is exactly `eu-west`.
#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct EuWestOverride {
    pub region: EuWestRegion,
}

/// `/payload/details/tenant` (L3's own field) is at most 16 chars.
#[derive(JsonSchema)]
#[allow(dead_code)] // schema-only helper, never constructed
pub struct ShortTenantOverride {
    #[schemars(length(max = 16))]
    pub tenant: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EuTenantSubjectV1,
    type_id = gts_id!("x.test.ovr4.subject.v1~x.test.ovr4.regional.v1~x.test.ovr4.eu_tenant.v1~x.test.ovr4.eu_west_premium.v1~"),
    description = "Premium EU-west tenant subject",
    properties = "seats",
    overrides(
        SubjectV1 = PremiumLevelOverride,
        RegionalSubjectV1 = EuWestOverride,
        EuTenantSubjectV1 = ShortTenantOverride
    )
)]
#[derive(Debug)]
pub struct EuWestPremiumSubjectV1 {
    pub seats: u32,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (SubjectV1::<()>::TYPE_ID.to_owned(), SubjectV1::<()>::gts_schema_with_refs()),
        (
            RegionalSubjectV1::<()>::TYPE_ID.to_owned(),
            RegionalSubjectV1::<()>::gts_schema_with_refs(),
        ),
        (
            EuTenantSubjectV1::<()>::TYPE_ID.to_owned(),
            EuTenantSubjectV1::<()>::gts_schema_with_refs(),
        ),
        (
            EuWestPremiumSubjectV1::TYPE_ID.to_owned(),
            EuWestPremiumSubjectV1::gts_schema_with_refs(),
        ),
    ]
}
