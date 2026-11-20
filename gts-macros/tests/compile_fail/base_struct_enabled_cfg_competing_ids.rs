//! Fields kept by `cfg` or renamed by an enabled `cfg_attr` still compete.

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.identity.enabled_cfg.v1~",
    description = "Two enabled GTS IDs",
    properties = "id"
)]
pub struct EnabledCfgV1 {
    #[cfg(all())]
    pub gts_id: gts::GtsInstanceId,
    pub id: gts::GtsInstanceId,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.identity.enabled_cfg_attr.v1~",
    description = "A GTS ID renamed into a competing alias",
    properties = "id,other"
)]
pub struct EnabledCfgAttrV1 {
    pub id: gts::GtsInstanceId,
    #[cfg_attr(all(), serde(rename = "gtsId"))]
    pub other: gts::GtsInstanceId,
}

fn main() {}
