// Golden case: an `Option<D>` generic slot on a mid-chain type under a base
// whose slot is a bare `P`. The mid renders its slot as `["object", "null"]`
// at the nested path; a leaf with a payload requires and narrows it; a
// body-less leaf keeps it optional and nullable.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.test.optmid.event.v1~"),
    description = "Base event with a required payload",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = gts_id!("x.test.optmid.event.v1~x.test.optmid.record.v1~"),
    description = "Mid-chain record whose details may be absent",
    properties = "actor,details"
)]
#[derive(Debug)]
pub struct RecordV1<D> {
    pub actor: String,
    pub details: Option<D>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RecordV1,
    type_id = gts_id!("x.test.optmid.event.v1~x.test.optmid.record.v1~x.test.optmid.login.v1~"),
    description = "Leaf with a payload",
    properties = "ip"
)]
#[derive(Debug)]
pub struct LoginV1 {
    pub ip: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RecordV1,
    type_id = gts_id!("x.test.optmid.event.v1~x.test.optmid.record.v1~x.test.optmid.logout.v1~"),
    description = "Body-less leaf",
    properties = ""
)]
#[derive(Debug)]
pub struct LogoutV1;

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (EventV1::<()>::TYPE_ID.to_owned(), EventV1::<()>::gts_schema_with_refs()),
        (RecordV1::<()>::TYPE_ID.to_owned(), RecordV1::<()>::gts_schema_with_refs()),
        (LoginV1::TYPE_ID.to_owned(), LoginV1::gts_schema_with_refs()),
        (LogoutV1::TYPE_ID.to_owned(), LogoutV1::gts_schema_with_refs()),
    ]
}
