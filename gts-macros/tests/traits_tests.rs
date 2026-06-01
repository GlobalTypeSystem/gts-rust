//! Trait (§9.7 / OP#13) **behaviour** tests for the macros — things a static
//! golden cannot capture: chain composition through the registry and
//! JSON-Schema compilability of the emitted trait shape.
//!
//! Emission **snapshots** live in the `golden_tests` test target
//! (`tests/golden/traits_*`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gts::{GtsInstanceId, GtsSchema};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;

fn default_retention() -> String {
    "P30D".to_owned()
}

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct EventTraits {
    #[schemars(extend("x-gts-ref" = "gts.x.core.events.topic.v1~"))]
    pub topic_ref: String,
    #[serde(default = "default_retention")]
    pub retention: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.behav.event.v1~",
    description = "Base event",
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
    type_id = "gts.x.test.behav.event.v1~x.test.order.placed.v1~",
    description = "Order placed",
    properties = "order_id",
    traits = serde_json::json!({
        "topic_ref": "gts.x.core.events.topic.v1~x.test._.orders.v1"
    }),
    gts_final = true,
)]
#[derive(Debug)]
pub struct OrderPlacedV1 {
    pub order_id: String,
}

// --- Negative cases (OP#13 / §9.7) ------------------------------------------
//
// These exercise *registry-time* trait validation that the proc-macro
// deliberately cannot perform at compile time: the required-property set of an
// `inline(T)` trait schema is computed by `schemars` at runtime, the trait
// *values* are runtime `serde_json` expressions, and cross-type compatibility
// is full JSON-Schema composition. Each builds real macro-generated schemas and
// runs them through the registry, asserting the expected failure.

/// Non-abstract base carrying `EventTraits` (which has the required `topic_ref`)
/// but supplying no `x-gts-traits`. A concrete type must resolve every required
/// trait, so registration must fail.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.behav.req.v1~",
    description = "Non-abstract base with an unresolved required trait",
    properties = "id,payload",
    traits_schema = inline(EventTraits),
)]
#[derive(Debug)]
pub struct ReqV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

/// Abstract base carrying the required `topic_ref` trait with no values — legal,
/// because abstract types are exempt from the completeness check.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.behav.absreq.v1~",
    description = "Abstract base with a required trait and no values",
    properties = "id,payload",
    traits_schema = inline(EventTraits),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct AbsReqV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

/// Concrete leaf under [`AbsReqV1`] that supplies no `x-gts-traits` — it leaves
/// the inherited required `topic_ref` unresolved, so it must fail.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AbsReqV1,
    type_id = "gts.x.test.behav.absreq.v1~x.test.absreq.leaf.v1~",
    description = "Concrete leaf leaving an inherited required trait unresolved",
    properties = "name",
)]
#[derive(Debug)]
pub struct AbsLeafV1 {
    pub name: String,
}

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct RetentionString {
    pub retention: String,
}

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct RetentionInt {
    pub retention: i64,
}

/// Abstract base declaring `retention` as a string trait.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.behav.compat.v1~",
    description = "Base declaring retention as a string trait",
    properties = "id,payload",
    traits_schema = inline(RetentionString),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct CompatV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

/// Derived type whose own trait schema redeclares `retention` as an integer —
/// incompatible with the parent's string declaration. Its `retention` value
/// cannot satisfy both branches of the composed `allOf`, so it must fail.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = CompatV1,
    type_id = "gts.x.test.behav.compat.v1~x.test.compat.bad.v1~",
    description = "Derived type whose trait schema contradicts the parent's",
    properties = "name",
    traits_schema = inline(RetentionInt),
    traits = serde_json::json!({ "retention": "P30D" }),
)]
#[derive(Debug)]
pub struct CompatBadV1 {
    pub name: String,
}

// --- const lock (§9.7.5) ----------------------------------------------------

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct IndexedTraits {
    #[schemars(extend("const" = true))]
    pub indexed: bool,
}

/// Abstract base locking `indexed` to `true` via a `const` in its trait schema.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.behav.const.v1~",
    description = "Base locking the indexed trait to true",
    properties = "id,payload",
    traits_schema = inline(IndexedTraits),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct ConstBaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

/// Concrete leaf trying to override the `const`-locked `indexed` with `false` —
/// the materialized trait value cannot satisfy `const: true`, so it must fail.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = ConstBaseV1,
    type_id = "gts.x.test.behav.const.v1~x.test.const.bad.v1~",
    description = "Leaf overriding a const-locked trait",
    properties = "name",
    traits = serde_json::json!({ "indexed": false }),
)]
#[derive(Debug)]
pub struct ConstBadLeafV1 {
    pub name: String,
}

// --- default materializes a required trait (§9.7.5) --------------------------

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct DefaultedTraits {
    // Required (non-Option, no serde default) yet carries a schema `default`, so
    // materialization fills it on a concrete type that supplies no value.
    #[schemars(extend("default" = "P30D"))]
    pub retention: String,
}

/// Non-abstract base whose only required trait has a `default`. It supplies no
/// `x-gts-traits`, but materialization fills `retention`, so completeness passes.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.behav.defmat.v1~",
    description = "Concrete base relying on a trait default for completeness",
    properties = "id",
    traits_schema = inline(DefaultedTraits),
)]
#[derive(Debug)]
pub struct DefaultBaseV1 {
    pub id: GtsInstanceId,
}

// --- additionalProperties: false narrowing (§9.7.2 / §3.1) -------------------

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
#[schemars(extend("additionalProperties" = false))]
pub struct ClosedTraits {
    #[schemars(extend("x-gts-ref" = "gts.x.core.events.topic.v1~"))]
    pub topic_ref: String,
}

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct ExtraTraits {
    pub extra: String,
}

/// Abstract base whose trait schema is closed (`additionalProperties: false`).
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.behav.closed.v1~",
    description = "Base with a closed trait surface",
    properties = "id,payload",
    traits_schema = inline(ClosedTraits),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct ClosedBaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

/// Concrete leaf introducing a new trait property `extra`. Because the ancestor
/// declared `additionalProperties: false`, the composed `allOf` rejects the new
/// property against the ancestor branch, so registration must fail.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = ClosedBaseV1,
    type_id = "gts.x.test.behav.closed.v1~x.test.extend.bad.v1~",
    description = "Leaf extending a closed trait surface with a new property",
    properties = "name",
    traits_schema = inline(ExtraTraits),
    traits = serde_json::json!({
        "topic_ref": "gts.x.core.events.topic.v1~x.test._.orders.v1",
        "extra": "nope"
    }),
)]
#[derive(Debug)]
pub struct ExtendBadLeafV1 {
    pub name: String,
}

// --- trait-schema chain merge via allOf (§9.7.5) -----------------------------

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct BasePriorityTraits {
    #[schemars(extend("default" = "medium"))]
    pub priority: String,
}

#[derive(JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct NarrowPriorityTraits {
    #[schemars(extend("enum" = ["low", "medium", "high", "critical"]))]
    pub priority: String,
}

/// Abstract base declaring `priority` as an open string trait.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.behav.narrow.v1~",
    description = "Base declaring an open-string priority trait",
    properties = "id,payload",
    traits_schema = inline(BasePriorityTraits),
    gts_abstract = true,
)]
#[derive(Debug)]
pub struct NarrowBaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

/// Leaf narrowing `priority` to an enum (its own trait-schema) and resolving it
/// with an in-enum value. The registry composes base + leaf trait-schemas via
/// `allOf`; "high" satisfies both branches.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = NarrowBaseV1,
    type_id = "gts.x.test.behav.narrow.v1~x.test.urgent.ok.v1~",
    description = "Leaf narrowing priority and supplying a valid value",
    properties = "order_id",
    traits_schema = inline(NarrowPriorityTraits),
    traits = serde_json::json!({ "priority": "high" }),
    gts_final = true,
)]
#[derive(Debug)]
pub struct NarrowOkLeafV1 {
    pub order_id: String,
}

/// Same narrowing, but the resolved value is outside the enum — it satisfies the
/// base's `string` branch yet violates the leaf's enum branch, so the composed
/// `allOf` rejects it.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = NarrowBaseV1,
    type_id = "gts.x.test.behav.narrow.v1~x.test.urgent.bad.v1~",
    description = "Leaf narrowing priority but supplying an out-of-enum value",
    properties = "order_id",
    traits_schema = inline(NarrowPriorityTraits),
    traits = serde_json::json!({ "priority": "ultra" }),
    gts_final = true,
)]
#[derive(Debug)]
pub struct NarrowBadLeafV1 {
    pub order_id: String,
}

/// A non-abstract type with a required trait and no values must fail — every
/// required trait property must be resolved on a concrete type.
#[test]
fn required_trait_unresolved_on_non_abstract_base_fails() {
    let base = ReqV1::<()>::gts_schema_with_refs();
    let err = gts::testing::validate_traits_chain(&[&base])
        .expect_err("non-abstract base with an unresolved required trait must fail OP#13");
    assert!(
        err.contains("topic_ref"),
        "error should name the unresolved required trait: {err}"
    );
}

/// An abstract base may leave a required trait unresolved, but a concrete type
/// derived from it that still supplies no value must fail.
#[test]
fn abstract_base_required_trait_unresolved_on_concrete_leaf_fails() {
    let base = AbsReqV1::<()>::gts_schema_with_refs();
    let leaf = AbsLeafV1::gts_schema_with_refs();
    let err = gts::testing::validate_traits_chain(&[&base, &leaf])
        .expect_err("concrete leaf leaving an inherited required trait unresolved must fail");
    assert!(
        err.contains("topic_ref"),
        "error should name the unresolved required trait: {err}"
    );
}

/// A derived type whose own trait schema contradicts the parent's must fail —
/// the composed `allOf` is unsatisfiable for the supplied value.
#[test]
fn derived_trait_schema_incompatible_with_parent_fails() {
    let base = CompatV1::<()>::gts_schema_with_refs();
    let leaf = CompatBadV1::gts_schema_with_refs();
    let err = gts::testing::validate_traits_chain(&[&base, &leaf])
        .expect_err("a derived trait schema that contradicts the parent's must fail composition");
    assert!(
        err.contains("retention") || err.contains("integer") || err.contains("trait validation"),
        "error should reflect the schema conflict: {err}"
    );
}

/// A leaf that overrides a `const`-locked trait value must fail — the
/// materialized effective traits cannot satisfy `const: true` (§9.7.5). Proves
/// the macro's schemars `extend("const" = ...)` lands so the registry enforces
/// the lock.
#[test]
fn const_locked_trait_override_on_leaf_fails() {
    let base = ConstBaseV1::<()>::gts_schema_with_refs();
    let leaf = ConstBadLeafV1::gts_schema_with_refs();
    let err = gts::testing::validate_traits_chain(&[&base, &leaf])
        .expect_err("overriding a const-locked trait value must fail OP#13");
    assert!(
        err.contains("indexed") || err.contains("const") || err.contains("trait validation"),
        "error should reflect the const-lock violation: {err}"
    );
}

/// A non-abstract type whose only required trait carries a `default` passes
/// completeness with no `x-gts-traits` — materialization fills the value before
/// the check (§9.7.5). Proves schemars `extend("default" = ...)` reaches the
/// effective trait-schema.
#[test]
fn default_materializes_required_trait_on_concrete_base() {
    let base = DefaultBaseV1::gts_schema_with_refs();
    gts::testing::validate_traits_chain(&[&base])
        .expect("a required trait with a default must be materialized and pass OP#13");
}

/// A leaf that introduces a new trait property under an ancestor's
/// `additionalProperties: false` must fail — the composed `allOf` rejects the
/// extra property against the closed ancestor branch (§9.7.2 / §3.1). Proves the
/// container-level schemars `extend("additionalProperties" = false)` composes.
#[test]
fn extending_closed_trait_surface_on_leaf_fails() {
    let base = ClosedBaseV1::<()>::gts_schema_with_refs();
    let leaf = ExtendBadLeafV1::gts_schema_with_refs();
    let err = gts::testing::validate_traits_chain(&[&base, &leaf])
        .expect_err("introducing a property under additionalProperties:false must fail");
    assert!(
        err.contains("extra") || err.contains("additional") || err.contains("trait validation"),
        "error should reflect the closed-surface violation: {err}"
    );
}

/// A base and a leaf each declare `x-gts-traits-schema`; the registry composes
/// them via `allOf`. The leaf narrows `priority` to an enum but resolves it with
/// an out-of-enum value: the value satisfies the base's `string` branch yet
/// fails the leaf's enum branch, proving both trait-schemas are merged. (The
/// accept path — an in-enum value passing — is the golden `traits_schema_narrowing`
/// case, registry-validated by the golden harness.)
#[test]
fn chained_trait_schemas_merge_via_allof_and_reject_out_of_range() {
    let base = NarrowBaseV1::<()>::gts_schema_with_refs();
    let leaf = NarrowBadLeafV1::gts_schema_with_refs();
    let err = gts::testing::validate_traits_chain(&[&base, &leaf])
        .expect_err("an out-of-enum value must fail the composed allOf trait-schema");
    assert!(
        err.contains("priority") || err.contains("enum") || err.contains("trait validation"),
        "error should reflect the enum-narrowing violation: {err}"
    );
}

// --- typed trait accessors (GtsSchema::gts_traits_schema / gts_traits) -------

/// The `gts_traits_schema()` / `gts_traits()` accessors must return the exact
/// declared trait shape / values (asserted against explicit literals, not
/// against the document they came from), `None` when the keyword is absent, and
/// the emitted document must carry the very same literal.
#[test]
fn trait_accessors_return_the_declared_values() {
    use serde_json::json;

    // Base `EventV1`: declares an inline `traits_schema` (from `EventTraits`),
    // resolves no values. `topic_ref` is required (no default); `retention`
    // carries the serde default and is therefore optional.
    let event_ts = json!({
        "type": "object",
        "properties": {
            "topic_ref": { "type": "string", "x-gts-ref": "gts.x.core.events.topic.v1~" },
            "retention": { "type": "string", "default": "P30D" }
        },
        "required": ["topic_ref"]
    });
    assert_eq!(EventV1::<()>::gts_traits_schema(), Some(event_ts.clone()));
    assert_eq!(EventV1::<()>::gts_traits(), None);
    // Independent cross-check: the document carries the identical literal, so the
    // accessor is not a second, divergent source of truth.
    assert_eq!(
        EventV1::<()>::gts_schema_with_refs().get("x-gts-traits-schema"),
        Some(&event_ts)
    );

    // Leaf `OrderPlacedV1`: resolves `traits` values, declares no local schema.
    let order_traits = json!({ "topic_ref": "gts.x.core.events.topic.v1~x.test._.orders.v1" });
    assert_eq!(OrderPlacedV1::gts_traits(), Some(order_traits.clone()));
    assert_eq!(OrderPlacedV1::gts_traits_schema(), None);
    assert_eq!(
        OrderPlacedV1::gts_schema_with_refs().get("x-gts-traits"),
        Some(&order_traits)
    );

    // `NarrowOkLeafV1`: both keywords (§9.7.4) — narrows `priority` to an enum
    // and resolves it.
    assert_eq!(
        NarrowOkLeafV1::gts_traits_schema(),
        Some(json!({
            "type": "object",
            "properties": {
                "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] }
            },
            "required": ["priority"]
        }))
    );
    assert_eq!(
        NarrowOkLeafV1::gts_traits(),
        Some(json!({ "priority": "high" }))
    );

    // Default impl: a type that declares no traits returns `None` for both.
    assert_eq!(<() as gts::GtsSchema>::gts_traits_schema(), None);
    assert_eq!(<() as gts::GtsSchema>::gts_traits(), None);
}
