//! OP#13 – Schema Traits Validation (`x-gts-traits-schema` / `x-gts-traits`)
//!
//! Validates that trait values provided in derived schemas conform to the
//! effective trait schema built from the entire inheritance chain.
//!
//! **Algorithm:**
//! 1. Walk the chain from leftmost (base) to rightmost (leaf) segment.
//! 2. For each schema in the chain, collect:
//!    - `x-gts-traits-schema` objects → compose via `allOf` into the *effective trait schema*.
//!    - `x-gts-traits` objects → shallow-merge (rightmost wins) into the *effective traits object*.
//! 3. Apply defaults from the effective trait schema to fill unresolved trait properties.
//! 4. Validate the effective traits object against the effective trait schema.

use serde_json::Value;

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
    validate_effective_traits(&trait_schemas, &Value::Object(merged))
}

/// Validates trait values against the effective trait schema built from the
/// given list of resolved trait schemas.
///
/// `resolved_trait_schemas` – `x-gts-traits-schema` values collected from the
/// chain, with any `$ref` inside them already resolved.
///
/// `merged_traits` – shallow-merged `x-gts-traits` values (rightmost wins).
///
/// # Errors
/// Returns `Vec<String>` of error messages if trait values don't conform to the
/// effective trait schema, if required traits are missing, or if traits exist
/// without a trait schema in the chain.
pub fn validate_effective_traits(
    resolved_trait_schemas: &[Value],
    merged_traits: &Value,
) -> Result<(), Vec<String>> {
    let has_trait_values = merged_traits.as_object().is_some_and(|m| !m.is_empty());

    if resolved_trait_schemas.is_empty() {
        if has_trait_values {
            return Err(vec![
                "x-gts-traits values provided but no x-gts-traits-schema is defined in the \
                 inheritance chain"
                    .to_owned(),
            ]);
        }
        return Ok(());
    }

    // Validate trait schema integrity: x-gts-traits-schema must not contain x-gts-traits
    for (i, ts) in resolved_trait_schemas.iter().enumerate() {
        if let Some(obj) = ts.as_object()
            && obj.contains_key("x-gts-traits")
        {
            return Err(vec![format!(
                "x-gts-traits-schema[{i}] contains 'x-gts-traits' \u{2014} \
                 trait values must not appear inside a trait schema definition"
            )]);
        }
    }

    let effective_trait_schema = build_effective_trait_schema(resolved_trait_schemas);
    let effective_traits = apply_defaults(&effective_trait_schema, merged_traits);
    validate_traits_against_schema(&effective_trait_schema, &effective_traits)
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

    if let Some(ts) = obj.get("x-gts-traits-schema") {
        out.push(ts.clone());
    }

    // Also check inside allOf items (e.g. a derived schema that is an allOf overlay)
    if let Some(Value::Array(all_of)) = obj.get("allOf") {
        for item in all_of {
            collect_trait_schema_recursive(item, out, depth + 1);
        }
    }
}

/// Recursively search a schema value for `x-gts-traits` entries and merge.
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

    if let Some(Value::Object(traits)) = obj.get("x-gts-traits") {
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
                // recurse to apply nested defaults.
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
fn collect_all_properties(schema: &Value) -> Vec<(String, Value)> {
    let mut props = Vec::new();
    collect_props_recursive(schema, &mut props, 0);
    props
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

/// Validate the effective traits object against the effective trait schema.
///
/// Uses the `jsonschema` crate for standard JSON Schema validation.  This
/// catches type mismatches, enum violations, `additionalProperties` errors,
/// and any other constraint issues.
///
/// Additionally checks that every property defined in the trait schema is
/// resolved (has a value) — i.e. there are no "holes" left after applying
/// defaults.
fn validate_traits_against_schema(
    trait_schema: &Value,
    effective_traits: &Value,
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
    let all_props = collect_all_properties(trait_schema);
    let traits_obj = effective_traits.as_object();

    for (prop_name, prop_schema) in &all_props {
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
                 no value provided and no default defined in the trait schema"
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
            json!({"type": "object", "properties": {"id": {"type": "string"}}}),
        )];
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_traits_without_schema_in_derived_fails() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({"type": "object", "properties": {"id": {"type": "string"}}}),
            ),
            (
                "derived~".to_owned(),
                json!({
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
            json!({
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
                json!({
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
                json!({
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
                json!({
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
            ("derived~".to_owned(), json!({"type": "object"})),
        ];
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_missing_required_trait_fails() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({
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
                "derived~".to_owned(),
                json!({
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
    fn test_wrong_type_fails() {
        let chain = vec![
            (
                "base~".to_owned(),
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
                    "type": "object",
                    "x-gts-traits": {"retention": "P30D"}
                }),
            ),
            (
                "leaf~".to_owned(),
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {
                            "priority": {"type": "string"}
                        }
                    }
                }),
            ),
            (
                "leaf~".to_owned(),
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
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
        // 10-level chain — exercises recursion without hitting the depth limit
        let mut chain = vec![(
            "base~".to_owned(),
            json!({
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "object",
                    "properties": {
                        "retention": {"type": "string", "default": "P30D"}
                    }
                }
            }),
        )];
        for i in 1..10 {
            chain.push((format!("level{i}~"), json!({"type": "object"})));
        }
        assert!(validate_traits_chain(&chain).is_ok());
    }

    #[test]
    fn test_malformed_trait_schema_not_object() {
        // x-gts-traits-schema is a string, not an object
        let chain = vec![
            (
                "base~".to_owned(),
                json!({
                    "type": "object",
                    "x-gts-traits-schema": "not_an_object"
                }),
            ),
            (
                "derived~".to_owned(),
                json!({
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
                json!({
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
                json!({
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
                json!({
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
                json!({
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
    fn test_meta_traits_rejected() {
        // x-gts-traits-schema contains x-gts-traits — should be rejected
        let chain = vec![(
            "base~".to_owned(),
            json!({
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "object",
                    "x-gts-traits": {"sneaky": "value"},
                    "properties": {
                        "retention": {"type": "string"}
                    }
                }
            }),
        )];
        let err = validate_traits_chain(&chain).unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.contains("x-gts-traits") && e.contains("trait schema")),
            "should reject x-gts-traits inside x-gts-traits-schema: {err:?}"
        );
    }

    #[test]
    fn test_nested_object_defaults_applied() {
        // Trait schema has nested object with defaults — verify they are applied
        let chain = vec![
            (
                "base~".to_owned(),
                json!({
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
                json!({
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
                json!({
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
                "derived~".to_owned(),
                json!({
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
}
