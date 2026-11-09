//! Schema declarations need no filesystem configuration.

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    base = true,
    type_id = gts_id!("x.test.paths.base.v1~"),
    description = "Base entity",
    properties = "id,payload"
)]
pub struct BaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    base = BaseV1,
    type_id = gts_id!("x.test.paths.base.v1~x.test.paths.detail.v1~"),
    description = "Entity details",
    properties = "name"
)]
pub struct DetailV1 {
    pub name: String,
}

#[test]
fn declarations_generate_schemas_in_memory_without_output_paths() {
    let base = BaseV1::<()>::gts_schema();
    assert_eq!(base["description"], "Base entity");
    assert!(base["properties"]["id"].is_object());

    let derived = DetailV1::gts_schema_with_refs();
    assert_eq!(derived["allOf"][0]["$ref"], base["$id"]);
    assert_eq!(
        derived["allOf"][1]["properties"]["payload"]["properties"]["name"]["type"],
        "string"
    );
}
