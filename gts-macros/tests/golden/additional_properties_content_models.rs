// Golden case: schema-valued map content models and combinator branches must
// remain untouched. Closing either as an ordinary object would change its JSON
// Schema meaning and reject otherwise valid instances.

use std::collections::HashMap;

use gts::{GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

#[derive(Debug, JsonSchema, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Choice {
    ByName { name: String },
    ByCode { code: u32 },
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.golden.contentmodels.v1~"),
    description = "Host preserving map and combinator content models",
    properties = "schema_type,labels,choice",
)]
#[derive(Debug)]
pub struct ContentModelsHostV1 {
    #[serde(rename = "type")]
    pub schema_type: GtsTypeId,
    pub labels: HashMap<String, String>,
    pub choice: Choice,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![(
        ContentModelsHostV1::TYPE_ID.to_owned(),
        ContentModelsHostV1::gts_schema_with_refs(),
    )]
}
