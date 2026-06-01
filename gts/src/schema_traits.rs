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
//! 3. Apply defaults from the effective trait schema to fill unresolved trait
//!    properties (materialization step).
//! 4. Validate the effective traits object against the effective trait schema
//!    (completeness check runs only when the type is non-abstract — see
//!    `store.rs::validate_schema_traits`).
//!
//! **Override semantics (RFC 7396 JSON Merge Patch):**
//! - Scalars: descendant value wins (last-wins).
//! - Objects: deep-merge recursively (keys not restated by the descendant are
//!   preserved from the ancestor).
//! - Arrays: replace wholesale (no element-wise merge).
//! - `null` at any depth deletes the key, after which `apply_defaults` may
//!   re-substitute a default.
//! - Locking publisher-controlled values is done via JSON Schema `const` in
//!   `x-gts-traits-schema`; the registry carries no GTS-specific immutability
//!   rule.
//!
//! **Empty trait schemas:** If a schema in the chain declares
//! `x-gts-traits-schema: {}` or `true`, it contributes an unconstrained
//! sub-schema. `false` contributes a sub-schema that rejects all values; a
//! type whose effective schema is `false` and which carries no traits passes
//! (nothing is validated), but any non-empty trait value fails.

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
/// integration uses [`validate_effective_traits`] directly after collecting
/// and resolving trait schemas itself.
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
    validate_effective_traits(&trait_schemas, &Value::Object(merged), true, dialect)
}

/// Validates trait values against the effective trait schema built from the
/// given list of resolved trait schemas.
///
/// `resolved_trait_schemas` – `x-gts-traits-schema` values collected from the
/// chain, with any `$ref` inside them already resolved.
///
/// `merged_traits` – shallow-merged `x-gts-traits` values (rightmost wins).
///
/// When `check_unresolved` is `true`, every *required* trait-schema property
/// without a default must have a value in `merged_traits` (optional properties
/// may be left unresolved, per README §9.7.5 / OP#13); set to `false` for
/// intermediate schema validation where descendants may still supply values.
///
/// `dialect` is the host document's `$schema` URI (e.g.
/// `http://json-schema.org/draft-07/schema#`). When `Some`, it pins the JSON
/// Schema draft used to validate trait values so they are interpreted under the
/// same dialect as the rest of the type schema (README §9.10) — necessary
/// because the inline trait fragment had its root-only `$schema` stripped when
/// embedded. When `None`, validation falls back to the validator's automatic
/// draft detection (Draft 2020-12 when no `$schema` is present), matching how
/// instance and schema validation behave elsewhere in this crate. A GTS Type
/// Schema always declares `$schema`, so the store path always passes `Some`.
///
/// # Errors
/// Returns `Vec<String>` of error messages if trait values don't conform to the
/// effective trait schema, if required traits are missing, or if traits exist
/// without a trait schema in the chain.
pub fn validate_effective_traits(
    resolved_trait_schemas: &[Value],
    merged_traits: &Value,
    check_unresolved: bool,
    dialect: Option<&str>,
) -> Result<(), Vec<String>> {
    let has_trait_values = merged_traits.as_object().is_some_and(|m| !m.is_empty());

    if resolved_trait_schemas.is_empty() {
        if has_trait_values {
            return Err(vec![format!(
                "{X_GTS_TRAITS} values provided but no {X_GTS_TRAITS_SCHEMA} is defined in the \
                 inheritance chain"
            )]);
        }
        return Ok(());
    }

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

    let mut effective_trait_schema = build_effective_trait_schema(resolved_trait_schemas);

    // If any subschema in the chain is the boolean `false`, the effective
    // schema is unsatisfiable. A type that carries no traits at all is still
    // valid (`false` prohibits traits, not the existence of typed descendants).
    // A type carrying any traits fails.
    if effective_schema_is_false(&effective_trait_schema) {
        if has_trait_values {
            return Err(vec![format!(
                "{X_GTS_TRAITS_SCHEMA} resolves to `false` in the chain — \
                 {X_GTS_TRAITS} values are prohibited"
            )]);
        }
        return Ok(());
    }

    // Pin the JSON Schema dialect to the host document's `$schema` so trait
    // values validate under the same draft as the rest of the type schema
    // (README §9.10: the dialect is set by `$schema`). The inline trait fragment
    // had its root-only `$schema` stripped when embedded, so we (re)set it from
    // the host here. When the caller supplies no dialect, we leave the schema as
    // is and let the validator detect/default the draft (Draft 2020-12), matching
    // instance/schema validation elsewhere in this crate.
    if let Some(dialect) = dialect
        && let Some(obj) = effective_trait_schema.as_object_mut()
    {
        obj.insert("$schema".to_owned(), Value::String(dialect.to_owned()));
    }

    let effective_traits = apply_defaults(&effective_trait_schema, merged_traits);

    let mut errors = match validate_traits_against_schema(
        &effective_trait_schema,
        &effective_traits,
        check_unresolved,
    ) {
        Ok(()) => Vec::new(),
        Err(e) => e,
    };

    // Enforce `x-gts-ref` on trait values (README §9.6). The standard
    // `jsonschema` validator ignores `x-gts-ref` as an unknown keyword, so a
    // trait value that violates the declared GTS-prefix would otherwise slip
    // through. Treat the effective trait-schema as the schema and the
    // materialized effective traits as the instance.
    let xref = crate::x_gts_ref::XGtsRefValidator::new();
    for err in xref.validate_instance(&effective_traits, &effective_trait_schema, "") {
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
/// `store::validate_schema_traits`) intact. Within a single level, multiple
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
///   `apply_defaults` can later substitute a `default` from the trait schema).
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
fn build_effective_trait_schema(schemas: &[Value]) -> Value {
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

/// Apply JSON Schema `default` values from the effective trait schema to the
/// merged traits object for any properties that are not yet present.
///
/// Handles nested object properties recursively: if a trait property is an object
/// type with its own `properties` and `default` values, those are applied to the
/// corresponding nested object in the traits.
fn apply_defaults(trait_schema: &Value, traits: &Value) -> Value {
    apply_defaults_recursive(trait_schema, traits, 0)
}

fn apply_defaults_recursive(trait_schema: &Value, traits: &Value, depth: usize) -> Value {
    if depth >= MAX_RECURSION_DEPTH {
        return traits.clone();
    }

    let mut result = match traits {
        Value::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };

    // Collect properties from the trait schema (may be in top-level or allOf)
    let props = collect_all_properties(trait_schema);

    for (prop_name, prop_schema) in &props {
        if let Some(prop_obj) = prop_schema.as_object() {
            if !result.contains_key(prop_name.as_str()) {
                // Property is absent — apply top-level default if present
                if let Some(default_val) = prop_obj.get("default") {
                    result.insert(prop_name.clone(), default_val.clone());
                }
            } else if prop_obj.get("type") == Some(&Value::String("object".to_owned()))
                && prop_obj.contains_key("properties")
            {
                // Property is present and is an object type with sub-properties —
                // recurse to apply nested defaults.  If the input value is a
                // non-object (e.g. a string where the schema expects an object),
                // the recursion will produce a defaulted object that replaces the
                // original value; JSON Schema validation will catch the type
                // mismatch later, so this is intentional.
                let nested = apply_defaults_recursive(
                    prop_schema,
                    result.get(prop_name.as_str()).unwrap_or(&Value::Null),
                    depth + 1,
                );
                result.insert(prop_name.clone(), nested);
            }
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

        let has_default = prop_schema
            .as_object()
            .is_some_and(|m| m.contains_key("default"));

        if !has_value && !has_default {
            let expected_type = prop_schema
                .as_object()
                .and_then(|m| m.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("any");
            errors.push(format!(
                "trait property '{prop_name}' (type: {expected_type}) is not resolved: \
                 no value provided and no default defined in the trait schema. \
                 All traits must be resolved (via a {X_GTS_TRAITS} value in the chain \
                 or a `default` in the trait schema) on non-abstract types; otherwise \
                 mark the type abstract (x-gts-abstract: true)"
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

        let effective = build_effective_trait_schema(&[base_ts, mid_ts, leaf_ts]);
        let materialized = apply_defaults(&effective, &Value::Object(serde_json::Map::new()));

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

        let effective = build_effective_trait_schema(&[base_ts, mid_ts, leaf_ts]);
        let mut merged = serde_json::Map::new();
        merged.insert("retention".to_owned(), Value::String("P42D".to_owned()));
        let materialized = apply_defaults(&effective, &Value::Object(merged));

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
        // → merged is empty.
        let mut merged = serde_json::Map::new();
        merge_rfc7396_into(
            &mut merged,
            json!({"$schema": "http://json-schema.org/draft-07/schema#", "retention": "P7D"})
                .as_object()
                .unwrap(),
        );
        merge_rfc7396_into(
            &mut merged,
            json!({"$schema": "http://json-schema.org/draft-07/schema#", "retention": null})
                .as_object()
                .unwrap(),
        );
        assert!(
            !merged.contains_key("retention"),
            "null patch should remove the key from merged"
        );

        let effective = build_effective_trait_schema(&[base_ts, leaf_ts]);
        let materialized = apply_defaults(&effective, &Value::Object(merged));

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
}
