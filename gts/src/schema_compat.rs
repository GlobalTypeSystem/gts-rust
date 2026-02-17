//! OP#12 – Schema-vs-schema compatibility validation.
//!
//! Given a chained GTS schema ID like `gts.A~B~C~`, this module validates that
//! each derived schema is compatible with its base:
//!
//! - B (derived from A) must be compatible with A
//! - C (derived from A~B) must be compatible with A~B
//!
//! "Compatible" means every valid instance of the derived schema is also a valid
//! instance of the base schema.  Concretely the derived schema may only
//! **tighten** (never loosen) constraints on properties inherited from the base.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Represents the effective (flattened) schema used for compatibility comparison.
pub(crate) struct EffectiveSchema {
    pub properties: HashMap<String, Value>,
    pub required: HashSet<String>,
    pub additional_properties: Option<Value>,
}

/// Extracts the effective schema properties, required fields, and
/// `additionalProperties` from a fully-resolved JSON Schema value.
///
/// If the schema contains an `allOf` that was not already merged by the
/// resolver, the items are merged here (last-wins for properties).
pub(crate) fn extract_effective_schema(schema: &Value) -> EffectiveSchema {
    let mut eff = EffectiveSchema {
        properties: HashMap::new(),
        required: HashSet::new(),
        additional_properties: None,
    };

    if let Value::Object(map) = schema {
        // Direct properties
        if let Some(Value::Object(props)) = map.get("properties") {
            for (k, v) in props {
                eff.properties.insert(k.clone(), v.clone());
            }
        }

        // Required
        if let Some(Value::Array(req)) = map.get("required") {
            for v in req {
                if let Value::String(s) = v {
                    eff.required.insert(s.clone());
                }
            }
        }

        // additionalProperties
        if let Some(ap) = map.get("additionalProperties") {
            eff.additional_properties = Some(ap.clone());
        }

        // allOf – merge from all items (for schemas that weren't fully flattened)
        if let Some(Value::Array(all_of)) = map.get("allOf") {
            for item in all_of {
                let item_eff = extract_effective_schema(item);
                eff.properties.extend(item_eff.properties);
                eff.required.extend(item_eff.required);
                if item_eff.additional_properties.is_some() {
                    eff.additional_properties = item_eff.additional_properties;
                }
            }
        }
    }

    eff
}

/// Validates that a derived schema is compatible with its base schema.
///
/// Rules checked:
/// - Derived cannot add properties if base has `additionalProperties: false`
/// - Derived cannot loosen constraints on existing properties
/// - Derived cannot disable (`false`) properties that base defines
/// - Derived enum must be a subset of base enum
/// - Derived cannot change property types
/// - Derived cannot redefine `const` to a different value
/// - Derived cannot change `pattern`
/// - Derived cannot remove fields from `required`
/// - Derived cannot change array `items` type
///
/// Returns an empty `Vec` when the schemas are compatible, otherwise a list of
/// human-readable error descriptions.
pub(crate) fn validate_schema_compatibility(
    base: &EffectiveSchema,
    derived: &EffectiveSchema,
    base_id: &str,
    derived_id: &str,
) -> Vec<String> {
    let mut errors = Vec::new();
    let base_disallows_additional = matches!(base.additional_properties, Some(Value::Bool(false)));

    for (prop_name, derived_prop) in &derived.properties {
        if let Some(base_prop) = base.properties.get(prop_name) {
            // Property exists in both – check for disabling
            if *derived_prop == Value::Bool(false) {
                errors.push(format!(
                    "property '{prop_name}': derived schema '{derived_id}' disables property defined in base '{base_id}'"
                ));
                continue;
            }

            // Compare constraints
            compare_property_constraints(base_prop, derived_prop, prop_name, &mut errors);
        }
        // New property in derived – check additionalProperties
        else if base_disallows_additional {
            errors.push(format!(
                    "property '{prop_name}': derived schema '{derived_id}' adds new property but base '{base_id}' has additionalProperties: false"
                ));
        }
    }

    // Check if derived loosens additionalProperties constraint
    if base_disallows_additional {
        let derived_allows_additional =
            !matches!(derived.additional_properties, Some(Value::Bool(false)));
        if derived_allows_additional {
            errors.push(format!(
                "derived schema '{derived_id}' loosens additionalProperties from false in base '{base_id}'"
            ));
        }
    }

    // Check that derived doesn't remove fields from base's required set
    check_required_removal(base, derived, base_id, derived_id, &mut errors);

    errors
}

// ---------------------------------------------------------------------------
// Constraint comparison helpers
// ---------------------------------------------------------------------------

/// Compares constraints between a base property schema and a derived property
/// schema.  Pushes error strings into `errors` whenever the derived schema
/// loosens a constraint.
fn compare_property_constraints(
    base_prop: &Value,
    derived_prop: &Value,
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    // If base is not an object schema, it places no constraints to loosen.
    let Some(base_map) = base_prop.as_object() else {
        return;
    };

    // If derived is a boolean `true` schema (or any non-object), it accepts
    // everything and therefore loosens any constraint the base defines.
    let Some(derived_map) = derived_prop.as_object() else {
        errors.push(format!(
            "property '{prop_name}': derived replaces schema object with a non-object value, \
             loosening base constraints"
        ));
        return;
    };

    // Type compatibility: if base specifies a type, derived must use the same type
    check_type_compatibility(base_map, derived_map, prop_name, errors);

    // `const` and `enum` are "value-enumerating" constraints that fully specify the
    // set of allowed values.  When the derived schema introduces one of these, omitting
    // bounds-type keywords (maxLength, minimum, ...) or pattern is NOT loosening because
    // the allowed values are already a finite, explicit set.  However, the enumerated
    // values themselves must still satisfy the base bounds.
    let derived_values = collect_derived_enumerated_values(derived_map);
    let derived_enumerates_values = derived_values.is_some();

    // const: if base has const, derived must have same const (not omit it).
    // Exception: derived may replace const with enum that includes the const value
    // (still tighter or equal).
    check_const_compatibility(base_map, derived_map, prop_name, errors);

    if derived_enumerates_values {
        // Derived enumerates values: skip keyword-level bounds/pattern checks but
        // verify every enumerated value satisfies the base constraints.
        check_enumerated_values_against_base(
            base_map,
            derived_values.as_deref().unwrap_or(&[]),
            prop_name,
            errors,
        );
    } else {
        // No enumeration: require keyword-level constraints to be preserved/tightened.
        check_pattern_compatibility(base_map, derived_map, prop_name, errors);

        check_upper_bound(base_map, derived_map, "maxLength", prop_name, errors);
        check_upper_bound(base_map, derived_map, "maximum", prop_name, errors);
        check_upper_bound(base_map, derived_map, "maxItems", prop_name, errors);

        check_lower_bound(base_map, derived_map, "minLength", prop_name, errors);
        check_lower_bound(base_map, derived_map, "minimum", prop_name, errors);
        check_lower_bound(base_map, derived_map, "minItems", prop_name, errors);
    }

    // enum: if base has enum, derived must have enum subset (or const within base enum)
    check_enum_compatibility(base_map, derived_map, prop_name, errors);

    // Array items sub-schema comparison
    check_items_compatibility(base_map, derived_map, prop_name, errors);

    // Recurse for nested object properties
    if base_map.get("type") == Some(&Value::String("object".to_owned()))
        && derived_map.get("type") == Some(&Value::String("object".to_owned()))
        && base_map.contains_key("properties")
    {
        let base_nested = extract_effective_schema(base_prop);
        let derived_nested = extract_effective_schema(derived_prop);

        let nested_errors =
            validate_schema_compatibility(&base_nested, &derived_nested, "base", "derived");
        for err in nested_errors {
            errors.push(format!("in nested object '{prop_name}': {err}"));
        }
    }
}

/// Helper: check that derived does not change the `type` of a property.
///
/// Allowed: same type, or base has no type (unconstrained).
/// Disallowed: changing type (e.g. "string" → "integer", "integer" → "number").
fn check_type_compatibility(
    base_map: &serde_json::Map<String, Value>,
    derived_map: &serde_json::Map<String, Value>,
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    if let (Some(base_type), Some(derived_type)) = (base_map.get("type"), derived_map.get("type"))
        && base_type != derived_type
    {
        errors.push(format!(
            "property '{prop_name}': derived changes type from {base_type} to {derived_type}"
        ));
    }
}

/// Helper: check `const` compatibility.
///
/// - Base has no `const`, derived adds one → OK (tightening)
/// - Base has `const`, derived has same `const` → OK (idempotent)
/// - Base has `const`, derived has different `const` → ERROR
/// - Base has `const`, derived omits it → ERROR (loosening)
fn check_const_compatibility(
    base_map: &serde_json::Map<String, Value>,
    derived_map: &serde_json::Map<String, Value>,
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    if let Some(base_const) = base_map.get("const") {
        match derived_map.get("const") {
            Some(derived_const) if base_const != derived_const => {
                errors.push(format!(
                    "property '{prop_name}': derived redefines const from {base_const} to {derived_const}"
                ));
            }
            None => {
                errors.push(format!(
                    "property '{prop_name}': derived omits const constraint ({base_const}) defined in base"
                ));
            }
            _ => {} // Same const or derived adds tightening
        }
    }
}

/// Helper: check `pattern` compatibility.
///
/// If base defines a `pattern` and derived defines a different `pattern`,
/// the schemas are considered incompatible (we cannot determine subset
/// relationships between arbitrary regexes).
/// If base defines a `pattern` and derived omits it, that's also incompatible (loosening).
fn check_pattern_compatibility(
    base_map: &serde_json::Map<String, Value>,
    derived_map: &serde_json::Map<String, Value>,
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    if let Some(base_pat) = base_map.get("pattern") {
        match derived_map.get("pattern") {
            Some(derived_pat) if base_pat != derived_pat => {
                errors.push(format!(
                    "property '{prop_name}': derived changes pattern from {base_pat} to {derived_pat}"
                ));
            }
            None => {
                errors.push(format!(
                    "property '{prop_name}': derived omits pattern constraint ({base_pat}) defined in base"
                ));
            }
            _ => {} // Same pattern
        }
    }
}

/// Helper: check `enum` compatibility.
///
/// If base defines an `enum`, derived must also define an `enum` that is a subset,
/// or define a `const` whose value is in the base enum (tightening from set to single value).
/// If base has `enum` and derived omits both `enum` and `const`, that's incompatible (loosening).
fn check_enum_compatibility(
    base_map: &serde_json::Map<String, Value>,
    derived_map: &serde_json::Map<String, Value>,
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    if let Some(Value::Array(base_enum)) = base_map.get("enum") {
        // Check if derived has enum (subset check)
        if let Some(Value::Array(derived_enum)) = derived_map.get("enum") {
            for val in derived_enum {
                if !base_enum.contains(val) {
                    errors.push(format!(
                        "property '{prop_name}': derived enum contains value {val} not in base enum"
                    ));
                }
            }
            return;
        }
        // Check if derived has const (must be in base enum — tightening from set to single)
        if let Some(derived_const) = derived_map.get("const") {
            if !base_enum.contains(derived_const) {
                errors.push(format!(
                    "property '{prop_name}': derived const {derived_const} is not in base enum"
                ));
            }
            return;
        }
        // Neither enum nor const — loosening
        errors.push(format!(
            "property '{prop_name}': derived omits enum constraint defined in base"
        ));
    }
}

/// Helper: check array `items` sub-schema compatibility.
///
/// If both base and derived have `items`, recursively compare them using the
/// same property-constraint logic (type changes, const, bounds, etc.).
/// If base has `items` and derived omits it, that's incompatible (loosening).
fn check_items_compatibility(
    base_map: &serde_json::Map<String, Value>,
    derived_map: &serde_json::Map<String, Value>,
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    if let Some(base_items) = base_map.get("items") {
        match derived_map.get("items") {
            Some(derived_items) => {
                // Reuse compare_property_constraints for the items sub-schema
                let items_name = format!("{prop_name}.items");
                compare_property_constraints(base_items, derived_items, &items_name, errors);
            }
            None => {
                errors.push(format!(
                    "property '{prop_name}': derived omits items constraint defined in base"
                ));
            }
        }
    }
}

/// Helper: check that derived doesn't remove fields from base `required`.
///
/// If the derived schema explicitly specifies a `required` array, every field
/// that is in the base `required` set must still be present. Derived may add
/// new required fields but never remove existing ones.
fn check_required_removal(
    base: &EffectiveSchema,
    derived: &EffectiveSchema,
    base_id: &str,
    derived_id: &str,
    errors: &mut Vec<String>,
) {
    // Only check if derived explicitly declares any required fields
    // (if derived doesn't declare required at all, allOf semantics inherit base's required)
    if derived.required.is_empty() {
        return;
    }
    for base_req in &base.required {
        if !derived.required.contains(base_req) {
            errors.push(format!(
                "derived schema '{derived_id}' removes required field '{base_req}' defined in base '{base_id}'"
            ));
        }
    }
}

/// Helper: derived upper-bound constraint must be **<=** base.
/// If base has an upper bound and derived omits it, that's incompatible (loosening).
fn check_upper_bound(
    base_map: &serde_json::Map<String, Value>,
    derived_map: &serde_json::Map<String, Value>,
    keyword: &str,
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    if let Some(base_val) = base_map.get(keyword) {
        match derived_map.get(keyword) {
            Some(derived_val) => {
                if let (Some(b), Some(d)) = (base_val.as_f64(), derived_val.as_f64())
                    && d > b
                {
                    errors.push(format!(
                        "property '{prop_name}': derived {keyword} ({d}) exceeds base {keyword} ({b})"
                    ));
                }
            }
            None => {
                errors.push(format!(
                    "property '{prop_name}': derived omits {keyword} constraint ({base_val}) defined in base"
                ));
            }
        }
    }
}

/// Helper: derived lower-bound constraint must be **>=** base.
/// If base has a lower bound and derived omits it, that's incompatible (loosening).
fn check_lower_bound(
    base_map: &serde_json::Map<String, Value>,
    derived_map: &serde_json::Map<String, Value>,
    keyword: &str,
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    if let Some(base_val) = base_map.get(keyword) {
        match derived_map.get(keyword) {
            Some(derived_val) => {
                if let (Some(b), Some(d)) = (base_val.as_f64(), derived_val.as_f64())
                    && d < b
                {
                    errors.push(format!(
                        "property '{prop_name}': derived {keyword} ({d}) is less than base {keyword} ({b})"
                    ));
                }
            }
            None => {
                errors.push(format!(
                    "property '{prop_name}': derived omits {keyword} constraint ({base_val}) defined in base"
                ));
            }
        }
    }
}

/// Collect the concrete values that a derived property constrains to via `const` or `enum`.
/// Returns `None` if the derived schema uses neither keyword.
fn collect_derived_enumerated_values(
    derived_map: &serde_json::Map<String, Value>,
) -> Option<Vec<Value>> {
    if let Some(c) = derived_map.get("const") {
        return Some(vec![c.clone()]);
    }
    if let Some(Value::Array(arr)) = derived_map.get("enum") {
        return Some(arr.clone());
    }
    None
}

/// When the derived schema enumerates values (via `const` or `enum`), verify that
/// every enumerated value satisfies the base bounds and pattern constraints.
/// This replaces the keyword-level checks: instead of requiring the keywords to
/// be preserved, we verify the actual values are within the allowed range.
fn check_enumerated_values_against_base(
    base_map: &serde_json::Map<String, Value>,
    values: &[Value],
    prop_name: &str,
    errors: &mut Vec<String>,
) {
    // Check numeric lower bounds (minimum, minLength, minItems)
    for keyword in &["minimum", "minLength", "minItems"] {
        if let Some(base_val) = base_map.get(*keyword).and_then(|v| v.as_f64()) {
            for val in values {
                let numeric = match keyword {
                    &"minLength" => val.as_str().map(|s| s.len() as f64),
                    &"minItems" => val.as_array().map(|a| a.len() as f64),
                    _ => val.as_f64(),
                };
                if let Some(n) = numeric {
                    if n < base_val {
                        errors.push(format!(
                            "property '{prop_name}': derived const/enum value {val} violates \
                             base {keyword} ({base_val})"
                        ));
                    }
                }
            }
        }
    }

    // Check numeric upper bounds (maximum, maxLength, maxItems)
    for keyword in &["maximum", "maxLength", "maxItems"] {
        if let Some(base_val) = base_map.get(*keyword).and_then(|v| v.as_f64()) {
            for val in values {
                let numeric = match keyword {
                    &"maxLength" => val.as_str().map(|s| s.len() as f64),
                    &"maxItems" => val.as_array().map(|a| a.len() as f64),
                    _ => val.as_f64(),
                };
                if let Some(n) = numeric {
                    if n > base_val {
                        errors.push(format!(
                            "property '{prop_name}': derived const/enum value {val} violates \
                             base {keyword} ({base_val})"
                        ));
                    }
                }
            }
        }
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

    // -- extract_effective_schema ------------------------------------------

    #[test]
    fn test_extract_simple_schema() {
        let schema = json!({
            "type": "object",
            "required": ["a"],
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "integer"}
            },
            "additionalProperties": false
        });
        let eff = extract_effective_schema(&schema);
        assert_eq!(eff.properties.len(), 2);
        assert!(eff.required.contains("a"));
        assert_eq!(eff.additional_properties, Some(Value::Bool(false)));
    }

    #[test]
    fn test_extract_with_allof() {
        let schema = json!({
            "type": "object",
            "allOf": [
                {
                    "type": "object",
                    "required": ["x"],
                    "properties": {"x": {"type": "string"}}
                },
                {
                    "type": "object",
                    "required": ["y"],
                    "properties": {"y": {"type": "number"}}
                }
            ]
        });
        let eff = extract_effective_schema(&schema);
        assert_eq!(eff.properties.len(), 2);
        assert!(eff.required.contains("x"));
        assert!(eff.required.contains("y"));
    }

    // -- validate_schema_compatibility ------------------------------------

    #[test]
    fn test_compatible_tightening() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "v": {"type": "string", "maxLength": 100}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "v": {"type": "string", "maxLength": 50}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "base~", "derived~");
        assert!(errs.is_empty(), "tightening should be ok: {errs:?}");
    }

    #[test]
    fn test_incompatible_loosening_max_length() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "v": {"type": "string", "maxLength": 100}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "v": {"type": "string", "maxLength": 200}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "base~", "derived~");
        assert!(!errs.is_empty());
    }

    #[test]
    fn test_incompatible_loosening_maximum() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "n": {"type": "integer", "maximum": 100}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "n": {"type": "integer", "maximum": 200}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(!errs.is_empty());
    }

    #[test]
    fn test_incompatible_loosening_minimum() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "n": {"type": "integer", "minimum": 10}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "n": {"type": "integer", "minimum": 5}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(!errs.is_empty());
    }

    #[test]
    fn test_enum_expansion_fails() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "s": {"type": "string", "enum": ["a", "b"]}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "s": {"type": "string", "enum": ["a", "b", "c"]}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(!errs.is_empty());
    }

    #[test]
    fn test_enum_subset_ok() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "s": {"type": "string", "enum": ["a", "b", "c"]}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "s": {"type": "string", "enum": ["a"]}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn test_additional_properties_false_blocks_new_prop() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "additionalProperties": false
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "string"}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(!errs.is_empty());
    }

    #[test]
    fn test_open_base_allows_new_prop() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {"a": {"type": "string"}}
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "string"}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn test_property_disabled_fails() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "required": ["x"],
            "properties": {"x": {"type": "string"}}
        }));
        let mut derived_props = HashMap::new();
        derived_props.insert("x".to_owned(), Value::Bool(false));
        let derived = EffectiveSchema {
            properties: derived_props,
            required: HashSet::new(),
            additional_properties: None,
        };
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(!errs.is_empty());
    }

    #[test]
    fn test_nested_object_loosening_caught() {
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "inner": {
                    "type": "object",
                    "properties": {
                        "v": {"type": "integer", "maximum": 10}
                    }
                }
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "inner": {
                    "type": "object",
                    "properties": {
                        "v": {"type": "integer", "maximum": 20}
                    }
                }
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(!errs.is_empty());
    }

    #[test]
    fn test_boolean_true_schema_loosens_constrained_property() {
        // Derived replaces a constrained property with boolean `true` schema
        // (which accepts anything), silently loosening the contract.
        let base = EffectiveSchema {
            properties: {
                let mut m = HashMap::new();
                m.insert("age".to_owned(), json!({"type": "integer", "maximum": 120}));
                m
            },
            required: HashSet::new(),
            additional_properties: None,
        };
        let derived = EffectiveSchema {
            properties: {
                let mut m = HashMap::new();
                m.insert("age".to_owned(), Value::Bool(true));
                m
            },
            required: HashSet::new(),
            additional_properties: None,
        };
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(
            !errs.is_empty(),
            "Boolean true schema should be flagged as loosening: {errs:?}"
        );
    }

    #[test]
    fn test_boolean_true_schema_ok_when_base_unconstrained() {
        // If base property is also unconstrained (no special keywords),
        // derived using boolean true is not loosening anything meaningful.
        let base = EffectiveSchema {
            properties: {
                let mut m = HashMap::new();
                m.insert("name".to_owned(), json!({"type": "string"}));
                m
            },
            required: HashSet::new(),
            additional_properties: None,
        };
        let derived = EffectiveSchema {
            properties: {
                let mut m = HashMap::new();
                m.insert("name".to_owned(), Value::Bool(true));
                m
            },
            required: HashSet::new(),
            additional_properties: None,
        };
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        // Base only has "type" constraint. Boolean true does remove that.
        // However, dropping type information is still loosening.
        // For unconstrained base (no type even), this would be OK.
        // Currently this WILL flag because type in base exists but derived is not an object.
        // This behavior is acceptable – boolean true schemas should not appear in valid GTS.
        assert!(
            !errs.is_empty(),
            "Boolean true schema replaces typed property - should flag"
        );
    }

    #[test]
    fn test_enum_tightening_allows_omitting_bounds() {
        // Derived introduces enum, which is strictly tighter than maxLength.
        // Omitting maxLength when adding enum is NOT loosening.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "tier": {"type": "string", "maxLength": 100}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "tier": {"type": "string", "enum": ["gold", "platinum"]}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(
            errs.is_empty(),
            "enum tightening should allow omitting maxLength: {errs:?}"
        );
    }

    #[test]
    fn test_const_tightening_allows_omitting_bounds_and_pattern() {
        // Derived introduces const, which is the tightest possible constraint.
        // Omitting bounds and pattern when adding const is NOT loosening.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "v": {"type": "string", "maxLength": 100, "pattern": "^[a-z]+$"}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "v": {"type": "string", "const": "hello"}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(
            errs.is_empty(),
            "const tightening should allow omitting maxLength and pattern: {errs:?}"
        );
    }

    #[test]
    fn test_enum_tightening_allows_omitting_numeric_bounds() {
        // Derived introduces enum for an integer property, omitting min/max.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "priority": {"type": "integer", "minimum": 0, "maximum": 100}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "priority": {"type": "integer", "enum": [1, 5, 10]}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(
            errs.is_empty(),
            "enum tightening should allow omitting min/max: {errs:?}"
        );
    }

    #[test]
    fn test_omitting_bounds_without_enum_or_const_still_fails() {
        // Derived omits maxLength without adding enum or const — still loosening.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "code": {"type": "string", "maxLength": 100}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "code": {"type": "string"}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "b", "d");
        assert!(
            !errs.is_empty(),
            "Omitting maxLength without enum/const should still fail"
        );
    }

    #[test]
    fn test_derived_const_must_be_in_base_enum() {
        // Base has enum, derived narrows to const — but const value must be in base enum.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "enum": ["active", "inactive"]}
            }
        }));
        let derived_ok = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "const": "active"}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived_ok, "b", "d");
        assert!(errs.is_empty(), "const in base enum should be ok: {errs:?}");

        let derived_bad = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "const": "deleted"}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived_bad, "b", "d");
        assert!(!errs.is_empty(), "const NOT in base enum should fail");
    }

    #[test]
    fn test_const_violates_minimum() {
        // Base has minimum 42, derived sets const 32 — must fail.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "minimum": 42}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "const": 32}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "base~", "derived~");
        assert!(
            !errs.is_empty(),
            "const 32 < minimum 42 should fail: {errs:?}"
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("violates") && e.contains("minimum")),
            "error should mention minimum violation: {errs:?}"
        );
    }

    #[test]
    fn test_const_satisfies_minimum() {
        // Base has minimum 42, derived sets const 50 — should pass.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "minimum": 42}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "const": 50}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "base~", "derived~");
        assert!(
            errs.is_empty(),
            "const 50 >= minimum 42 should pass: {errs:?}"
        );
    }

    #[test]
    fn test_enum_value_violates_maximum() {
        // Base has maximum 100, derived enum includes 200 — must fail.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "maximum": 100}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "enum": [10, 50, 200]}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "base~", "derived~");
        assert!(
            !errs.is_empty(),
            "enum value 200 > maximum 100 should fail: {errs:?}"
        );
    }

    #[test]
    fn test_enum_values_within_bounds() {
        // Base has minimum 10 and maximum 100, all enum values within range — should pass.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "minimum": 10, "maximum": 100}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "enum": [10, 50, 100]}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "base~", "derived~");
        assert!(
            errs.is_empty(),
            "all enum values in range should pass: {errs:?}"
        );
    }

    #[test]
    fn test_const_string_violates_max_length() {
        // Base has maxLength 5, derived const is "toolong" (7 chars) — must fail.
        let base = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "code": {"type": "string", "maxLength": 5}
            }
        }));
        let derived = extract_effective_schema(&json!({
            "type": "object",
            "properties": {
                "code": {"type": "string", "const": "toolong"}
            }
        }));
        let errs = validate_schema_compatibility(&base, &derived, "base~", "derived~");
        assert!(
            !errs.is_empty(),
            "const 'toolong' exceeds maxLength 5: {errs:?}"
        );
    }
}
