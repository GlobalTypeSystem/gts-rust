//! Fixture crate for `overrides_harness_tests.rs`, built and run there with
//! `cargo test`: every `overrides(...)` type gets a generated `#[test]`.

#![allow(dead_code)]

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_harness.subject.v1~",
    description = "Subject",
    properties = "subject_type,id,payload"
)]
pub struct SubjectV1<P> {
    #[serde(rename = "type")]
    pub subject_type: gts::gts::GtsTypeId,
    #[schemars(length(max = 64))]
    pub id: String,
    pub payload: P,
}

/// No `overrides(...)`: no generated test.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.plain.v1~",
    description = "Plain",
    properties = ""
)]
pub struct PlainV1;

mod valid {
    use super::SubjectV1;
    use gts_macros::struct_to_gts_schema;
    use schemars::JsonSchema;

    /// Private helper.
    #[derive(JsonSchema)]
    struct ShortId {
        #[schemars(length(max = 8))]
        id: String,
    }

    /// Private generic type: tested as `GenericV1<()>`.
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = SubjectV1,
        type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.generic.v1~",
        description = "Generic",
        properties = "ext",
        overrides(SubjectV1 = ShortId)
    )]
    struct GenericV1<E> {
        ext: E,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = SubjectV1,
        type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.valid_unit.v1~",
        description = "Valid unit",
        properties = "",
        overrides(SubjectV1 = ShortId)
    )]
    struct UnitV1;
}

mod invalid {
    use super::SubjectV1;
    use gts_macros::struct_to_gts_schema;
    use schemars::JsonSchema;

    #[derive(JsonSchema)]
    struct WideId {
        #[schemars(length(max = 128))]
        id: String,
    }

    /// Same name as `valid::UnitV1`; widens `/id`, so its test fails.
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = SubjectV1,
        type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.invalid_unit.v1~",
        description = "Invalid unit",
        properties = "",
        overrides(SubjectV1 = WideId)
    )]
    struct UnitV1;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = SubjectV1,
        type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.invalid_parent.v1~",
        description = "Invalid parent",
        properties = "body",
        overrides(SubjectV1 = WideId)
    )]
    struct InvalidParentV1<P> {
        body: P,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = InvalidParentV1,
        type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.invalid_parent.v1~x.test.ovr_harness.child.v1~",
        description = "Inherits its parent's invalid override",
        properties = ""
    )]
    struct ChildV1;

    /// Generating an invalid override is independent of checking it.
    #[test]
    fn schema_generation_and_validation_are_separate() {
        let schema = <UnitV1 as gts::GtsSchema>::gts_schema_with_refs();
        assert_eq!(schema["allOf"][1]["properties"]["id"]["maxLength"], 128);
        let error = <UnitV1 as gts::GtsSchema>::validate_overrides().unwrap_err();
        assert!(error.to_string().contains("widens"));
        let _ = <ChildV1 as gts::GtsSchema>::gts_schema_with_refs();
        assert!(<ChildV1 as gts::GtsSchema>::validate_overrides().is_err());
    }
}

/// Overrides that strict replacement admits but the registry's derivation
/// admission (OP#12) rejects: the generated test must fail on them too.
mod admission {
    use gts_macros::struct_to_gts_schema;
    use schemars::JsonSchema;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema)]
    pub struct Pair {
        a: String,
        b: Option<String>,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = "gts.x.test.ovr_harness.pair.v1~",
        description = "Pair subject",
        properties = "subject_type,pair,payload"
    )]
    pub struct PairSubjectV1<P> {
        #[serde(rename = "type")]
        pub subject_type: gts::gts::GtsTypeId,
        pub pair: Pair,
        pub payload: P,
    }

    /// Restates `b` and narrows `a`.
    #[derive(JsonSchema)]
    #[schemars(deny_unknown_fields)]
    struct NarrowPair {
        #[schemars(length(max = 3))]
        a: String,
        b: Option<String>,
    }

    #[derive(JsonSchema)]
    struct NarrowRules {
        pair: NarrowPair,
    }

    /// Closed without restating the inherited optional `b`: included in the
    /// inherited declaration, but OP#12 rejects a closed branch that leaves
    /// an ancestor property unusable.
    #[derive(JsonSchema)]
    #[schemars(deny_unknown_fields)]
    struct ClosedPair {
        a: String,
    }

    #[derive(JsonSchema)]
    struct ClosedRules {
        pair: ClosedPair,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = PairSubjectV1,
        type_id = "gts.x.test.ovr_harness.pair.v1~x.test.ovr_harness.narrow.v1~",
        description = "Narrowed pair",
        properties = "ext",
        overrides(PairSubjectV1 = NarrowRules)
    )]
    struct NarrowV1<E> {
        ext: E,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = PairSubjectV1,
        type_id = "gts.x.test.ovr_harness.pair.v1~x.test.ovr_harness.closed.v1~",
        description = "Closed pair",
        properties = "ext",
        overrides(PairSubjectV1 = ClosedRules)
    )]
    struct ClosedV1<E> {
        ext: E,
    }

    /// No own overrides: checked through its chain, own level included.
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = NarrowV1,
        type_id = "gts.x.test.ovr_harness.pair.v1~x.test.ovr_harness.narrow.v1~x.test.ovr_harness.narrow_child.v1~",
        description = "Child of a valid override",
        properties = ""
    )]
    struct NarrowChildV1;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = ClosedV1,
        type_id = "gts.x.test.ovr_harness.pair.v1~x.test.ovr_harness.closed.v1~x.test.ovr_harness.closed_child.v1~",
        description = "Child of a rejected override",
        properties = ""
    )]
    struct ClosedChildV1;

    /// Hand-written parent: its chain declares overrides and its generic
    /// slot is closed, so a child that adds a field there is not admitted.
    /// The macro never emits such a slot; this isolates the child's own pair.
    pub struct ClosedSlotV1<P>(std::marker::PhantomData<P>);

    impl<P> gts::GtsSchema for ClosedSlotV1<P> {
        const TYPE_ID: &'static str = "gts.x.test.ovr_harness.closed_slot.v1~";
        const GENERIC_FIELD: Option<&'static str> = Some("payload");
        const GTS_CHAIN_OVERRIDES: bool = true;

        fn gts_schema_with_refs() -> serde_json::Value {
            serde_json::json!({
                "$id": "gts://gts.x.test.ovr_harness.closed_slot.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {
                    "payload": { "type": "object", "additionalProperties": false }
                },
                "required": ["payload"]
            })
        }
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = ClosedSlotV1,
        type_id = "gts.x.test.ovr_harness.closed_slot.v1~x.test.ovr_harness.slot_child.v1~",
        description = "Adds a field to a closed slot",
        properties = "x"
    )]
    struct SlotChildV1 {
        x: String,
    }

    #[test]
    fn a_descendant_without_overrides_checks_its_own_level() {
        use gts::GtsSchema;

        <ClosedSlotV1<()>>::validate_overrides().unwrap();
        let error = SlotChildV1::validate_overrides().unwrap_err();
        assert_eq!(error.ancestor, "gts.x.test.ovr_harness.closed_slot.v1~");
        let error = error.to_string();
        assert!(error.contains("derivation admission"), "{error}");
        assert!(error.contains("slot_child"), "{error}");
    }

    #[test]
    fn descendants_validate_their_whole_chain() {
        use gts::GtsSchema;

        assert!(!<PairSubjectV1<()>>::GTS_CHAIN_OVERRIDES);
        assert!(NarrowChildV1::GTS_CHAIN_OVERRIDES);
        let chain = NarrowChildV1::gts_schema_chain();
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[2], NarrowChildV1::gts_schema_with_refs_allof());
        NarrowChildV1::validate_overrides().unwrap();

        let error = ClosedChildV1::validate_overrides().unwrap_err().to_string();
        assert!(error.contains("derivation admission"), "{error}");
        assert!(error.contains("pair.b"), "{error}");
    }
}

/// libtest cannot reach a test inside a fn body: the generated test must
/// fail the test build instead of being skipped.
#[cfg(feature = "local")]
fn local_types() {
    use gts_macros::struct_to_gts_schema;
    use schemars::JsonSchema;

    #[derive(JsonSchema)]
    struct WideId {
        #[schemars(length(max = 128))]
        id: String,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = SubjectV1,
        type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.local.v1~",
        description = "Local",
        properties = "",
        overrides(SubjectV1 = WideId)
    )]
    struct LocalV1;
}

/// A local type shadowing a valid module-level one of the same name: the
/// module's generated test must not stand in for the local type's.
#[cfg(feature = "shadow")]
mod shadow {
    use super::SubjectV1;
    use gts_macros::struct_to_gts_schema;
    use schemars::JsonSchema;

    #[derive(JsonSchema)]
    struct ShortId {
        #[schemars(length(max = 8))]
        id: String,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = SubjectV1,
        type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.shadowed.v1~",
        description = "Valid, module level",
        properties = "",
        overrides(SubjectV1 = ShortId)
    )]
    struct ChildV1;

    fn shadowed_type() {
        #[derive(JsonSchema)]
        struct WideId {
            #[schemars(length(max = 128))]
            id: String,
        }

        #[struct_to_gts_schema(
            dir_path = "schemas",
            base = SubjectV1,
            type_id = "gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.shadowing.v1~",
            description = "Invalid, local",
            properties = "",
            overrides(SubjectV1 = WideId)
        )]
        struct ChildV1;
    }
}

#[cfg(test)]
mod chain;

#[cfg(test)]
mod nullable;

#[cfg(test)]
mod entity_refs;
