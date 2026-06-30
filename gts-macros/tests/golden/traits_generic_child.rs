// Golden case: a still-generic mid (derives from a generic base, payload slot
// not yet bound) that carries `x-gts-traits` and a modifier. Exercises the
// generic + `base = Parent` emission branch's top-level trait/modifier
// injection, which the non-generic leaf cases do not reach. Mirrors the
// `AuditPayloadV1<D>` / `PlaceOrderDataV1<E>` shape from gts-macros-cli: the
// generic param is the nested payload field carried forward down the chain.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::{gts_id, struct_to_gts_schema, GtsTraitsSchema};
use schemars::JsonSchema;

const TOPIC_REF: &str = gts_id!("x.core.events.topic.v1~");

#[derive(JsonSchema, serde::Serialize, GtsTraitsSchema)]
pub struct EventTraits {
    #[schemars(extend("x-gts-ref" = TOPIC_REF))]
    pub topic_ref: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.gen.event.v1~"),
    description = "Abstract generic base",
    properties = "id,payload",
    traits_schema = inline(EventTraits),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = gts_id!("x.test.gen.event.v1~x.test.audit.event.v1~"),
    description = "Still-generic abstract mid resolving the inherited topic trait",
    properties = "user_id,data",
    traits = serde_json::json!({
        "topic_ref": gts_id!("x.core.events.topic.v1~x.test._.audit.v1")
    }),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct AuditEventV1<D> {
    pub user_id: String,
    pub data: D,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (EventV1::<()>::TYPE_ID.to_owned(), EventV1::<()>::gts_schema_with_refs()),
        (
            AuditEventV1::<()>::TYPE_ID.to_owned(),
            AuditEventV1::<()>::gts_schema_with_refs(),
        ),
    ]
}
