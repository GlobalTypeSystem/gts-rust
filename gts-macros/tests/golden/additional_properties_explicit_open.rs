// Golden case: an explicitly open nested struct is a deliberate extension
// point. The macro must preserve its `additionalProperties: true` instead of
// replacing it with the default closed content model.

use gts::{GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[derive(Debug, JsonSchema, serde::Serialize, serde::Deserialize)]
#[schemars(extend("additionalProperties" = true))]
pub struct ExtensionPoint {
    pub label: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.golden.explicitopen.v1~"),
    description = "Host with an explicitly open nested extension point",
    properties = "schema_type,extension",
)]
#[derive(Debug)]
pub struct ExplicitOpenHostV1 {
    #[serde(rename = "type")]
    pub schema_type: GtsTypeId,
    pub extension: ExtensionPoint,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![(
        ExplicitOpenHostV1::TYPE_ID.to_owned(),
        ExplicitOpenHostV1::gts_schema_with_refs(),
    )]
}
