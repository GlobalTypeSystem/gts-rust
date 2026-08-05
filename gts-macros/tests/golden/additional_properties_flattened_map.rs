// Golden case: flattening a map is the Serde/Schemars way to model an object
// with known properties plus arbitrary extension properties. Schemars emits
// `additionalProperties: true`; the macro must preserve that open model.

use std::collections::HashMap;

use gts::{GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[derive(Debug, JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct ExtensibleMetadata {
    pub label: String,
    #[serde(flatten)]
    pub extensions: HashMap<String, serde_json::Value>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.golden.flattenedmap.v1~"),
    description = "Host with a nested struct opened by a flattened map",
    properties = "schema_type,metadata",
)]
#[derive(Debug)]
pub struct FlattenedMapHostV1 {
    #[serde(rename = "type")]
    pub schema_type: GtsTypeId,
    pub metadata: ExtensibleMetadata,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![(
        FlattenedMapHostV1::TYPE_ID.to_owned(),
        FlattenedMapHostV1::gts_schema_with_refs(),
    )]
}
