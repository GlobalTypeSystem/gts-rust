//! OP#13 – Schema Traits Validation (`x-gts-traits-schema` / `x-gts-traits`)
//!
//! Validates that trait values provided in derived schemas conform to the
//! effective trait schema built from the entire inheritance chain.
//!
//! **Algorithm:**
//! 1. Walk the chain from leftmost (base) to rightmost (leaf) segment.
//! 2. For each schema in the chain, collect:
//!    - `x-gts-traits-schema` subschemas (object | `true` | `false`) → compose
//!      via `allOf` into the *effective trait schema*.
//!    - `x-gts-traits` objects → merge per RFC 7396 JSON Merge Patch into the
//!      *effective traits object*.
//! 3. Materialize unresolved trait properties from the effective trait schema,
//!    filling each absent property with its `const` (a locked value) or, failing
//!    that, its `default`.
//! 4. Validate the effective traits object against the effective trait schema
//!    (completeness check runs only when the type is non-abstract — see
//!    `store.rs::validate_schema`).
//!
//! **Override semantics (RFC 7396 JSON Merge Patch):**
//! - Scalars: descendant value wins (last-wins).
//! - Objects: deep-merge recursively (keys not restated by the descendant are
//!   preserved from the ancestor).
//! - Arrays: replace wholesale (no element-wise merge).
//! - `null` at any depth deletes the key, after which `materialize_traits` may
//!   re-substitute a `const` or `default`.
//! - Locking publisher-controlled values is done via JSON Schema `const` in
//!   `x-gts-traits-schema`; the registry carries no GTS-specific immutability
//!   rule.
//!
//! **Empty trait schemas:** If a schema in the chain declares
//! `x-gts-traits-schema: {}` or `true`, it contributes an unconstrained
//! sub-schema. `false` contributes a sub-schema that rejects all values; a
//! type whose effective schema is `false` and which carries no traits passes
//! (nothing is validated), but any non-empty trait value fails.
//!
//! **Construction side.** This module also owns the Rust-side helpers that
//! *build* the `x-gts-traits-schema` value the validation logic above consumes:
//! the [`GtsTraitsSchema`] opt-in marker and [`inline_traits_schema_of`] (see
//! the "Inline trait-schema construction" section). Keeping construction and
//! validation together — alongside the [`X_GTS_TRAITS_SCHEMA`] / [`X_GTS_TRAITS`]
//! keyword constants — means there is one home for everything `x-gts-traits-*`.

use serde_json::Value;

/// JSON Schema annotation keyword that defines the *shape* of trait properties
/// available to a GTS type and its descendants. Schema-only — MUST NOT appear
/// in instances (see GTS spec § 9.7.1).
pub const X_GTS_TRAITS_SCHEMA: &str = "x-gts-traits-schema";

/// JSON Schema annotation keyword that supplies concrete *values* for trait
/// properties declared via [`X_GTS_TRAITS_SCHEMA`]. Schema-only — MUST NOT
/// appear in instances (see GTS spec § 9.7.1).
pub const X_GTS_TRAITS: &str = "x-gts-traits";

/// Maximum recursion depth for traversing `allOf` nesting.
/// Prevents stack overflow on deeply nested or maliciously crafted schemas.
const MAX_RECURSION_DEPTH: usize = 64;

/// Built trait validation artifacts plus the raw chain inputs they were built
/// from. A single self-contained value: callers build it once (via
/// [`build_effective_traits`] or `GtsStore::effective_traits`) and then both
/// read the composed `schema`/`values` and run [`EffectiveTraits::validate`]
/// off the same instance — no rebuild, no separate bookkeeping flags.
pub(crate) struct EffectiveTraits {
    /// Dialect-pinned, `allOf`-composed effective trait schema.
    pub schema: Value,
    /// Chain-merged (RFC 7396) and const/default-materialized trait values.
    pub values: Value,
    /// `$ref`-resolved `x-gts-traits-schema` subschemas, root → leaf — retained
    /// for per-index integrity checks and the closed-entity check.
    pub(crate) resolved_trait_schemas: Vec<Value>,
    /// RFC 7396-merged `x-gts-traits` values across the chain (pre-defaults).
    pub(crate) merged_traits: Value,
}

impl EffectiveTraits {
    /// `true` when the chain contributed at least one `x-gts-traits-schema`.
    fn has_schema(&self) -> bool {
        !self.resolved_trait_schemas.is_empty()
    }

    /// `true` when the chain supplied at least one explicit `x-gts-traits` value.
    fn has_explicit_values(&self) -> bool {
        self.merged_traits
            .as_object()
            .is_some_and(|m| !m.is_empty())
    }

    /// Validate these built artifacts. Runs, in order: per-index integrity of
    /// the collected subschemas (preserving indexed error messages), the
    /// empty-chain and `false`-schema guards, then JSON Schema + `x-gts-ref`
    /// value validation (with the required-trait completeness check when
    /// `check_unresolved`).
    ///
    /// # Errors
    /// Returns `Vec<String>` of error messages if any trait schema is malformed,
    /// if traits are provided without a schema, if the schema resolves to
    /// `false` with values present, or if values don't conform.
    pub(crate) fn validate(&self, check_unresolved: bool) -> Result<(), Vec<String>> {
        validate_trait_schema_integrity(&self.resolved_trait_schemas)?;

        if !self.has_schema() {
            if self.has_explicit_values() {
                return Err(vec![format!(
                    "{X_GTS_TRAITS} values provided but no {X_GTS_TRAITS_SCHEMA} is defined in the \
                     inheritance chain"
                )]);
            }
            return Ok(());
        }

        if effective_schema_is_false(&self.schema) {
            if self.has_explicit_values() {
                return Err(vec![format!(
                    "{X_GTS_TRAITS_SCHEMA} resolves to `false` in the chain — \
                     {X_GTS_TRAITS} values are prohibited"
                )]);
            }
            return Ok(());
        }

        validate_trait_values(&self.schema, &self.values, check_unresolved)
    }
}

// ---------------------------------------------------------------------------
// Inline trait-schema construction
// ---------------------------------------------------------------------------
//
// A trait shape can be supplied two ways:
//
// - **inline** — a private object subschema embedded directly under
//   `x-gts-traits-schema`. Produced from any `#[derive(schemars::JsonSchema)]`
//   struct via [`inline_traits_schema_of`] (the macro emits
//   `traits_schema = inline(MyStruct)`).
// - **referenced** — a reusable trait-schema registered as an ordinary GTS
//   type, pulled in via `$ref`. The macro emits this for `traits_schema = T`
//   where `T` is a `#[struct_to_gts_schema]` type, as
//   `{ "type": "object", "allOf": [{ "$ref": "gts://<TYPE_ID>" }] }`.
//
// `const`, `default` and `x-gts-ref` on trait properties are expressed with
// standard schemars/serde attributes (`#[schemars(extend("const" = ...))]`,
// `#[serde(default = "...")]`, `#[schemars(extend("x-gts-ref" = "..."))]`), so
// no GTS-specific field attributes are needed.

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
/// stripped (meaningless inside an embedded subschema), so the fragment is
/// self-contained when embedded into a host document.
///
/// All subschemas are inlined via `inline_subschemas`: any `$ref` schemars
/// would otherwise emit points at `#/$defs/<Name>`, a JSON pointer resolved
/// against the *host document* root rather than this fragment — and the
/// fragment carries no `$defs` of its own, so such a ref would be structurally
/// broken. Inlining expands every named subschema in place, including
/// `GtsInstanceId` / `GtsTypeId` (whose `JsonSchema` impls already emit the
/// canonical inline body) and arbitrary user enums / nested structs used as
/// trait-schema fields.
///
/// The one shape that cannot be inlined is a genuinely *recursive* type, for
/// which schemars must keep a `$ref` to break the cycle; such a type is not a
/// valid inline trait-schema field.
///
/// # Panics
/// Panics only if serializing `T`'s generated `schemars::Schema` to a
/// `serde_json::Value` fails, which is infallible for a valid `JsonSchema`
/// impl (a schema carries no non-string map keys or non-finite floats). The
/// panic is preferred over silently degrading to an accept-anything `{}`.
#[must_use]
// `serde_json::to_value` on a `schemars::Schema` is infallible (no non-string
// map keys, no NaN/Inf floats in a generated schema), so the only way to reach
// the panic is a schemars bug. For a spec reference implementation, failing
// loudly is correct: silently degrading to `{ "type": "object" }` would yield
// an accept-anything trait schema that validates nothing.
#[allow(clippy::expect_used)]
pub fn inline_traits_schema_of<T: schemars::JsonSchema>() -> Value {
    let mut generator = schemars::generate::SchemaSettings::default()
        .with(|s| s.inline_subschemas = true)
        .into_generator();
    let schema = <T as schemars::JsonSchema>::json_schema(&mut generator);
    let mut value = serde_json::to_value(&schema)
        .expect("schemars JsonSchema serialization to a JSON value is infallible for valid types");

    if let Some(obj) = value.as_object_mut() {
        obj.remove("$schema");
    }
    value
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validates schema traits for a full inheritance chain.
///
/// `chain_schemas` is an ordered list of `(schema_id, raw_schema_content)` pairs
/// from base (index 0) to leaf (last index).  The content should be **raw**
/// (not allOf-flattened) so that `x-gts-*` extension keys are preserved.
///
/// This is the self-contained entry point used by unit tests.  The store
/// integration builds an [`EffectiveTraits`] from its own chain walk and calls
/// [`EffectiveTraits::validate`] on it.
///
/// # Errors
/// Returns `Vec<String>` of error messages if trait values don't conform to the
/// effective trait schema or if traits are provided without trait schema.
#[cfg(test)]
pub fn validate_traits_chain(chain_schemas: &[(String, Value)]) -> Result<(), Vec<String>> {
    let mut trait_schemas = Vec::new();
    let mut merged = serde_json::Map::new();
    for (_id, content) in chain_schemas {
        collect_trait_schema_from_value(content, &mut trait_schemas);
        collect_traits_from_value(content, &mut merged);
    }
    // Dialect comes from the leaf document's `$schema`, mirroring the store path.
    // Absent (synthetic fixtures without `$schema`) falls back to the validator's
    // default draft, exactly like the rest of the crate.
    let dialect = chain_schemas
        .last()
        .and_then(|(_, content)| content.get("$schema").and_then(Value::as_str));
    build_effective_traits(&trait_schemas, &Value::Object(merged), dialect).validate(true)
}

/// Validate each collected `x-gts-traits-schema` subschema before composition,
/// preserving indexed error messages for malformed inputs. Run first by
/// [`EffectiveTraits::validate`].
///
/// # Errors
/// Returns `Vec<String>` of error messages if any collected trait schema is not
/// a JSON Schema object or boolean subschema.
fn validate_trait_schema_integrity(resolved_trait_schemas: &[Value]) -> Result<(), Vec<String>> {
    // Each x-gts-traits-schema is a JSON Schema subschema. Accepted forms are
    // an object subschema, `true`, or `false`. Validate JSON Schema integrity
    // only for object-form subschemas; the boolean forms have well-defined
    // JSON Schema semantics (true = accept anything, false = reject anything)
    // and need no further checks here.
    //
    // Note on x-gts-* keys inside an object subschema: any GTS type may be
    // referenced from another host's `x-gts-traits-schema` via `$ref`, in
    // which case the inlined body of the referenced type will contain its own
    // `x-gts-traits-schema` / `x-gts-traits` keys as ordinary JSON members.
    // To a standard JSON Schema validator these are unknown keywords (JSON
    // Schema treats unknown keys as annotations and ignores them for
    // validation), so they are inert here. This module deliberately does not
    // reject their presence — doing so would prevent the legitimate authoring
    // pattern where an existing GTS type is reused as a trait-schema source.
    for (i, ts) in resolved_trait_schemas.iter().enumerate() {
        match ts {
            Value::Bool(_) => {}
            Value::Object(_) => {
                if let Err(e) = jsonschema::validator_for(ts) {
                    return Err(vec![format!(
                        "x-gts-traits-schema[{i}] is not a valid JSON Schema: {e}"
                    )]);
                }
            }
            _ => {
                return Err(vec![format!(
                    "x-gts-traits-schema[{i}] must be an object subschema or a boolean; got {ts}"
                )]);
            }
        }
    }
    Ok(())
}

/// Build the dialect-pinned effective traits schema and the materialized
/// effective-traits object from chain-collected inputs, returning a
/// self-contained [`EffectiveTraits`] (the inputs are retained so the result
/// can validate itself via [`EffectiveTraits::validate`] without a rebuild).
///
/// `resolved_trait_schemas` must already have any `$ref`s resolved. `dialect`
/// is the host document's `$schema`, re-injected here because the inline trait
/// fragment had its root-only `$schema` stripped when embedded; when `None`,
/// the schema is left as-is so the validator detects/defaults the draft (Draft
/// 2020-12), matching instance/schema validation elsewhere in this crate.
pub(crate) fn build_effective_traits(
    resolved_trait_schemas: &[Value],
    merged_traits: &Value,
    dialect: Option<&str>,
) -> EffectiveTraits {
    let mut effective_traits_schema = build_effective_traits_schema(resolved_trait_schemas);

    if let Some(dialect) = dialect
        && let Some(obj) = effective_traits_schema.as_object_mut()
    {
        obj.insert("$schema".to_owned(), Value::String(dialect.to_owned()));
    }

    let values = materialize_traits(&effective_traits_schema, merged_traits);
    EffectiveTraits {
        schema: effective_traits_schema,
        values,
        resolved_trait_schemas: resolved_trait_schemas.to_vec(),
        merged_traits: merged_traits.clone(),
    }
}

/// Validate a default-filled trait values object against its composed schema:
/// standard JSON Schema validation (plus the required-trait completeness check
/// when `check_unresolved`) followed by GTS `x-gts-ref` enforcement, which the
/// standard validator ignores as an unknown keyword.
fn validate_trait_values(
    effective_traits_schema: &Value,
    effective_traits: &Value,
    check_unresolved: bool,
) -> Result<(), Vec<String>> {
    let mut errors = match validate_traits_against_schema(
        effective_traits_schema,
        effective_traits,
        check_unresolved,
    ) {
        Ok(()) => Vec::new(),
        Err(e) => e,
    };

    // Enforce `x-gts-ref` on trait values. The standard `jsonschema` validator
    // ignores `x-gts-ref` as an unknown keyword, so a trait value that violates
    // the declared GTS-prefix would otherwise slip through. Treat the effective
    // trait-schema as the schema and the materialized effective traits as the
    // instance.
    let xref = crate::x_gts_ref::XGtsRefValidator::new();
    for err in xref.validate_instance(effective_traits, effective_traits_schema, "") {
        errors.push(format!("trait x-gts-ref: {err}"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Returns true when at least one subschema along the chain is the JSON
/// boolean `false`. Under JSON Schema `allOf` semantics, `false` makes the
/// composed schema unsatisfiable; treat it as the "traits prohibited" signal.
///
/// Recursion is bounded by [`MAX_RECURSION_DEPTH`] to prevent stack overflow.
fn effective_schema_is_false(schema: &Value) -> bool {
    effective_schema_is_false_recursive(schema, 0)
}

fn effective_schema_is_false_recursive(schema: &Value, depth: usize) -> bool {
    if depth >= MAX_RECURSION_DEPTH {
        return false;
    }
    match schema {
        Value::Bool(false) => true,
        Value::Object(obj) => {
            if let Some(Value::Array(items)) = obj.get("allOf") {
                items
                    .iter()
                    .any(|item| effective_schema_is_false_recursive(item, depth + 1))
            } else {
                false
            }
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Collection helpers (pub(crate) so the store can call them)
// ---------------------------------------------------------------------------

/// Inline JSON Pointer `$ref`s (`#/...`) inside a trait-schema fragment by
/// resolving them against `root` — the host document the fragment was lifted
/// from.
///
/// The extracted `x-gts-traits-schema` fragment carries no `$defs` of its own,
/// so a root-relative pointer like `#/$defs/Retention` would dangle once the
/// fragment is composed into the effective trait-schema's `allOf` (the document
/// root there no longer holds those `$defs`). Per gts-spec §9.7.5, a `$ref`
/// inside `x-gts-traits-schema` MUST resolve under standard JSON Schema rules,
/// and a JSON Pointer fragment resolves against the host document — which is
/// `root` here. Inlining at collection time, while `root` is still the document
/// root, keeps the composed fragment self-contained.
///
/// Only pointers that actually resolve against `root` are inlined; anything
/// else (notably `gts://` refs and the synthetic `#/$defs/GtsInstanceId` family
/// that the macro emits and [`crate::store::GtsStore::resolve_schema_refs`]
/// special-cases) is left untouched. Recursion is bounded by
/// [`MAX_RECURSION_DEPTH`].
pub(crate) fn inline_local_pointers(fragment: &Value, root: &Value) -> Value {
    inline_local_pointers_recursive(fragment, root, 0)
}

fn inline_local_pointers_recursive(value: &Value, root: &Value, depth: usize) -> Value {
    if depth >= MAX_RECURSION_DEPTH {
        return value.clone();
    }
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("$ref")
                && let Some(ptr) = r.strip_prefix("#/")
                && let Some(target) = root.pointer(&format!("/{ptr}"))
            {
                // Resolve the target against the same root, then overlay any
                // sibling keywords (JSON Schema `$ref`-with-siblings).
                let mut resolved = inline_local_pointers_recursive(target, root, depth + 1);
                if map.len() > 1
                    && let Value::Object(resolved_map) = &mut resolved
                {
                    for (k, v) in map {
                        if k != "$ref" {
                            resolved_map.insert(
                                k.clone(),
                                inline_local_pointers_recursive(v, root, depth + 1),
                            );
                        }
                    }
                }
                return resolved;
            }
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(
                    k.clone(),
                    inline_local_pointers_recursive(v, root, depth + 1),
                );
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| inline_local_pointers_recursive(v, root, depth + 1))
                .collect(),
        ),
        _ => value.clone(),
    }
}

/// Recursively search a schema value for `x-gts-traits-schema` entries.
///
/// Handles both top-level and `allOf`-nested occurrences.
/// Recursion is bounded by [`MAX_RECURSION_DEPTH`] to prevent stack overflow.
pub(crate) fn collect_trait_schema_from_value(value: &Value, out: &mut Vec<Value>) {
    collect_trait_schema_recursive(value, out, 0);
}

fn collect_trait_schema_recursive(value: &Value, out: &mut Vec<Value>, depth: usize) {
    if depth >= MAX_RECURSION_DEPTH {
        return;
    }

    let Some(obj) = value.as_object() else {
        return;
    };

    if let Some(ts) = obj.get(X_GTS_TRAITS_SCHEMA) {
        out.push(ts.clone());
    }

    // Also check inside allOf items (e.g. a derived schema that is an allOf overlay)
    if let Some(Value::Array(all_of)) = obj.get("allOf") {
        for item in all_of {
            collect_trait_schema_recursive(item, out, depth + 1);
        }
    }
}

/// Recursively search a schema value for `x-gts-traits` entries and union
/// them into a single per-level trait patch.
///
/// `null` values are preserved verbatim — they carry RFC 7396 "delete this
/// key" semantics and must reach the cross-level merge step (in
/// `store::effective_traits`) intact. Within a single level, multiple
/// `x-gts-traits` blocks (e.g. one inline + ones nested in `allOf` overlays)
/// are unioned with later-occurring entries winning per key. The cross-level
/// step then applies these per-level patches in chain order via RFC 7396.
///
/// Recursion is bounded by [`MAX_RECURSION_DEPTH`] to prevent stack overflow.
pub(crate) fn collect_traits_from_value(
    value: &Value,
    merged: &mut serde_json::Map<String, Value>,
) {
    collect_traits_recursive(value, merged, 0);
}

fn collect_traits_recursive(
    value: &Value,
    merged: &mut serde_json::Map<String, Value>,
    depth: usize,
) {
    if depth >= MAX_RECURSION_DEPTH {
        return;
    }

    let Some(obj) = value.as_object() else {
        return;
    };

    if let Some(Value::Object(traits)) = obj.get(X_GTS_TRAITS) {
        for (k, v) in traits {
            merged.insert(k.clone(), v.clone());
        }
    }

    if let Some(Value::Array(all_of)) = obj.get("allOf") {
        for item in all_of {
            collect_traits_recursive(item, merged, depth + 1);
        }
    }
}

/// Merge `patch` into `target` per RFC 7396 JSON Merge Patch.
///
/// Semantics:
/// - Scalar / array values replace the existing value wholesale.
/// - Objects merge recursively (keys not restated by `patch` are preserved).
/// - `null` values **delete** the corresponding key from `target`; if the
///   target had no such key the null is a no-op (the key remains absent so
///   `materialize_traits` can later substitute a `const`/`default` from the
///   trait schema).
///
/// This is the trait-merge primitive used to compose `x-gts-traits` along the
/// `$id` chain (root → leaf).
///
/// Recursion over nested objects is bounded by [`MAX_RECURSION_DEPTH`] to
/// prevent stack overflow on deeply-nested (or maliciously crafted) trait
/// values.
pub(crate) fn merge_rfc7396_into(
    target: &mut serde_json::Map<String, Value>,
    patch: &serde_json::Map<String, Value>,
) {
    merge_rfc7396_recursive(target, patch, 0);
}

fn merge_rfc7396_recursive(
    target: &mut serde_json::Map<String, Value>,
    patch: &serde_json::Map<String, Value>,
    depth: usize,
) {
    if depth >= MAX_RECURSION_DEPTH {
        return;
    }
    for (k, v) in patch {
        match v {
            Value::Null => {
                target.remove(k);
            }
            Value::Object(patch_obj) => {
                if let Some(Value::Object(existing)) = target.get_mut(k) {
                    merge_rfc7396_recursive(existing, patch_obj, depth + 1);
                } else {
                    // Either target lacks the key or holds a non-object —
                    // RFC 7396: a new object value replaces wholesale, but
                    // inner `null`s in the patch still mean "no such key".
                    let mut fresh = serde_json::Map::new();
                    merge_rfc7396_recursive(&mut fresh, patch_obj, depth + 1);
                    target.insert(k.clone(), Value::Object(fresh));
                }
            }
            other => {
                target.insert(k.clone(), other.clone());
            }
        }
    }
}

/// Build a single effective trait schema by composing all collected trait schemas
/// using `allOf`.  When there is only one schema, return it directly.
///
/// **Note on `additionalProperties`:** When multiple trait schemas are composed
/// via `allOf`, standard JSON Schema semantics apply.  If one sub-schema sets
/// `additionalProperties: false`, properties introduced by *other* sub-schemas
/// in the same `allOf` may fail validation.  This is correct per the JSON Schema
/// specification — authors should use `additionalProperties: false` only in the
/// outermost (single) trait schema, or omit it in favour of explicit property
/// lists.
pub(crate) fn build_effective_traits_schema(schemas: &[Value]) -> Value {
    match schemas.len() {
        0 => Value::Object(serde_json::Map::new()),
        1 => schemas[0].clone(),
        _ => {
            let mut wrapper = serde_json::Map::new();
            wrapper.insert("type".to_owned(), Value::String("object".to_owned()));
            wrapper.insert("allOf".to_owned(), Value::Array(schemas.to_vec()));
            Value::Object(wrapper)
        }
    }
}

/// Materialize trait values from the effective trait schema onto the merged
/// traits object, filling any property that is not yet present.
///
/// Resolution precedence for an absent property is **`const` → `default`**: a
/// `const` locks the value (it is the only value the schema accepts, so the
/// effective value is fully determined even when the chain never restates it),
/// and `default` fills the rest. A property already supplied by the chain is
/// left as-is — a value that conflicts with a `const`/enum is caught by the
/// later JSON Schema validation, which gives a clearer error than silently
/// overwriting it here.
///
/// Handles nested object properties recursively: if a present trait property is
/// an object type with its own `properties`, nested `const`/`default` values
/// are materialized into the corresponding nested object.
fn materialize_traits(trait_schema: &Value, traits: &Value) -> Value {
    materialize_traits_recursive(trait_schema, traits, 0)
}

/// Per-property materialization view: (most-derived declaration, nearest
/// `const`, nearest `default`).
type PropResolution = (Value, Option<Value>, Option<Value>);

fn materialize_traits_recursive(trait_schema: &Value, traits: &Value, depth: usize) -> Value {
    if depth >= MAX_RECURSION_DEPTH {
        return traits.clone();
    }

    let mut result = match traits {
        Value::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };

    // All property declarations along the chain, in root→leaf order. The same
    // property may appear in several `allOf` branches — an ancestor declaring it
    // plus a descendant narrowing it.
    let mut all_props: Vec<(String, Value)> = Vec::new();
    collect_props_recursive(trait_schema, &mut all_props, 0);

    // Resolve each property once. `const`/`default` are taken from the *nearest*
    // (most-derived) declaration that carries them — scanning leaf→root — because
    // `default` does not participate in narrowing, so an ancestor default ripples
    // to descendants even when a descendant redeclares the property without one
    // (gts-spec §9.7.2, ADR-0003). The most-derived declaration also drives the
    // "recurse into nested object" decision.
    let mut order: Vec<String> = Vec::new();
    let mut resolved: std::collections::HashMap<String, PropResolution> =
        std::collections::HashMap::new();
    for (name, sch) in all_props.iter().rev() {
        let obj = sch.as_object();
        let entry = resolved.entry(name.clone()).or_insert_with(|| {
            order.push(name.clone());
            (sch.clone(), None, None)
        });
        if entry.1.is_none()
            && let Some(const_val) = obj.and_then(|o| o.get("const"))
        {
            entry.1 = Some(const_val.clone());
        }
        if entry.2.is_none()
            && let Some(default_val) = obj.and_then(|o| o.get("default"))
        {
            entry.2 = Some(default_val.clone());
        }
    }

    for name in &order {
        let (prop_schema, nearest_const, nearest_default) = &resolved[name];
        if !result.contains_key(name.as_str()) {
            // Property is absent — a `const` locks the value (highest priority),
            // otherwise fall back to the nearest `default` up the chain.
            if let Some(const_val) = nearest_const {
                result.insert(name.clone(), const_val.clone());
            } else if let Some(default_val) = nearest_default {
                result.insert(name.clone(), default_val.clone());
            }
        } else if result.get(name.as_str()).is_some_and(Value::is_object)
            && prop_schema.as_object().is_some_and(|o| {
                o.get("type") == Some(&Value::String("object".to_owned()))
                    && o.contains_key("properties")
            })
        {
            // Present value is an object and the schema declares an object with
            // sub-properties — recurse to materialize nested const/default. We
            // skip recursion for a non-object value so a freshly materialized
            // object doesn't mask the type error from later validation.
            let nested = materialize_traits_recursive(
                prop_schema,
                result.get(name.as_str()).unwrap_or(&Value::Null),
                depth + 1,
            );
            result.insert(name.clone(), nested);
        }
    }

    Value::Object(result)
}

/// Collect all property definitions from a schema, handling `allOf` composition.
///
/// When the same property name appears in multiple `allOf` sub-schemas (e.g.
/// base defines `priority: {type: string}` and mid narrows to an enum), the
/// *last-seen* definition wins.  This matches the rightmost-wins semantics of
/// JSON Schema `allOf` merge and avoids duplicate "unresolved" errors.
fn collect_all_properties(schema: &Value) -> Vec<(String, Value)> {
    let mut props = Vec::new();
    collect_props_recursive(schema, &mut props, 0);
    // Deduplicate: keep last occurrence of each property name (rightmost wins)
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::with_capacity(props.len());
    for (name, schema) in props.into_iter().rev() {
        if seen.insert(name.clone()) {
            deduped.push((name, schema));
        }
    }
    deduped.reverse();
    deduped
}

fn collect_props_recursive(schema: &Value, props: &mut Vec<(String, Value)>, depth: usize) {
    if depth >= MAX_RECURSION_DEPTH {
        return;
    }

    let Some(obj) = schema.as_object() else {
        return;
    };

    if let Some(Value::Object(p)) = obj.get("properties") {
        for (k, v) in p {
            props.push((k.clone(), v.clone()));
        }
    }

    if let Some(Value::Array(all_of)) = obj.get("allOf") {
        for item in all_of {
            collect_props_recursive(item, props, depth + 1);
        }
    }
}

/// Collect the union of `required` property names declared at the top level or
/// within any `allOf` branch of the (effective) trait schema. Mirrors
/// [`collect_all_properties`] so completeness enforcement matches JSON Schema's
/// own `required` aggregation across the composed chain.
fn collect_all_required(schema: &Value) -> std::collections::HashSet<String> {
    let mut req = std::collections::HashSet::new();
    collect_required_recursive(schema, &mut req, 0);
    req
}

fn collect_required_recursive(
    schema: &Value,
    req: &mut std::collections::HashSet<String>,
    depth: usize,
) {
    if depth >= MAX_RECURSION_DEPTH {
        return;
    }

    let Some(obj) = schema.as_object() else {
        return;
    };

    if let Some(Value::Array(required)) = obj.get("required") {
        for item in required {
            if let Some(name) = item.as_str() {
                req.insert(name.to_owned());
            }
        }
    }

    if let Some(Value::Array(all_of)) = obj.get("allOf") {
        for item in all_of {
            collect_required_recursive(item, req, depth + 1);
        }
    }
}

/// Validate the effective traits object against the effective trait schema.
///
/// Uses the `jsonschema` crate for standard JSON Schema validation.  This
/// catches type mismatches, enum violations, `additionalProperties` errors,
/// and any other constraint issues.
///
/// Additionally checks that every *required* property defined in the trait
/// schema is resolved (has a value or default) — i.e. there are no required
/// "holes" left after applying defaults. Optional properties may be unresolved.
fn validate_traits_against_schema(
    trait_schema: &Value,
    effective_traits: &Value,
    check_unresolved: bool,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // Standard JSON Schema validation of the traits object
    match jsonschema::validator_for(trait_schema) {
        Ok(validator) => {
            for error in validator.iter_errors(effective_traits) {
                errors.push(format!("trait validation: {error}"));
            }
        }
        Err(e) => {
            errors.push(format!("failed to compile trait schema: {e}"));
        }
    }

    // Check for unresolved (missing) trait properties that have no default.
    // A property is "unresolved" if:
    // - It exists in the trait schema `properties`
    // - It has no `default`
    // - It is absent from the effective traits object
    // Skipped when check_unresolved is false (intermediate schema validation).
    if !check_unresolved {
        return if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        };
    }

    let all_props = collect_all_properties(trait_schema);
    let required = collect_all_required(trait_schema);
    let traits_obj = effective_traits.as_object();

    for (prop_name, prop_schema) in &all_props {
        // Only *required* trait properties must be resolved. An optional
        // property left unresolved is spec-valid: the GTS spec keys completeness
        // on standard JSON Schema validation (README §9.7.5) and OP#13 requires
        // resolution of "all required trait properties" — not every declared one.
        // (Standard JSON Schema validation above already reports missing required
        // members; this loop adds a type-annotated, trait-specific message.)
        if !required.contains(prop_name.as_str()) {
            continue;
        }

        let has_value = traits_obj.is_some_and(|m| m.contains_key(prop_name.as_str()));

        // A `const` fully determines the value (materialized by
        // `materialize_traits`), so it resolves the property just like a
        // `default` does.
        let has_default_or_const = prop_schema
            .as_object()
            .is_some_and(|m| m.contains_key("default") || m.contains_key("const"));

        if !has_value && !has_default_or_const {
            let expected_type = prop_schema
                .as_object()
                .and_then(|m| m.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("any");
            errors.push(format!(
                "trait property '{prop_name}' (type: {expected_type}) is not resolved: \
                 no value provided and no default or const defined in the trait schema. \
                 All traits must be resolved (via a {X_GTS_TRAITS} value in the chain \
                 or a `default`/`const` in the trait schema) on non-abstract types; \
                 otherwise mark the type abstract (x-gts-abstract: true)"
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_trait_schema_integrity_rejects_non_object_non_boolean() {
        // A resolved trait schema that is neither an object subschema nor a
        // boolean (here, an array) must hit the dedicated error arm.
        let schemas = vec![json!([1, 2])];
        let err = validate_trait_schema_integrity(&schemas).unwrap_err();
        assert!(
            err.iter()
                .any(|m| m.contains("must be an object subschema or a boolean")),
            "expected the non-object/non-boolean arm, got: {err:?}"
        );
    }

    #[test]
    fn test_inline_local_pointers_ref_with_siblings_overlay() {
        // `$ref` with sibling keywords: the pointer target is inlined and the
        // sibling (`description`) is overlaid onto the result.
        let root = json!({
            "$defs": {"X": {"type": "string", "minLength": 1}}
        });
        let fragment = json!({"$ref": "#/$defs/X", "description": "extra"});
        let inlined = inline_local_pointers(&fragment, &root);
        assert_eq!(inlined["type"], json!("string"));
        assert_eq!(inlined["minLength"], json!(1));
        assert_eq!(inlined["description"], json!("extra"));
        assert!(inlined.get("$ref").is_none());
    }

    #[test]
    fn test_no_traits_schema_passes() {
        let chain = vec![(
            "gts.x.test.base.v1~".to_owned(),
            json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object", "properties": {"id": {"type": "string"}}}),
        )];
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_traits_without_schema_in_derived_fails() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object", "properties": {"id": {"type": "string"}}}),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"retention": "P30D"}
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            err.iter().any(|e| e.contains("no x-gts-traits-schema")),
            "should fail when traits provided without schema: {err:?}"
        );
    }

    #[test]
    fn test_traits_without_schema_in_base_fails() {
        let chain = vec![(
            "base~".to_owned(),
            json!({"$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-traits": {"retention": "P30D"},
                "properties": {"id": {"type": "string"}}
            }),
        )];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            err.iter().any(|e| e.contains("no x-gts-traits-schema")),
            "should fail when base has traits but no schema: {err:?}"
        );
    }

    #[test]
    fn test_all_traits_resolved() {
        let chain = vec![
            (
                "gts.x.test.base.v1~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "retention": {"type": "string"},
                            "topicRef": {"type": "string"}
                        }
                    }
                }),
            ),
            (
                "gts.x.test.base.v1~x.test._.derived.v1~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {
                        "retention": "P90D",
                        "topicRef": "gts.x.core.events.topic.v1~x.test._.orders.v1"
                    }
                }),
            ),
        ];
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_defaults_fill_traits() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "retention": {"type": "string", "default": "P30D"},
                            "topicRef": {"type": "string", "default": "default_topic"}
                        }
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object"}),
            ),
        ];
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_const_only_required_trait_resolves_and_materializes() {
        // A required trait whose schema pins a `const` (no default, no explicit
        // x-gts-traits value) must (a) be materialized into the effective traits
        // and (b) pass completeness — its value is fully determined by the lock.
        let schemas = vec![json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "channel": {"type": "string", "const": "audit"}
            },
            "required": ["channel"]
        })];
        let traits = build_effective_traits(
            &schemas,
            &json!({}),
            Some("http://json-schema.org/draft-07/schema#"),
        );
        assert_eq!(
            traits.values["channel"], "audit",
            "const must be materialized into effective traits values"
        );
        assert!(
            traits.validate(true).is_ok(),
            "a const-locked required trait is fully resolved: {:?}",
            traits.validate(true)
        );
    }

    #[test]
    fn test_const_takes_priority_over_default_in_materialization() {
        let schemas = vec![json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string", "const": "locked", "default": "open"}
            }
        })];
        let traits = build_effective_traits(&schemas, &json!({}), None);
        assert_eq!(
            traits.values["mode"], "locked",
            "const wins over default when the value is absent"
        );
    }

    #[test]
    fn test_explicit_value_conflicting_with_const_fails() {
        // An explicit x-gts-traits value is kept as-is (not silently overwritten
        // by const); JSON Schema validation then reports the conflict.
        let chain = vec![(
            "base~".to_owned(),
            json!({"$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "object",
                    "properties": {"channel": {"type": "string", "const": "audit"}}
                },
                "x-gts-traits": {"channel": "events"}
            }),
        )];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            !err.is_empty(),
            "explicit value conflicting with const must fail validation"
        );
    }

    #[test]
    fn test_ancestor_default_ripples_when_descendant_redeclares_without_default() {
        // base declares `retention` with a default; a descendant narrows the same
        // property in its own trait-schema but omits the default. Per gts-spec
        // §9.7.2 / ADR-0003 the ancestor default does not participate in narrowing
        // and still ripples down — it must be materialized (nearest existing
        // default wins), not shadowed by the bare redeclaration.
        let schemas = vec![
            json!({
                "type": "object",
                "properties": {"retention": {"type": "string", "default": "P30D"}},
                "required": ["retention"]
            }),
            json!({
                "type": "object",
                "properties": {"retention": {"type": "string"}}
            }),
        ];
        let traits = build_effective_traits(
            &schemas,
            &json!({}),
            Some("http://json-schema.org/draft-07/schema#"),
        );
        assert_eq!(
            traits.values["retention"], "P30D",
            "ancestor default must ripple when a descendant redeclares without a default"
        );
        assert!(
            traits.validate(true).is_ok(),
            "the rippled default resolves the required trait: {:?}",
            traits.validate(true)
        );
    }

    #[test]
    fn test_missing_required_trait_fails() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "topicRef": {"type": "string"},
                            "retention": {"type": "string", "default": "P30D"}
                        },
                        "required": ["topicRef"]
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {
                        "retention": "P90D"
                    }
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            err.iter().any(|e| e.contains("topicRef")),
            "should mention missing topicRef: {err:?}"
        );
    }

    #[test]
    fn test_optional_unresolved_trait_passes() {
        // Spec (README §9.7.5 / OP#13): only *required* trait properties must be
        // resolved. An optional declared property left unresolved is valid — it
        // simply stays absent and standard JSON Schema validation accepts it.
        let chain = vec![(
            "base~".to_owned(),
            json!({"$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "object",
                    "properties": {
                        "topicRef": {"type": "string"},
                        "note": {"type": "string"}
                    },
                    "required": ["topicRef"]
                },
                "x-gts-traits": {"topicRef": "events.orders"}
            }),
        )];
        assert!(
            validate_traits_chain(&chain).is_ok(),
            "optional unresolved trait property must not fail completeness"
        );
    }

    #[test]
    fn test_wrong_type_fails() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "maxRetries": {"type": "integer", "minimum": 0, "default": 3}
                        }
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {
                        "maxRetries": "not_a_number"
                    }
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(!err.is_empty(), "wrong type should fail");
    }

    #[test]
    fn test_unknown_property_fails() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "retention": {"type": "string", "default": "P30D"}
                        }
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {
                        "retention": "P90D",
                        "unknownTrait": "some_value"
                    }
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.contains("additional") || e.contains("unknownTrait")),
            "unknown property should fail: {err:?}"
        );
    }

    #[test]
    fn test_override_in_chain() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "retention": {"type": "string"}
                        }
                    }
                }),
            ),
            (
                "mid~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"retention": "P30D"}
                }),
            ),
            (
                "leaf~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"retention": "P365D"}
                }),
            ),
        ];
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_both_keywords_in_same_schema() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "topicRef": {"type": "string"},
                            "retention": {"type": "string", "default": "P30D"}
                        }
                    }
                }),
            ),
            (
                "mid~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "auditRetention": {"type": "string", "default": "P365D"}
                        }
                    },
                    "x-gts-traits": {
                        "topicRef": "gts.x.core.events.topic.v1~x.test._.audit.v1"
                    }
                }),
            ),
        ];
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_three_level_chain_missing_in_leaf() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "retention": {"type": "string", "default": "P30D"}
                        }
                    }
                }),
            ),
            (
                "mid~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {"type": "string"}
                        },
                        "required": ["priority"]
                    }
                }),
            ),
            (
                "leaf~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"retention": "P90D"}
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            err.iter().any(|e| e.contains("priority")),
            "should mention missing priority: {err:?}"
        );
    }

    #[test]
    fn test_enum_constraint_violation() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {
                                "type": "string",
                                "enum": ["low", "medium", "high", "critical"],
                                "default": "medium"
                            }
                        }
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"priority": "ultra_high"}
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(!err.is_empty(), "enum violation should fail");
    }

    #[test]
    fn test_minimum_violation() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "maxRetries": {
                                "type": "integer",
                                "minimum": 0,
                                "maximum": 10,
                                "default": 3
                            }
                        }
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"maxRetries": -1}
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(!err.is_empty(), "minimum violation should fail");
    }

    #[test]
    fn test_narrowing_valid() {
        // Base: priority is open string
        // Mid: narrows to enum, provides valid value
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {"type": "string"},
                            "retention": {"type": "string", "default": "P30D"}
                        }
                    }
                }),
            ),
            (
                "mid~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {
                                "type": "string",
                                "enum": ["low", "medium", "high", "critical"]
                            }
                        }
                    },
                    "x-gts-traits": {"priority": "high"}
                }),
            ),
        ];
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_narrowing_violation() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {"type": "string"},
                            "retention": {"type": "string", "default": "P30D"}
                        }
                    }
                }),
            ),
            (
                "mid~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {
                                "type": "string",
                                "enum": ["low", "medium", "high", "critical"]
                            }
                        }
                    },
                    "x-gts-traits": {"priority": "high"}
                }),
            ),
            (
                "leaf~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"priority": "ultra_high"}
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(!err.is_empty(), "narrowing violation should fail");
    }

    #[test]
    fn test_deep_inheritance_chain() {
        // Chain near MAX_RECURSION_DEPTH — exercises recursion guard boundary
        let mut chain = vec![(
            "base~".to_owned(),
            json!({"$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "object",
                    "properties": {
                        "retention": {"type": "string", "default": "P30D"}
                    }
                }
            }),
        )];
        for i in 1..super::MAX_RECURSION_DEPTH {
            chain.push((
                format!("level{i}~"),
                json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object"}),
            ));
        }
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_malformed_trait_schema_not_object() {
        // x-gts-traits-schema is a string, not an object
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": "not_an_object"
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"foo": "bar"}
                }),
            ),
        ];
        // The string value should be collected but fail gracefully at validation
        let result = validate_traits_chain(&chain);
        // The trait schema "not_an_object" has no properties, so "foo" is undeclared.
        // The chain should fail because traits are provided without a valid schema.
        assert!(
            result.is_err(),
            "malformed trait schema should fail: {result:?}"
        );
    }

    #[test]
    fn test_trait_values_as_object() {
        // Trait value is a nested object, not just a primitive
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "retry": {
                                "type": "object",
                                "properties": {
                                    "maxAttempts": {"type": "integer", "default": 3},
                                    "backoff": {"type": "string", "default": "exponential"}
                                }
                            }
                        }
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {
                        "retry": {"maxAttempts": 5}
                    }
                }),
            ),
        ];
        assert!(
            validate_traits_chain(&chain).is_ok(),
            "object trait values should be accepted"
        );
    }

    #[test]
    fn test_trait_values_as_array() {
        // Trait value is an array
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "tags": {
                                "type": "array",
                                "items": {"type": "string"},
                                "default": []
                            }
                        }
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {
                        "tags": ["audit", "compliance"]
                    }
                }),
            ),
        ];
        assert!(
            validate_traits_chain(&chain).is_ok(),
            "array trait values should be accepted"
        );
    }

    #[test]
    fn test_x_gts_keys_inside_trait_schema_are_tolerated() {
        // A trait-schema may contain GTS-extension keys as ordinary members —
        // this happens when an existing GTS type (which carries its own
        // `x-gts-traits-schema` / `x-gts-traits`) is reused as a trait-schema
        // source via $ref. Standard JSON Schema treats unknown keywords as
        // annotations, so these keys are inert at validation time and must
        // not cause registration to fail.
        //
        // To isolate this property from the unrelated completeness check, the
        // trait-schema below declares only optional properties and the chain
        // supplies a matching x-gts-traits value.
        let chain = vec![(
            "base~".to_owned(),
            json!({"$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "object",
                    // GTS-extension keys nested here mimic a $ref'd GTS type
                    // body — they are unknown keywords to JSON Schema and
                    // must be ignored.
                    "x-gts-traits-schema": {"type": "object"},
                    "x-gts-traits": {"foo": "bar"},
                    "properties": {
                        "retention": {"type": "string"}
                    }
                },
                "x-gts-traits": {"retention": "P30D"}
            }),
        )];
        assert!(
            validate_traits_chain(&chain).is_ok(),
            "x-gts-traits / x-gts-traits-schema nested inside a trait-schema body \
             should be tolerated as unknown JSON Schema keywords"
        );
    }

    #[test]
    fn test_nested_object_defaults_applied() {
        // Trait schema has nested object with defaults — verify they are applied
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "retry": {
                                "type": "object",
                                "properties": {
                                    "maxAttempts": {"type": "integer", "default": 3},
                                    "backoff": {"type": "string", "default": "exponential"}
                                }
                            }
                        }
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {
                        "retry": {"maxAttempts": 5}
                    }
                }),
            ),
        ];
        // Should pass because nested defaults fill in the missing "backoff"
        assert!(
            validate_traits_chain(&chain).is_ok(),
            "nested defaults should fill in missing sub-properties"
        );
    }

    #[test]
    fn test_improved_error_message_includes_type() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "topicRef": {"type": "string"},
                            "retention": {"type": "string", "default": "P30D"}
                        },
                        "required": ["topicRef"]
                    }
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"retention": "P90D"}
                }),
            ),
        ];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            err.iter().any(|e| e.contains("type: string")),
            "error message should include expected type: {err:?}"
        );
    }

    #[test]
    fn test_empty_trait_schema_permits_any_traits() {
        // An empty x-gts-traits-schema: {} is unconstrained — any trait values pass
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {}
                }),
            ),
            (
                "derived~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"anything": "goes", "count": 42}
                }),
            ),
        ];
        assert!(
            validate_traits_chain(&chain).is_ok(),
            "empty trait schema should permit any traits"
        );
    }

    #[test]
    fn test_duplicate_property_dedup_rightmost_wins() {
        // Base defines `priority: string`, mid narrows to enum.
        // The dedup should keep the enum definition (rightmost), not report
        // "priority" as unresolved twice.
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {"type": "string"},
                            "retention": {"type": "string", "default": "P30D"}
                        }
                    }
                }),
            ),
            (
                "mid~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {
                                "type": "string",
                                "enum": ["low", "medium", "high"]
                            }
                        }
                    }
                }),
            ),
            (
                "leaf~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits": {"priority": "high"}
                }),
            ),
        ];
        // Should pass: priority is provided, retention has default
        assert!(
            validate_traits_chain(&chain).is_ok(),
            "dedup should keep rightmost definition"
        );

        // Verify the unresolved-property check dedups: priority is declared in
        // two layers but the trait-specific "is not resolved" message must be
        // emitted only once (not per declaration). priority is required here so
        // the completeness check fires.
        let chain_missing = vec![
            (
                "base~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {"type": "string"},
                            "retention": {"type": "string", "default": "P30D"}
                        },
                        "required": ["priority"]
                    }
                }),
            ),
            (
                "mid~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {
                                "type": "string",
                                "enum": ["low", "medium", "high"]
                            }
                        }
                    }
                }),
            ),
            (
                "leaf~".to_owned(),
                json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object"}),
            ),
        ];
        let err = validate_traits_chain(&chain_missing).unwrap_err();
        // Isolate the manual completeness message (the standard JSON Schema
        // validator separately reports the missing `required` member).
        let unresolved_priority: Vec<_> = err
            .iter()
            .filter(|e| e.contains("priority") && e.contains("is not resolved"))
            .collect();
        assert_eq!(
            unresolved_priority.len(),
            1,
            "priority should be reported as unresolved exactly once, got: {unresolved_priority:?}"
        );
    }

    #[test]
    fn test_invalid_trait_schema_caught_early() {
        // x-gts-traits-schema with an invalid "type" value should fail early
        // with a clear message about being an invalid JSON Schema
        let chain = vec![(
            "base~".to_owned(),
            json!({"$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "invalid_type_value"
                }
            }),
        )];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.contains("not a valid JSON Schema") || e.contains("failed to compile")),
            "should report invalid JSON Schema early: {err:?}"
        );
    }

    #[test]
    fn test_chain_default_leaf_wins_over_ancestor() {
        // Three-level chain — base, mid, leaf — each redeclares the same trait
        // property's `default` to a different value. No x-gts-traits is supplied
        // anywhere in the chain. Materialization must pick the LEAF-most default,
        // because (a) defaults are JSON Schema annotations that don't participate
        // in narrowing and (b) `collect_all_properties` dedup keeps the last
        // occurrence along the root→leaf-ordered `allOf`.
        let base_ts = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "retention": {"type": "string", "default": "P30D"}
            }
        });
        let mid_ts = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "retention": {"type": "string", "default": "P90D"}
            }
        });
        let leaf_ts = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "retention": {"type": "string", "default": "P365D"}
            }
        });

        let effective = build_effective_traits_schema(&[base_ts, mid_ts, leaf_ts]);
        let materialized = materialize_traits(&effective, &Value::Object(serde_json::Map::new()));

        let retention = materialized
            .as_object()
            .and_then(|m| m.get("retention"))
            .and_then(Value::as_str)
            .expect("retention should be present after materialization");
        assert_eq!(
            retention, "P365D",
            "leaf-most default must win; got {retention}"
        );
    }

    #[test]
    fn test_chain_default_explicit_value_wins_over_defaults() {
        // Same 3-level chain as above, but mid supplies an explicit value via
        // x-gts-traits. After RFC 7396 chain merge, retention is set; the
        // materialization step must NOT clobber it with the leaf's default.
        let base_ts = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "retention": {"type": "string", "default": "P30D"}
            }
        });
        let mid_ts = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "retention": {"type": "string", "default": "P90D"}
            }
        });
        let leaf_ts = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "retention": {"type": "string", "default": "P365D"}
            }
        });

        let effective = build_effective_traits_schema(&[base_ts, mid_ts, leaf_ts]);
        let mut merged = serde_json::Map::new();
        merged.insert("retention".to_owned(), Value::String("P42D".to_owned()));
        let materialized = materialize_traits(&effective, &Value::Object(merged));

        let retention = materialized
            .as_object()
            .and_then(|m| m.get("retention"))
            .and_then(Value::as_str)
            .expect("retention should be present after materialization");
        assert_eq!(
            retention, "P42D",
            "explicit chain-merged value must override all defaults; got {retention}"
        );
    }

    #[test]
    fn test_chain_default_null_delete_restores_leaf_default() {
        // Chain where ancestor sets the value and descendant deletes it via
        // RFC 7396 null. After the cross-level merge the key is absent;
        // materialization should fill it from the leaf-most default declaration.
        // This is the "null reverts to the schema default" path documented for
        // the merge strategy.
        let base_ts = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "retention": {"type": "string", "default": "P30D"}
            }
        });
        let leaf_ts = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "retention": {"type": "string", "default": "P365D"}
            }
        });

        // Simulate chain merge: base sets retention=P7D, leaf deletes via null
        // → merged is empty. The patches are `x-gts-traits` value objects, which
        // never carry `$schema`.
        let mut merged = serde_json::Map::new();
        merge_rfc7396_into(
            &mut merged,
            json!({"retention": "P7D"}).as_object().unwrap(),
        );
        merge_rfc7396_into(&mut merged, json!({"retention": null}).as_object().unwrap());
        assert!(
            !merged.contains_key("retention"),
            "null patch should remove the key from merged"
        );

        let effective = build_effective_traits_schema(&[base_ts, leaf_ts]);
        let materialized = materialize_traits(&effective, &Value::Object(merged));

        let retention = materialized
            .as_object()
            .and_then(|m| m.get("retention"))
            .and_then(Value::as_str)
            .expect("retention should be restored from the leaf default");
        assert_eq!(
            retention, "P365D",
            "after null delete, materialization must use the leaf-most default; got {retention}"
        );
    }

    #[test]
    fn test_build_effective_traits_materializes_and_pins_dialect() {
        let ts = json!({
            "type": "object",
            "properties": { "retention": {"type": "string", "default": "P30D"} }
        });
        let merged = json!({});
        let traits = super::build_effective_traits(
            std::slice::from_ref(&ts),
            &merged,
            Some("http://json-schema.org/draft-07/schema#"),
        );
        assert_eq!(
            traits.schema["$schema"],
            "http://json-schema.org/draft-07/schema#"
        );
        assert_eq!(traits.values["retention"], "P30D"); // default materialized
    }

    #[test]
    fn test_validate_trait_values_flags_x_gts_ref_violation() {
        let schema = json!({
            "type": "object",
            "properties": {
                "topicRef": {"type": "string", "x-gts-ref": "gts.x.core.events.topic.v1~"}
            }
        });
        // A value that does not match the required gts prefix must be reported,
        // even though the standard jsonschema validator ignores x-gts-ref.
        let values = json!({ "topicRef": "not-a-gts-id" });
        let res = super::validate_trait_values(&schema, &values, false);
        assert!(
            res.is_err(),
            "x-gts-ref violation should be reported: {res:?}"
        );
    }

    #[test]
    fn test_inline_local_pointers_resolves_against_root() {
        let root = json!({
            "$defs": { "Retention": {"type": "string", "enum": ["P30D"]} },
            "x-gts-traits-schema": {
                "type": "object",
                "properties": { "retention": {"$ref": "#/$defs/Retention"} }
            }
        });
        let fragment = &root["x-gts-traits-schema"];
        let inlined = super::inline_local_pointers(fragment, &root);
        let retention = &inlined["properties"]["retention"];
        assert!(
            retention.get("$ref").is_none(),
            "local pointer must be inlined: {retention}"
        );
        assert_eq!(retention["enum"], json!(["P30D"]));
    }

    #[test]
    fn test_inline_local_pointers_leaves_unresolvable_and_gts_refs_untouched() {
        let root = json!({ "type": "object" }); // no $defs
        // A `gts://` ref and an unresolvable `#/...` pointer are both left as-is
        // (the former is handled later by try_resolve_schema_refs; the
        // latter has no target in `root`).
        let fragment = json!({
            "allOf": [
                {"$ref": "gts://gts.x.a.b.v1~"},
                {"$ref": "#/$defs/Missing"}
            ]
        });
        let inlined = super::inline_local_pointers(&fragment, &root);
        assert_eq!(inlined["allOf"][0]["$ref"], "gts://gts.x.a.b.v1~");
        assert_eq!(inlined["allOf"][1]["$ref"], "#/$defs/Missing");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod inline_traits_schema_tests {
    use super::*;
    use crate::gts::GtsInstanceId;
    use schemars::JsonSchema;

    /// Recursively collect every `$ref` string found anywhere in the value.
    fn collect_refs(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(map) => {
                if let Some(Value::String(r)) = map.get("$ref") {
                    out.push(r.clone());
                }
                for val in map.values() {
                    collect_refs(val, out);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    collect_refs(val, out);
                }
            }
            _ => {}
        }
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    enum SeverityLevel {
        Low,
        High,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct EnumFieldTraits {
        level: SeverityLevel,
    }

    /// A non-primitive field (here an enum) must be inlined into the embedded
    /// fragment, not left as a `$ref` into a `$defs` block that the fragment
    /// does not carry. Otherwise `x-gts-traits-schema` is structurally broken
    /// and fails when a JSON Schema validator tries to resolve the dangling ref.
    #[test]
    fn enum_field_is_inlined_with_no_dangling_refs() {
        let schema = inline_traits_schema_of::<EnumFieldTraits>();

        // The embedded fragment must be self-contained: no $defs block...
        assert!(
            schema.get("$defs").is_none(),
            "embedded fragment must not carry a $defs block: {schema}"
        );
        // ...and therefore no $ref anywhere pointing into one.
        let mut refs = Vec::new();
        collect_refs(&schema, &mut refs);
        assert!(
            refs.is_empty(),
            "embedded fragment has dangling refs: {refs:?} in {schema}"
        );

        // The enum's variants must actually be present inline.
        let serialized = schema.to_string();
        assert!(
            serialized.contains("Low") && serialized.contains("High"),
            "enum variants should be inlined into the fragment: {schema}"
        );
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct InstanceIdTraits {
        topic_ref: GtsInstanceId,
    }

    /// Regression: the canonical `GtsInstanceId` representation (its inline
    /// `x-gts-ref` body) must survive, with no dangling ref left behind.
    #[test]
    fn gts_instance_id_field_keeps_canonical_inline_form() {
        let schema = inline_traits_schema_of::<InstanceIdTraits>();

        let mut refs = Vec::new();
        collect_refs(&schema, &mut refs);
        assert!(
            refs.is_empty(),
            "unexpected dangling refs: {refs:?} in {schema}"
        );

        let prop = &schema["properties"]["topic_ref"];
        assert_eq!(prop["type"], "string");
        assert_eq!(prop["format"], "gts-instance-id");
        assert_eq!(prop["x-gts-ref"], "gts.*");
    }
}
