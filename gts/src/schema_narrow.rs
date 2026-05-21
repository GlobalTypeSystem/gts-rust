//! Typed narrowing — go from a runtime-typed payload (commonly
//! `serde_json::Value`) to a typed GTS view `Q: GtsSchema`, validating
//! the runtime schema-id discriminator against the target's
//! `<Q as GtsSchema>::innermost_type_id()` and deserialising the
//! payload into `Q` via [`crate::GtsDeserializeWrapper`].
//!
//! The complementary "shape" to [`crate::schema_cast`]:
//!
//! - `schema_cast` converts an instance between **schema versions**
//!   (forward/backward compatibility, additive/removed properties).
//! - `schema_narrow` resolves a **typed Rust view** from an opaque
//!   runtime carrier — the schema versions are assumed identical;
//!   what's being chosen is which Rust type to materialise the payload
//!   as.
//!
//! Use this when a heterogeneous batch arrives as `Base<serde_json::Value>`
//! (e.g. a multi-provider model catalog, a multi-event audit log) and the
//! consumer wants to dispatch each item to a typed leaf — see
//! [`try_narrow`] for the canonical entry point, and the
//! `value_dispatch_tests` integration tests in `gts-macros` for an
//! end-to-end demonstration over 2-level and 3-level GTS chains.
//!
//! # Example
//!
//! ```ignore
//! use gts::{try_narrow, GtsSchema};
//!
//! let actual_id = envelope.gts_type.as_ref();       // read off the data
//! let payload   = envelope.payload;                  // serde_json::Value
//! let typed: AlphaLeafV1 = try_narrow::<AlphaLeafV1>(actual_id, payload)?;
//! ```

use serde_json::Value;
use thiserror::Error;

use crate::schema::{GtsDeserialize, GtsDeserializeWrapper, GtsSchema};

/// Error returned by typed-narrowing helpers when a runtime-typed value
/// cannot be narrowed into a target type `Q: GtsSchema`.
///
/// Two distinct failure modes:
///
/// - [`NarrowError::SchemaId`] — the runtime discriminator (e.g. an
///   `info.gts_type` / `event_type` field on an envelope) does not match
///   the target type's `<Q as GtsSchema>::innermost_type_id()`. The
///   data is for a different leaf than the caller asked for.
/// - [`NarrowError::Deserialize`] — the schema id matched, but the JSON
///   payload failed to deserialize into `Q`'s shape. Indicates malformed
///   or out-of-spec data — not an open-set extension point.
#[derive(Debug, Error)]
pub enum NarrowError {
    /// Discriminator field mismatch.
    #[error("gts schema id mismatch: expected `{expected}`, got `{actual}`")]
    SchemaId {
        /// `<Q as GtsSchema>::innermost_type_id()` — what [`try_narrow`]
        /// expected to find on the data.
        expected: String,
        /// What the data actually carried.
        actual: String,
    },

    /// JSON payload deserialization into the target shape failed.
    #[error("gts payload deserialization failed: {0}")]
    Deserialize(#[from] serde_json::Error),
}

/// Narrow a JSON payload into a typed GTS view `Q`, after validating
/// `actual_schema_id` against `<Q as GtsSchema>::innermost_type_id()`.
///
/// `actual_schema_id` is the runtime discriminator (a string field on
/// the envelope, e.g. `gts_type` / `event_type`) the consumer reads off
/// the data. `payload` is the raw JSON value carrying the leaf's typed
/// data — typically a `serde_json::Value` extracted from an
/// `Envelope<serde_json::Value>` after the upstream
/// `impl GtsSchema for serde_json::Value`.
///
/// # Type parameter `Q`
///
/// `Q` is the target Rust type — either a flat leaf
/// (e.g. `AlphaLeafV1`) or a composed view through one or more
/// intermediates (e.g. `Intermediate<BetaLeafV1>`). Internally:
///
/// - `<Q as GtsSchema>::innermost_type_id()` is used to compute the
///   expected id (walks the chain so composed views resolve to the leaf
///   id, exact match is enforced).
/// - The payload is deserialized via [`GtsDeserializeWrapper<Q>`] so
///   macro-generated nested types (which do not impl `serde::Deserialize`
///   directly, only [`GtsDeserialize`]) work correctly.
///
/// # Errors
///
/// - [`NarrowError::SchemaId`] when the data's discriminator does not
///   match the target type's innermost id.
/// - [`NarrowError::Deserialize`] when the JSON payload does not match
///   `Q`'s shape.
///
/// # Ownership
///
/// `payload` is consumed on **both** the success and failure paths —
/// the function takes `Value` by value and does not return it back on
/// error. The canonical use is discriminator-first dispatch: read the
/// envelope's `gts_type`, `match` on it, then call `try_narrow` exactly
/// once with the chosen target. If you instead want to attempt several
/// targets in sequence without inspecting the discriminator first,
/// clone the payload before each call.
pub fn try_narrow<Q>(actual_schema_id: &str, payload: Value) -> Result<Q, NarrowError>
where
    Q: GtsSchema,
    for<'de> Q: GtsDeserialize<'de>,
{
    let expected = <Q as GtsSchema>::innermost_type_id();
    if actual_schema_id != expected {
        return Err(NarrowError::SchemaId {
            expected: expected.to_owned(),
            actual: actual_schema_id.to_owned(),
        });
    }
    let wrapper: GtsDeserializeWrapper<Q> = serde_json::from_value(payload)?;
    Ok(wrapper.0)
}
