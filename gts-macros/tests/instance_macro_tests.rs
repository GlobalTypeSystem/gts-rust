//! Runtime tests for `gts_instance!` and `gts_instance_raw!`.
//!
//! Covers:
//! - Expression form: id rewriting from a string literal field, the
//!   compile-time prefix assert on the happy path (mismatch cases live
//!   in `compile_fail/`).
//! - `#[gts_static(NAME)]`: emits `pub static NAME: LazyLock<T>` over
//!   the rewritten struct.
//! - Auto-derivation of the const-assert target for chained generic
//!   carriers — `BaseV1::<LeafV1> { ... }` targets `LeafV1`'s `SCHEMA_ID`.
//! - Alternative id field names (`gts_id`, `gtsId`) — picked from the
//!   struct literal automatically, no extra modifier needed.
//! - Raw expression form: id auto-injection into JSON.
//!
//! All fixture types here are produced by `#[struct_to_gts_schema]` —
//! `gts_instance!` only requires `T: GtsSchema`, but in practice every
//! consumer goes through the macro so the schema and the instance share
//! the same registration story. Tests follow the same convention.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gts::GtsInstanceId;
use gts_macros::{gts_instance, gts_instance_raw, struct_to_gts_schema};

// ---------- Local test types ----------
//
// Generic example types — neutral naming to keep the tests applicable to
// any GTS consumer, not tied to a particular framework or domain.

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.acme.core.events.topic.v1~",
    description = "Test topic type used to exercise gts_instance!",
    properties = "id,name,retention"
)]
#[derive(Debug)]
pub struct TopicV1 {
    pub id: GtsInstanceId,
    pub name: String,
    pub retention: String,
}

// Two-level chain: `BaseV1<P>` is the level-1 base (carrying `id` +
// `payload`), `LeafV1` is a level-2 derived schema. Used to cover the
// auto-derivation of the const-assert target from the carrier's
// turbofish.

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.acme.core.test.base.v1~",
    description = "Generic base type for chained-instance turbofish tests",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct BaseV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseV1,
    schema_id = "gts.acme.core.test.base.v1~acme.core.test.leaf.v1~",
    description = "Derived leaf for chained-instance turbofish tests",
    properties = "name"
)]
#[derive(Debug)]
pub struct LeafV1 {
    pub name: String,
}

// Three-level chain: `L1OuterV1<P>` (base) -> `L2MidV1<D>` (derives from
// L1OuterV1) -> `L3LeafV1` (derives from L2MidV1). Used to verify that the
// auto-derivation walks through nested generics and picks the *deepest*
// non-generic type, not any intermediate one. With a wrong choice the
// const-assert would either reject the literal (target's SCHEMA_ID is
// not a prefix of the full-chain id) or accept under a stale prefix —
// the only target whose SCHEMA_ID is exactly the full-chain prefix is
// `L3LeafV1`.

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.acme.core.test.l1.v1~",
    description = "Level-1 base for three-level chained-instance tests",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct L1OuterV1<P> {
    pub id: GtsInstanceId,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = L1OuterV1,
    schema_id = "gts.acme.core.test.l1.v1~acme.core.test.l2.v1~",
    description = "Level-2 mid for three-level chained-instance tests",
    properties = "data"
)]
#[derive(Debug)]
pub struct L2MidV1<D> {
    pub data: D,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = L2MidV1,
    schema_id = "gts.acme.core.test.l1.v1~acme.core.test.l2.v1~acme.core.test.l3.v1~",
    description = "Level-3 leaf for three-level chained-instance tests",
    properties = "value"
)]
#[derive(Debug)]
pub struct L3LeafV1 {
    pub value: String,
}

// =====================================================================
//                  gts_instance! — expression form
// =====================================================================

#[test]
fn typed_form_constructs_value_with_rewritten_id() {
    let t: TopicV1 = gts_instance!(TopicV1 {
        id: "gts.acme.core.events.topic.v1~vendor.app.orders.created.v1",
        name: "orders".to_owned(),
        retention: "P30D".to_owned(),
    });

    assert_eq!(t.name, "orders");
    assert_eq!(t.retention, "P30D");
    assert_eq!(
        t.id.as_ref(),
        "gts.acme.core.events.topic.v1~vendor.app.orders.created.v1"
    );
}

#[test]
fn typed_form_serialises_with_id_field() {
    let t: TopicV1 = gts_instance!(TopicV1 {
        id: "gts.acme.core.events.topic.v1~vendor.app.events.audit.v1",
        name: "audit".to_owned(),
        retention: "P7D".to_owned(),
    });

    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(
        v["id"].as_str().unwrap(),
        "gts.acme.core.events.topic.v1~vendor.app.events.audit.v1"
    );
    assert_eq!(v["name"].as_str().unwrap(), "audit");
}

#[test]
fn typed_form_chained_derives_target_from_turbofish() {
    // `BaseV1::<LeafV1>` carries `LeafV1` as the deepest type arg, so the
    // const-assert target is derived as `LeafV1` and the literal must
    // match `<LeafV1 as GtsSchema>::SCHEMA_ID` (the full chain prefix).
    let v: BaseV1<LeafV1> = gts_instance!(BaseV1::<LeafV1> {
        id: "gts.acme.core.test.base.v1~acme.core.test.leaf.v1~vendor.app.things.example.v1",
        payload: LeafV1 {
            name: "ex".to_owned()
        },
    });

    assert_eq!(
        v.id.as_ref(),
        "gts.acme.core.test.base.v1~acme.core.test.leaf.v1~vendor.app.things.example.v1"
    );
}

#[test]
fn typed_form_three_level_chain_picks_deepest_generic() {
    // `L1OuterV1::<L2MidV1<L3LeafV1>>` — the macro must descend through the
    // intermediate generic carrier `L2MidV1<L3LeafV1>` and land on `L3LeafV1`,
    // whose `SCHEMA_ID` matches the full-chain prefix. Targeting `L1OuterV1`
    // would reject (extra `~` in suffix); targeting `L2MidV1<L3LeafV1>` would
    // also reject (its `SCHEMA_ID` is the L1~L2~ prefix, leaving the L3
    // segment in the suffix and tripping the no-tilde-in-segment check).
    let v: L1OuterV1<L2MidV1<L3LeafV1>> = gts_instance!(L1OuterV1::<L2MidV1<L3LeafV1>> {
        id: "gts.acme.core.test.l1.v1~acme.core.test.l2.v1~acme.core.test.l3.v1~vendor.app.things.deep.v1",
        payload: L2MidV1 {
            data: L3LeafV1 {
                value: "deep".to_owned()
            }
        },
    });

    assert_eq!(
        v.id.as_ref(),
        "gts.acme.core.test.l1.v1~acme.core.test.l2.v1~acme.core.test.l3.v1~vendor.app.things.deep.v1"
    );
    assert_eq!(v.payload.data.value, "deep");
}

#[test]
fn typed_form_unit_param_keeps_carrier_as_target() {
    // `BaseV1::<()>` denotes a base-level instance — the descent stops on
    // `()` (no `SCHEMA_ID`) and the carrier is kept as the target.
    let v: BaseV1<()> = gts_instance!(BaseV1::<()> {
        id: "gts.acme.core.test.base.v1~vendor.app.things.bare.v1",
        payload: (),
    });

    assert_eq!(
        v.id.as_ref(),
        "gts.acme.core.test.base.v1~vendor.app.things.bare.v1"
    );
}

// =====================================================================
//             gts_instance! — #[gts_static(NAME)] item form
// =====================================================================

gts_instance! {
    #[gts_static(ORDERS_TOPIC)]
    TopicV1 {
        id: "gts.acme.core.events.topic.v1~vendor.app.orders.created.v1",
        name: "orders".to_owned(),
        retention: "P30D".to_owned(),
    }
}

#[test]
fn static_form_exposes_typed_static_value() {
    let t: &TopicV1 = &ORDERS_TOPIC;
    assert_eq!(t.name, "orders");
    assert_eq!(t.retention, "P30D");
    assert_eq!(
        t.id.as_ref(),
        "gts.acme.core.events.topic.v1~vendor.app.orders.created.v1"
    );
}

#[test]
fn static_form_static_is_lazy_and_stable() {
    let first = &*ORDERS_TOPIC;
    let second = &*ORDERS_TOPIC;
    assert!(std::ptr::eq(first, second));
}

// `#[gts_static(NAME)]` over a chained generic carrier — auto-derivation
// resolves the const-assert target to `LeafV1`.

gts_instance! {
    #[gts_static(EXAMPLE_LEAF)]
    BaseV1::<LeafV1> {
        id: "gts.acme.core.test.base.v1~acme.core.test.leaf.v1~vendor.app.things.example.v1",
        payload: LeafV1 { name: "ex".to_owned() },
    }
}

#[test]
fn static_form_chained_carrier_with_auto_derivation() {
    let v: &BaseV1<LeafV1> = &EXAMPLE_LEAF;
    assert_eq!(
        v.id.as_ref(),
        "gts.acme.core.test.base.v1~acme.core.test.leaf.v1~vendor.app.things.example.v1"
    );
    assert_eq!(v.payload.name, "ex");
}

// =====================================================================
//                       gts_instance_raw! (JSON)
// =====================================================================

#[test]
fn raw_form_constructs_json_value_with_validated_id() {
    let v: serde_json::Value = gts_instance_raw!({
        "id": "gts.acme.core.events.topic.v1~vendor.app.events.audit.v1",
        "name": "audit",
        "description": "Audit log events"
    });

    assert_eq!(
        v["id"].as_str().unwrap(),
        "gts.acme.core.events.topic.v1~vendor.app.events.audit.v1"
    );
    assert_eq!(v["name"].as_str().unwrap(), "audit");
    assert_eq!(v["description"].as_str().unwrap(), "Audit log events");
}

#[test]
fn raw_form_supports_chained_instance_ids() {
    let v: serde_json::Value = gts_instance_raw!({
        "id": "gts.acme.core.test.base.v1~acme.core.test.leaf.v1~vendor.app.things.x.v1",
        "value": 42,
    });

    assert_eq!(
        v["id"].as_str().unwrap(),
        "gts.acme.core.test.base.v1~acme.core.test.leaf.v1~vendor.app.things.x.v1"
    );
    assert_eq!(v["value"].as_i64().unwrap(), 42);
}

#[test]
fn raw_form_supports_nested_objects_and_arrays() {
    // Top-level `id` is the only key the macro inspects; nested objects
    // and arrays pass through to `json!` untouched.
    let v: serde_json::Value = gts_instance_raw!({
        "id": "gts.acme.core.events.topic.v1~vendor.app.events.audit.v1",
        "tags": ["a", "b", "c"],
        "meta": { "id": "nested-not-touched", "count": 3 },
    });

    assert_eq!(
        v["id"].as_str().unwrap(),
        "gts.acme.core.events.topic.v1~vendor.app.events.audit.v1"
    );
    assert_eq!(v["tags"][1].as_str().unwrap(), "b");
    assert_eq!(v["meta"]["id"].as_str().unwrap(), "nested-not-touched");
    assert_eq!(v["meta"]["count"].as_i64().unwrap(), 3);
}

// =====================================================================
//          gts_instance! — alternative id field names (gts_id / gtsId)
// =====================================================================

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.acme.core.events.legacy_topic.v1~",
    description = "Legacy-style base struct using gts_id instead of id",
    properties = "gts_id,name"
)]
#[derive(Debug)]
pub struct LegacyTopicV1 {
    pub gts_id: GtsInstanceId,
    pub name: String,
}

#[test]
fn typed_form_picks_up_gts_id_field_automatically() {
    // Schema struct uses `gts_id` instead of `id` — the macro picks
    // whichever reserved id-field name appears in the literal.
    let t: LegacyTopicV1 = gts_instance!(LegacyTopicV1 {
        gts_id: "gts.acme.core.events.legacy_topic.v1~vendor.app.legacy.example.v1",
        name: "legacy".to_owned(),
    });

    assert_eq!(t.name, "legacy");
    assert_eq!(
        t.gts_id.as_ref(),
        "gts.acme.core.events.legacy_topic.v1~vendor.app.legacy.example.v1"
    );
}

// Same coverage for the camelCase alias `gtsId` — the third reserved name
// accepted by `struct_to_gts_schema`.

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.acme.core.events.legacy_topic_camel.v1~",
    description = "Legacy-style base struct using the gtsId camelCase alias",
    properties = "gtsId,name"
)]
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct LegacyTopicCamelV1 {
    pub gtsId: GtsInstanceId,
    pub name: String,
}

#[test]
fn typed_form_picks_up_gtsid_camel_case_field_automatically() {
    // gtsId is the camelCase alias accepted alongside id / gts_id; the
    // macro should pick it up identically without any extra modifier.
    let t: LegacyTopicCamelV1 = gts_instance!(LegacyTopicCamelV1 {
        gtsId: "gts.acme.core.events.legacy_topic_camel.v1~vendor.app.legacy.example.v1",
        name: "legacy".to_owned(),
    });

    assert_eq!(t.name, "legacy");
    assert_eq!(
        t.gtsId.as_ref(),
        "gts.acme.core.events.legacy_topic_camel.v1~vendor.app.legacy.example.v1"
    );
}
