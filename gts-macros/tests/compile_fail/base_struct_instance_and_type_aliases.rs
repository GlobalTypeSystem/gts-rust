//! Runtime ID and type aliases count as identity fields.

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.identity.iid_type.v1~",
    description = "Instance ID under a runtime alias beside a GTS type",
    properties = "id,gts_type"
)]
pub struct IidAndTypeV1 {
    #[serde(rename = "gtsIid")]
    pub id: gts::GtsInstanceId,
    pub gts_type: gts::GtsTypeId,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.identity.id_tid.v1~",
    description = "Instance ID beside a GTS type under a runtime alias",
    properties = "id,parent"
)]
pub struct IdAndTidV1 {
    pub id: gts::GtsInstanceId,
    #[serde(rename = "gtsTid")]
    pub parent: gts::GtsTypeId,
}

fn main() {}
