#![allow(clippy::expect_used)]

use gts::GtsSchema;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_generics.event.v1~",
    description = "Event envelope",
    properties = "id,subject_id,payload"
)]
pub struct EventV1<P> {
    pub id: gts::GtsInstanceId,
    pub subject_id: String,
    pub payload: P,
}

#[derive(schemars::JsonSchema)]
pub struct SubjectRules<T> {
    pub subject_id: T,
}

mod model {
    // These names intentionally match the annotated structs' generic parameter.
    pub use super::EventV1 as E;

    pub mod subject {
        #[derive(schemars::JsonSchema)]
        #[allow(dead_code)] // schema-only type, never constructed
        pub enum E {
            Order,
        }
    }
}

use model::subject::E as OrderSubject;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_generics.event.v1~x.test.ovr_generics.named.v1~",
    description = "Concrete generic helper",
    properties = "ext",
    overrides(EventV1 = SubjectRules<OrderSubject>),
)]
pub struct NamedV1<E> {
    pub ext: E,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_generics.event.v1~x.test.ovr_generics.qualified.v1~",
    description = "Qualified names are independent of the generic parameter",
    properties = "ext",
    overrides(EventV1<()> = SubjectRules<model::subject::E>),
)]
pub struct QualifiedV1<E> {
    pub ext: E,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_generics.event.v1~x.test.ovr_generics.inline.v1~",
    description = "Qualified names in inline overrides",
    properties = "ext",
    overrides(model::E = { subject_id: model::subject::E }),
)]
pub struct InlineV1<E> {
    pub ext: E,
}

#[test]
fn concrete_generic_helpers_narrow_inherited_fields() {
    NamedV1::<()>::validate_overrides().expect("concrete generic helper is admitted");
    let schema = NamedV1::<()>::gts_schema_with_refs();
    assert_eq!(
        schema["allOf"][1]["properties"]["subject_id"],
        serde_json::json!({"type": "string", "enum": ["Order"]}),
    );
}

#[test]
fn qualified_names_do_not_capture_the_structs_generic_parameter() {
    QualifiedV1::<()>::validate_overrides().expect("qualified generic helper is admitted");
    InlineV1::<()>::validate_overrides().expect("qualified inline field is admitted");
    let expected = NamedV1::<()>::gts_schema_with_refs();
    for schema in [
        QualifiedV1::<()>::gts_schema_with_refs(),
        InlineV1::<()>::gts_schema_with_refs(),
    ] {
        assert_eq!(schema["allOf"][1], expected["allOf"][1]);
    }
}
