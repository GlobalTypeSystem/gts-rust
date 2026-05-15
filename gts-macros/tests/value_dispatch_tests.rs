//! Tests for [`GtsSchema`] on [`serde_json::Value`] — heterogeneous
//! runtime dispatch over 2-level and 3-level GTS chains.
//!
//! The pattern: a base type `EnvelopeV1<P>` carries a `gts_type` field
//! (the runtime discriminator) plus a generic `payload: P` field. The
//! default `P = Value` lets callers transport mixed-provider envelopes
//! through type-erased layers (lists, channels, RPC boundaries). When a
//! caller wants typed access, they match `gts_type` against
//! `<TargetLeaf>::innermost_schema_id()` and re-deserialise the JSON
//! payload into the chosen typed shape — either a direct leaf
//! (`AlphaLeafV1`, `GammaLeafV1`) or a composed view through an
//! intermediate (`IntermediateV1<BetaLeafV1>` — the 3-level case).
//!
//! Mirrors the canonical consumer-side dispatcher pattern that consumes
//! `Vec<Model<serde_json::Value>>` in downstream SDKs.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]

use gts::gts::GtsSchemaId;
use gts::{GtsSchema, NarrowError, try_narrow};
use gts_macros::struct_to_gts_schema;

// =============================================================================
// Type hierarchy
//   EnvelopeV1<P>                         (level-1 base)
//     ├─ AlphaLeafV1                      (level-2 direct leaf)
//     ├─ GammaLeafV1                      (level-2 direct leaf)
//     └─ IntermediateV1<Q>                (level-2 intermediate)
//          └─ BetaLeafV1                  (level-3 leaf)
// =============================================================================

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.test.value_dispatch.envelope.v1~",
    description = "EnvelopeV1 carrying an opaque (default) or typed payload",
    properties = "gts_type,payload"
)]
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopeV1<P> {
    pub gts_type: GtsSchemaId,
    pub payload: P,
}

// ── Level-2 direct leaf #1 ───────────────────────────────────────────────────

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EnvelopeV1,
    schema_id = "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.alpha.v1~",
    description = "Alpha leaf — directly under EnvelopeV1",
    properties = "alpha_data"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlphaLeafV1 {
    pub alpha_data: String,
}

// ── Level-2 direct leaf #2 ───────────────────────────────────────────────────

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EnvelopeV1,
    schema_id = "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.gamma.v1~",
    description = "Gamma leaf — directly under EnvelopeV1, different shape from Alpha",
    properties = "gamma_count,gamma_flag"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GammaLeafV1 {
    pub gamma_count: u32,
    pub gamma_flag: bool,
}

// ── Level-2 intermediate (generic) ───────────────────────────────────────────

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EnvelopeV1,
    schema_id = "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.intermediate.v1~",
    description = "IntermediateV1 node — common fields plus a generic `extension` for level-3 leaves",
    properties = "common_label,extension"
)]
#[derive(Debug, Clone, PartialEq)]
pub struct IntermediateV1<Q = ()> {
    pub common_label: String,
    pub extension: Q,
}

// ── Level-3 leaf (under IntermediateV1) ────────────────────────────────────────

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = IntermediateV1,
    schema_id = "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.intermediate.v1~x.test.value_dispatch.beta.v1~",
    description = "Beta leaf — under IntermediateV1, 3 segments deep",
    properties = "beta_value"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaLeafV1 {
    pub beta_value: i64,
}

// =============================================================================
// Sanity: schema id constants behave as the macro contract documents
// =============================================================================

#[cfg(test)]
mod schema_id_contract {
    use super::*;

    const ALPHA_CHAIN: &str =
        "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.alpha.v1~";
    const GAMMA_CHAIN: &str =
        "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.gamma.v1~";
    const INTERMEDIATE_CHAIN: &str =
        "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.intermediate.v1~";
    const BETA_CHAIN: &str = "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.intermediate.v1~x.test.value_dispatch.beta.v1~";

    #[test]
    fn unit_and_value_are_both_empty_id_placeholders() {
        assert_eq!(<() as GtsSchema>::SCHEMA_ID, "");
        assert_eq!(<serde_json::Value as GtsSchema>::SCHEMA_ID, "");
        assert_eq!(<() as GtsSchema>::innermost_schema_id(), "");
        assert_eq!(<serde_json::Value as GtsSchema>::innermost_schema_id(), "");
    }

    #[test]
    fn envelope_with_value_payload_keeps_envelope_id() {
        // When the payload is a placeholder (Value or ()), the composed
        // type's innermost id falls back to the envelope's own literal
        // — i.e. nothing further is known about the leaf at type level.
        assert_eq!(
            <EnvelopeV1<serde_json::Value> as GtsSchema>::SCHEMA_ID,
            "gts.x.test.value_dispatch.envelope.v1~"
        );
        assert_eq!(
            <EnvelopeV1<serde_json::Value> as GtsSchema>::innermost_schema_id(),
            "gts.x.test.value_dispatch.envelope.v1~",
            "Value-tail innermost is the envelope's own literal"
        );
        assert_eq!(
            <EnvelopeV1<()> as GtsSchema>::innermost_schema_id(),
            "gts.x.test.value_dispatch.envelope.v1~",
            "(): same protocol -- empty tail collapses to the envelope's literal"
        );
    }

    #[test]
    fn direct_leaves_carry_their_two_segment_chain_via_innermost() {
        assert_eq!(<AlphaLeafV1 as GtsSchema>::SCHEMA_ID, ALPHA_CHAIN);
        assert_eq!(
            <AlphaLeafV1 as GtsSchema>::innermost_schema_id(),
            ALPHA_CHAIN
        );
        assert_eq!(<GammaLeafV1 as GtsSchema>::SCHEMA_ID, GAMMA_CHAIN);
        assert_eq!(
            <GammaLeafV1 as GtsSchema>::innermost_schema_id(),
            GAMMA_CHAIN
        );

        // Composed views: EnvelopeV1<AlphaLeafV1>::SCHEMA_ID is the
        // envelope's literal (does not know about its generic), but
        // innermost walks the chain and returns AlphaLeafV1's id.
        assert_eq!(
            <EnvelopeV1<AlphaLeafV1> as GtsSchema>::SCHEMA_ID,
            "gts.x.test.value_dispatch.envelope.v1~"
        );
        assert_eq!(
            <EnvelopeV1<AlphaLeafV1> as GtsSchema>::innermost_schema_id(),
            ALPHA_CHAIN
        );
    }

    #[test]
    fn three_level_chain_resolves_to_leaf_via_innermost() {
        assert_eq!(<BetaLeafV1 as GtsSchema>::SCHEMA_ID, BETA_CHAIN);
        assert_eq!(<BetaLeafV1 as GtsSchema>::innermost_schema_id(), BETA_CHAIN);

        // IntermediateV1 is the 2-level node; its `SCHEMA_ID` is its own
        // 2-segment id regardless of what fills the `extension` slot.
        assert_eq!(
            <IntermediateV1<()> as GtsSchema>::SCHEMA_ID,
            INTERMEDIATE_CHAIN
        );
        assert_eq!(
            <IntermediateV1<serde_json::Value> as GtsSchema>::SCHEMA_ID,
            INTERMEDIATE_CHAIN
        );

        // Walking innermost through IntermediateV1<BetaLeafV1> gives the
        // 3-segment chain (the leaf).
        assert_eq!(
            <IntermediateV1<BetaLeafV1> as GtsSchema>::innermost_schema_id(),
            BETA_CHAIN
        );
        assert_eq!(
            <EnvelopeV1<IntermediateV1<BetaLeafV1>> as GtsSchema>::innermost_schema_id(),
            BETA_CHAIN
        );

        // Whereas IntermediateV1<()> "loses" the leaf — its innermost is
        // the intermediate's own id. This documents that narrowing to
        // "intermediate-only" via innermost_schema_id is intentionally
        // distinct from narrowing to a leaf.
        assert_eq!(
            <IntermediateV1<()> as GtsSchema>::innermost_schema_id(),
            INTERMEDIATE_CHAIN
        );
        assert_eq!(
            <EnvelopeV1<IntermediateV1<()>> as GtsSchema>::innermost_schema_id(),
            INTERMEDIATE_CHAIN
        );
    }
}

// =============================================================================
// Deserialisation: EnvelopeV1<Value> as the runtime carrier
// =============================================================================

#[cfg(test)]
mod deserialisation {
    use super::*;

    #[test]
    fn envelope_of_value_round_trips_arbitrary_payload() {
        // Wire shape: gts_type discriminator + opaque JSON payload.
        let wire = serde_json::json!({
            "gts_type": "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.alpha.v1~",
            "payload": { "alpha_data": "hello" }
        });

        // Round-trip through EnvelopeV1<Value>.
        let envelope: EnvelopeV1<serde_json::Value> =
            serde_json::from_value(wire.clone()).expect("deserialize EnvelopeV1<Value>");
        assert_eq!(
            envelope.gts_type.as_ref(),
            "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.alpha.v1~"
        );
        assert_eq!(
            envelope.payload,
            serde_json::json!({ "alpha_data": "hello" })
        );

        // Re-serialize and confirm the wire shape is preserved (object
        // equality, not byte equality — serde_json::Value compares
        // structurally).
        let round_tripped = serde_json::to_value(&envelope).expect("serialize");
        assert_eq!(round_tripped, wire);
    }

    #[test]
    fn payload_field_is_carried_through_serialize_gts_unchanged_for_value() {
        // The macro wraps the generic field with `serialize_gts` /
        // `deserialize_gts`. For Value, those go through the blanket
        // impl over `serde::Serialize` / `serde::Deserialize`, so they
        // are effectively identity wrappers — the payload survives
        // any nesting without mangling.
        let envelope = EnvelopeV1::<serde_json::Value> {
            gts_type: GtsSchemaId::new("gts.x.test.value_dispatch.envelope.v1~x.test.unknown.v1~"),
            payload: serde_json::json!({
                "anything": ["the", "future", 42, null, { "nested": true }]
            }),
        };
        let v = serde_json::to_value(&envelope).unwrap();
        assert_eq!(v["payload"]["anything"][2], serde_json::json!(42));

        // Round-trip back.
        let back: EnvelopeV1<serde_json::Value> = serde_json::from_value(v).unwrap();
        assert_eq!(
            back.payload["anything"][4]["nested"],
            serde_json::json!(true)
        );
    }
}

// =============================================================================
// Dispatch: choose the typed leaf at runtime by matching gts_type
// =============================================================================

#[cfg(test)]
mod dispatch {
    use super::*;

    /// Per-envelope decode result. Each known variant carries the **full
    /// typed envelope** (`EnvelopeV1<TypedLeaf>` / `EnvelopeV1<Intermediate<TypedLeaf>>`),
    /// not just the payload — `gts_type` discrimination, the leaf-specific
    /// payload, and any envelope-level metadata are all preserved in the
    /// same shape as the input wire. The open-set `Unknown` branch keeps
    /// the un-narrowed `EnvelopeV1<serde_json::Value>` so callers can still
    /// inspect / pass it through.
    #[derive(Debug, PartialEq)]
    enum Decoded {
        Alpha(EnvelopeV1<AlphaLeafV1>),
        Gamma(EnvelopeV1<GammaLeafV1>),
        BetaUnderIntermediate(EnvelopeV1<IntermediateV1<BetaLeafV1>>),
        Unknown(EnvelopeV1<serde_json::Value>),
    }

    /// Take a runtime-typed `EnvelopeV1<Value>` and produce a `Decoded`
    /// variant by looking up the leaf type from `gts_type`. This is the
    /// canonical dispatcher: matches `gts_type` against each known
    /// target's `innermost_schema_id()`, narrows the payload only for the
    /// chosen branch, and re-wraps it as a fully-typed envelope so the
    /// caller never has to reach into raw JSON for `gts_type` again.
    fn decode(envelope: EnvelopeV1<serde_json::Value>) -> Decoded {
        // Destructure once so each match arm can move `gts_type` and
        // `payload` independently (or re-wrap them into a new typed
        // envelope in the chosen branch).
        let EnvelopeV1 { gts_type, payload } = envelope;
        let actual = gts_type.as_ref().to_owned();
        match actual.as_str() {
            id if id == <AlphaLeafV1 as GtsSchema>::innermost_schema_id() => {
                let typed: AlphaLeafV1 = try_narrow(id, payload).expect("alpha payload");
                Decoded::Alpha(EnvelopeV1 {
                    gts_type,
                    payload: typed,
                })
            }
            id if id == <GammaLeafV1 as GtsSchema>::innermost_schema_id() => {
                let typed: GammaLeafV1 = try_narrow(id, payload).expect("gamma payload");
                Decoded::Gamma(EnvelopeV1 {
                    gts_type,
                    payload: typed,
                })
            }
            id if id
                == <EnvelopeV1<IntermediateV1<BetaLeafV1>> as GtsSchema>::innermost_schema_id() =>
            {
                let typed: IntermediateV1<BetaLeafV1> =
                    try_narrow(id, payload).expect("intermediate payload");
                Decoded::BetaUnderIntermediate(EnvelopeV1 {
                    gts_type,
                    payload: typed,
                })
            }
            _ => Decoded::Unknown(EnvelopeV1 { gts_type, payload }),
        }
    }

    #[test]
    fn dispatches_heterogeneous_batch_across_2_and_3_level_chains() {
        const ALPHA_CHAIN: &str =
            "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.alpha.v1~";
        const GAMMA_CHAIN: &str =
            "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.gamma.v1~";
        const BETA_CHAIN: &str = "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.intermediate.v1~x.test.value_dispatch.beta.v1~";

        // A batch with two direct leaves, one 3-level chain, and one
        // gts_type the dispatcher was not built against — exactly the
        // shape a consumer would see coming out of a multi-tenant
        // catalog with mixed providers.
        let inputs: Vec<serde_json::Value> = vec![
            serde_json::json!({
                "gts_type": "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.alpha.v1~",
                "payload": { "alpha_data": "first" }
            }),
            serde_json::json!({
                "gts_type": "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.gamma.v1~",
                "payload": { "gamma_count": 7, "gamma_flag": true }
            }),
            // 3-level: payload contains intermediate's common_label plus
            // the leaf nested in `extension` (the generic-field path).
            serde_json::json!({
                "gts_type": "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.intermediate.v1~x.test.value_dispatch.beta.v1~",
                "payload": {
                    "common_label": "shared",
                    "extension": { "beta_value": 99 }
                }
            }),
            // A future / unknown gts_type — survives dispatch via the
            // open-set Unknown branch.
            serde_json::json!({
                "gts_type": "gts.x.test.value_dispatch.envelope.v1~x.test.unmodelled_future.v1~",
                "payload": { "anything": "goes" }
            }),
        ];

        let decoded: Vec<Decoded> = inputs
            .into_iter()
            .map(|j| {
                serde_json::from_value::<EnvelopeV1<serde_json::Value>>(j)
                    .expect("EnvelopeV1<Value> deserialization")
            })
            .map(decode)
            .collect();

        assert_eq!(
            decoded[0],
            Decoded::Alpha(EnvelopeV1 {
                gts_type: GtsSchemaId::new(ALPHA_CHAIN),
                payload: AlphaLeafV1 {
                    alpha_data: "first".into()
                },
            })
        );
        assert_eq!(
            decoded[1],
            Decoded::Gamma(EnvelopeV1 {
                gts_type: GtsSchemaId::new(GAMMA_CHAIN),
                payload: GammaLeafV1 {
                    gamma_count: 7,
                    gamma_flag: true,
                },
            })
        );
        assert_eq!(
            decoded[2],
            Decoded::BetaUnderIntermediate(EnvelopeV1 {
                gts_type: GtsSchemaId::new(BETA_CHAIN),
                payload: IntermediateV1 {
                    common_label: "shared".into(),
                    extension: BetaLeafV1 { beta_value: 99 },
                },
            })
        );
        match &decoded[3] {
            Decoded::Unknown(env) => {
                assert_eq!(
                    env.gts_type.as_ref(),
                    "gts.x.test.value_dispatch.envelope.v1~x.test.unmodelled_future.v1~"
                );
                // Payload survives intact as the original JSON Value:
                assert_eq!(env.payload, serde_json::json!({ "anything": "goes" }));
            }
            other => panic!("expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn dispatch_can_inspect_common_fields_before_deciding_to_narrow() {
        // Common fields (here: `gts_type`) on `EnvelopeV1<Value>` are
        // accessible WITHOUT narrowing — that's the whole point of
        // keeping Value as the default `P`.
        let wire = serde_json::json!({
            "gts_type": "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.alpha.v1~",
            "payload": { "alpha_data": "no narrowing needed" }
        });
        let envelope: EnvelopeV1<serde_json::Value> = serde_json::from_value(wire).unwrap();

        // Read common discriminator without consuming the envelope.
        assert!(
            envelope
                .gts_type
                .as_ref()
                .ends_with("x.test.value_dispatch.alpha.v1~")
        );

        // Only NOW do we narrow.
        let typed = decode(envelope);
        assert!(matches!(typed, Decoded::Alpha(_)));
    }
}

// =============================================================================
// `try_narrow` helper + `NarrowError` variants — focused tests
// =============================================================================

#[cfg(test)]
mod narrow_helper {
    use super::*;

    const ALPHA_CHAIN: &str =
        "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.alpha.v1~";
    const BETA_CHAIN: &str = "gts.x.test.value_dispatch.envelope.v1~x.test.value_dispatch.intermediate.v1~x.test.value_dispatch.beta.v1~";

    #[test]
    fn try_narrow_succeeds_on_matching_chain_for_2_level_leaf() {
        let payload = serde_json::json!({ "alpha_data": "ok" });
        let typed: AlphaLeafV1 =
            try_narrow(ALPHA_CHAIN, payload).expect("matching chain should narrow");
        assert_eq!(typed.alpha_data, "ok");
    }

    #[test]
    fn try_narrow_succeeds_on_matching_chain_for_3_level_composed_view() {
        // The payload shape matches the composed view: intermediate's
        // common fields at top level, leaf nested under `extension`.
        let payload = serde_json::json!({
            "common_label": "shared",
            "extension": { "beta_value": 7 }
        });
        let typed: IntermediateV1<BetaLeafV1> =
            try_narrow(BETA_CHAIN, payload).expect("matching chain should narrow");
        assert_eq!(typed.common_label, "shared");
        assert_eq!(typed.extension.beta_value, 7);
    }

    #[test]
    fn try_narrow_returns_schema_id_mismatch_for_wrong_chain() {
        let payload = serde_json::json!({ "alpha_data": "irrelevant" });
        let err = try_narrow::<AlphaLeafV1>(BETA_CHAIN, payload)
            .expect_err("alpha target vs beta chain - must mismatch");
        match err {
            NarrowError::SchemaId { expected, actual } => {
                assert_eq!(expected, ALPHA_CHAIN);
                assert_eq!(actual, BETA_CHAIN);
            }
            other @ NarrowError::Deserialize(_) => {
                panic!("expected SchemaId variant, got {other:?}")
            }
        }
    }

    #[test]
    fn try_narrow_returns_deserialize_error_for_malformed_payload() {
        // Chain matches AlphaLeafV1, but payload is missing the required
        // `alpha_data` field - schema-id pre-check passes, deserialize
        // fails downstream.
        let payload = serde_json::json!({ "wrong_key": 42 });
        let err = try_narrow::<AlphaLeafV1>(ALPHA_CHAIN, payload)
            .expect_err("missing required field - must surface deserialize error");
        assert!(matches!(err, NarrowError::Deserialize(_)));
    }

    #[test]
    fn try_narrow_into_placeholder_value_is_always_allowed() {
        // `Value`'s innermost_schema_id() is "" (placeholder protocol);
        // so passing any chain string is a SchemaId mismatch UNLESS the
        // caller explicitly passes "" - which is the protocol-correct
        // way to say "I don't care about narrowing, just pull the JSON".
        // This documents the boundary of the helper - it's a strict
        // exact-match operation.
        let payload = serde_json::json!({ "anything": "goes" });
        let value: serde_json::Value = try_narrow("", payload.clone()).expect("empty id matches");
        assert_eq!(value, payload);

        // Non-empty actual_id with target=Value still fails:
        let err = try_narrow::<serde_json::Value>(ALPHA_CHAIN, serde_json::json!({}))
            .expect_err("Value target requires empty id");
        assert!(matches!(err, NarrowError::SchemaId { .. }));
    }

    #[test]
    fn try_narrow_returns_fully_typed_envelope_in_one_call() {
        // The "fat" narrowing target: a 3-level chain wrapped in the
        // envelope. `try_narrow::<EnvelopeV1<IntermediateV1<BetaLeafV1>>>`
        // takes the FULL envelope JSON and produces a fully-typed
        // `EnvelopeV1<IntermediateV1<BetaLeafV1>>` in a single call.
        //
        // `<EnvelopeV1<IntermediateV1<BetaLeafV1>> as GtsSchema>::innermost_schema_id()`
        // walks the chain through both generics — Envelope -> Intermediate
        // -> Beta — and lands on `BETA_CHAIN`. The runtime discriminator
        // we read off `gts_type` is also `BETA_CHAIN`, so the schema-id
        // pre-check passes and the whole JSON deserialises into the
        // composed type.
        let full_envelope_json = serde_json::json!({
            "gts_type": BETA_CHAIN,
            "payload": {
                "common_label": "shared-via-envelope",
                "extension": { "beta_value": 17 }
            }
        });

        // In a real dispatcher the caller may not know the chain id ahead
        // of time — they'd peek at the wire to read it before choosing a
        // narrow target. Demonstrate that path:
        let peeked_id = full_envelope_json
            .get("gts_type")
            .and_then(serde_json::Value::as_str)
            .expect("gts_type present on the wire")
            .to_owned();

        let typed: EnvelopeV1<IntermediateV1<BetaLeafV1>> =
            try_narrow(&peeked_id, full_envelope_json).expect("whole-envelope narrow succeeds");

        // gts_type carried through, plus both common-intermediate and
        // leaf-specific fields are typed and reachable through one
        // chain of accesses — no second `try_narrow` needed:
        assert_eq!(typed.gts_type.as_ref(), BETA_CHAIN);
        assert_eq!(typed.payload.common_label, "shared-via-envelope");
        assert_eq!(typed.payload.extension.beta_value, 17);
    }

    #[test]
    fn try_narrow_whole_envelope_fails_on_id_mismatch() {
        // Same fat target as above, but the wire claims a different leaf
        // id — narrowing must reject before attempting deserialize, since
        // the resulting typed view would otherwise silently misinterpret
        // the data.
        let full_envelope_json = serde_json::json!({
            "gts_type": ALPHA_CHAIN,
            "payload": {
                "common_label": "doesn't matter",
                "extension": { "beta_value": 0 }
            }
        });
        let err =
            try_narrow::<EnvelopeV1<IntermediateV1<BetaLeafV1>>>(ALPHA_CHAIN, full_envelope_json)
                .expect_err("wire says alpha, target expects beta chain - mismatch");
        match err {
            NarrowError::SchemaId { expected, actual } => {
                assert_eq!(expected, BETA_CHAIN);
                assert_eq!(actual, ALPHA_CHAIN);
            }
            other @ NarrowError::Deserialize(_) => {
                panic!("expected SchemaId variant, got {other:?}")
            }
        }
    }

    #[test]
    fn try_narrow_unwrapping_a_value_envelope_is_two_steps() {
        // Realistic dispatcher pattern: first deserialize the outer
        // Envelope<Value>, then `try_narrow` its payload into the
        // target leaf using `gts_type` as the actual id.
        let wire = serde_json::json!({
            "gts_type": ALPHA_CHAIN,
            "payload": { "alpha_data": "stepped" }
        });
        let env: EnvelopeV1<serde_json::Value> = serde_json::from_value(wire).unwrap();
        let leaf: AlphaLeafV1 =
            try_narrow(env.gts_type.as_ref(), env.payload).expect("two-step narrow");
        assert_eq!(leaf.alpha_data, "stepped");
    }
}
