//! GTS Type Schema Traits — Rust-side helper for building the inline
//! `x-gts-traits-schema` fragment.
//!
//! A trait shape can be supplied two ways:
//!
//! - **inline** — a private object subschema embedded directly under
//!   `x-gts-traits-schema`. Produced from any `#[derive(schemars::JsonSchema)]`
//!   struct via [`inline_traits_schema_of`] (the macro emits
//!   `traits_schema = inline(MyStruct)`).
//! - **referenced** — a reusable trait-schema registered as an ordinary GTS
//!   type, pulled in via `$ref`. The macro emits this for `traits_schema = T`
//!   where `T` is a `#[struct_to_gts_schema]` type, as
//!   `{ "type": "object", "allOf": [{ "$ref": "gts://<TYPE_ID>" }] }`.
//!
//! `const`, `default` and `x-gts-ref` on trait properties are expressed with
//! standard schemars/serde attributes (`#[schemars(extend("const" = ...))]`,
//! `#[serde(default = "...")]`, `#[schemars(extend("x-gts-ref" = "..."))]`), so
//! no GTS-specific field attributes are needed.

use serde_json::Value;

use crate::gts::{GtsInstanceId, GtsTypeId};

/// Opt-in marker for a struct that backs an inline `x-gts-traits-schema`.
///
/// Implement it by adding `GtsTraitsSchema` to a struct's `#[derive(...)]` list
/// (the derive macro lives in `gts_macros`), alongside `schemars::JsonSchema`.
/// It is the bound `#[struct_to_gts_schema(..., traits_schema = inline(T))]`
/// requires of `T`, so a struct used in `inline(...)` without the derive fails
/// to compile — the same opt-in gate the `$ref` form already gets from
/// [`crate::GtsSchema`].
///
/// `JsonSchema` is a supertrait because the inline subschema is generated from
/// `T`'s `JsonSchema` impl at runtime (see [`inline_traits_schema_of`]); this
/// also means deriving `GtsTraitsSchema` without `JsonSchema` is a compile
/// error, mirroring `Eq: PartialEq`.
pub trait GtsTraitsSchema: schemars::JsonSchema {}

/// Build the inline `x-gts-traits-schema` object subschema for a `JsonSchema` type.
///
/// Returns the type's own JSON Schema with the root-only `$schema` annotation
/// stripped (meaningless inside an embedded subschema) and the canonical
/// `GtsInstanceId` / `GtsTypeId` `$defs` references inlined, so the fragment is
/// self-contained when embedded into a host document.
#[must_use]
pub fn inline_traits_schema_of<T: schemars::JsonSchema>() -> Value {
    let mut generator = schemars::SchemaGenerator::default();
    let schema = <T as schemars::JsonSchema>::json_schema(&mut generator);
    let mut value =
        serde_json::to_value(&schema).unwrap_or_else(|_| serde_json::json!({ "type": "object" }));

    if let Some(obj) = value.as_object_mut() {
        obj.remove("$schema");
        if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
            for prop_value in props.values_mut() {
                let Some(prop) = prop_value.as_object_mut() else {
                    continue;
                };
                let Some(ref_str) = prop.get("$ref").and_then(Value::as_str) else {
                    continue;
                };
                let resolved = match ref_str {
                    "#/$defs/GtsInstanceId" => Some(GtsInstanceId::json_schema_value()),
                    "#/$defs/GtsTypeId" | "#/$defs/GtsSchemaId" => {
                        Some(GtsTypeId::json_schema_value())
                    }
                    _ => None,
                };
                if let Some(inline) = resolved {
                    *prop_value = inline;
                }
            }
        }
    }
    value
}
