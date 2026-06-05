// Golden case: the trait surface is a *derivation chain of registered GTS trait
// types* (not inline). A generic base trait `PriorityTraitV1<P>` declares an
// open `payload` slot; a derived trait `UrgentDetailTraitV1` specifies that
// payload. Because the derived trait is itself a `#[struct_to_gts_schema]`
// child, the macro emits its document as `allOf: [{ $ref: gts://<base-trait> }]`
// — i.e. the derived traits schema references the base traits schema.
//
// The hosts pull these in via the referenced form (`traits_schema = T`): the
// base host references the base trait, the derived host references the derived
// trait (which in turn `$ref`s the base trait, so the base surface is reachable
// through the chain).

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

// --- trait types (the reusable, separately-governed trait surface) -----------

#[derive(Debug, JsonSchema, serde::Serialize, serde::Deserialize)]
#[schemars(inline)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Medium,
    High,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.traits.priority.v1~",
    description = "Reusable priority trait with an open payload slot",
    properties = "id,label,priority,payload"
)]
#[derive(Debug, JsonSchema)]
pub struct PriorityTraitV1<P> {
    pub id: GtsInstanceId,
    #[schemars(length(min = 1, max = 32))]
    pub label: String,
    pub priority: Priority,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = PriorityTraitV1,
    type_id = "gts.x.test.traits.priority.v1~x.test.urgent.detail.v1~",
    description = "Priority trait specifying the payload slot",
    properties = "category"
)]
#[derive(Debug, JsonSchema)]
pub struct UrgentDetailTraitV1 {
    pub category: String,
}

// --- hosts (reference the trait types via the referenced form) ---------------

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.golden.refchain.v1~",
    description = "Base host referencing the base priority trait type",
    properties = "id,payload",
    traits_schema = PriorityTraitV1::<()>,
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct RefBaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

// Abstract: the referenced trait chain carries required properties (`id`,
// `priority`, `payload`) that this host does not resolve, so OP#13 completeness
// is skipped for it. The point of the case is the `$ref` emission.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RefBaseV1,
    type_id = "gts.x.test.golden.refchain.v1~x.test.urgent.event.v1~",
    description = "Derived host referencing the derived priority trait type",
    properties = "order_id",
    traits_schema = UrgentDetailTraitV1,
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct RefUrgentV1 {
    pub order_id: String,
}

pub fn schemas() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            PriorityTraitV1::<()>::TYPE_ID.to_owned(),
            PriorityTraitV1::<()>::gts_schema_with_refs(),
        ),
        (
            UrgentDetailTraitV1::TYPE_ID.to_owned(),
            UrgentDetailTraitV1::gts_schema_with_refs(),
        ),
        (
            RefBaseV1::<()>::TYPE_ID.to_owned(),
            RefBaseV1::<()>::gts_schema_with_refs(),
        ),
        (
            RefUrgentV1::TYPE_ID.to_owned(),
            RefUrgentV1::gts_schema_with_refs(),
        ),
    ]
}
