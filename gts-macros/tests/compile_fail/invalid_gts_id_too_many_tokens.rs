//! Test: GTS schema ID with too many tokens in segment
//! This is the exact case from issue #47
//! The second segment has 5 name tokens instead of 4:
//! x.core.license_enforcer.integration.plugin.v1
//! Should be: vendor.package.namespace.type.vMAJOR

use gts_macros::struct_to_gts_schema;

// First define the base struct that we extend
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.core.modkit.plugin.v1~",
    description = "Base modkit plugin",
    properties = "id"
)]
pub struct BaseModkitPluginV1 {
    pub id: gts::GtsInstanceId,
}

// This should fail - the second segment has too many tokens
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseModkitPluginV1,
    type_id = "gts.x.core.modkit.plugin.v1~x.core.license_enforcer.integration.plugin.v1~",
    description = "License Enforcer platform integration plugin specification",
    properties = ""
)]
pub struct LicensePlatformPluginSpecV1;

fn main() {}
