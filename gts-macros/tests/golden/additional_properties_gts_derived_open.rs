// Golden case: on a derived GTS struct, an explicitly open Schemars content
// model applies to the nested object level that carries the derived type's own
// fields, not to the document-level `allOf` wrapper.

use gts::{GtsSchema, GtsTypeId};
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.golden.openchain.v1~"),
    description = "Abstract base with a payload extension slot",
    properties = "schema_type,payload",
    gts_abstract = true,
)]
#[derive(Debug)]
#[schemars(extend("additionalProperties" = true))]
pub struct OpenChainBaseV1<P> {
    #[serde(rename = "type")]
    pub schema_type: GtsTypeId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OpenChainBaseV1,
    type_id = gts_id!("x.test.golden.openchain.v1~x.test.audit.payload.v1~"),
    description = "Derived payload with an explicitly open content model",
    properties = "label,data",
    gts_abstract = true,
)]
#[derive(Debug)]
#[schemars(extend("additionalProperties" = true))]
pub struct OpenDerivedPayloadV1<D> {
    pub label: String,
    pub data: D,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OpenDerivedPayloadV1,
    type_id = gts_id!(
        "x.test.golden.openchain.v1~x.test.audit.payload.v1~x.test.final.payload.v1~"
    ),
    description = "Open non-generic leaf nested under the derived data slot",
    properties = "value",
)]
#[derive(Debug)]
#[schemars(extend("additionalProperties" = true))]
pub struct OpenDerivedLeafV1 {
    pub value: String,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            OpenChainBaseV1::<()>::TYPE_ID.to_owned(),
            OpenChainBaseV1::<()>::gts_schema_with_refs(),
        ),
        (
            OpenDerivedPayloadV1::<()>::TYPE_ID.to_owned(),
            OpenDerivedPayloadV1::<()>::gts_schema_with_refs(),
        ),
        (
            OpenDerivedLeafV1::TYPE_ID.to_owned(),
            OpenDerivedLeafV1::gts_schema_with_refs(),
        ),
    ]
}
