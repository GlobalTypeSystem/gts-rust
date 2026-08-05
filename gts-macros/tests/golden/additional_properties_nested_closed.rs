// Golden case: ordinary nested Rust structs are closed at every object level.
// Schemars emits them through Draft-07 `definitions`; the macro must preserve
// those definitions and add `additionalProperties: false` recursively.

use gts::{GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[derive(Debug, JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct Contact {
    pub email: String,
}

#[derive(Debug, JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    pub display_name: String,
    pub contact: Contact,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.golden.nestedclosed.v1~"),
    description = "Host with recursively closed nested data structs",
    properties = "schema_type,profile",
)]
#[derive(Debug)]
pub struct NestedClosedHostV1 {
    #[serde(rename = "type")]
    pub schema_type: GtsTypeId,
    pub profile: Profile,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![(
        NestedClosedHostV1::TYPE_ID.to_owned(),
        NestedClosedHostV1::gts_schema_with_refs(),
    )]
}
