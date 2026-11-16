//! Qualified helpers and associated types must still reject dependencies on E.

use gts::GtsInstanceId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_dependencies.event.v1~",
    description = "Event envelope",
    properties = "id,subject_id,payload",
)]
pub struct EventV1<P> {
    pub id: GtsInstanceId,
    pub subject_id: String,
    pub payload: P,
}

mod helpers {
    #[derive(schemars::JsonSchema)]
    pub struct Rules<T> {
        pub subject_id: T,
    }
}

pub trait Subject {
    type Id;
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_dependencies.event.v1~x.test.ovr_dependencies.nested.v1~",
    description = "Nested generic dependency",
    properties = "ext",
    overrides(EventV1 = helpers::Rules<Option<E>>),
)]
pub struct NestedV1<E> {
    pub ext: E,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_dependencies.event.v1~x.test.ovr_dependencies.associated.v1~",
    description = "Associated type dependency",
    properties = "ext",
    overrides(EventV1 = helpers::Rules<E::Id>),
)]
pub struct AssociatedV1<E: Subject> {
    pub ext: E,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_dependencies.event.v1~x.test.ovr_dependencies.qualified.v1~",
    description = "Qualified associated type dependency",
    properties = "ext",
    overrides(EventV1 = { subject_id: <E as Subject>::Id }),
)]
pub struct QualifiedV1<E: Subject> {
    pub ext: E,
}

fn main() {}
