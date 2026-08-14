#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use serde_json::json;
use std::collections::HashMap;

// Helper struct for compatibility results
#[derive(Debug, Default)]
#[allow(clippy::struct_field_names)]
struct CompatibilityResult {
    backward_compatibility: CompatibilityVerdict,
    forward_compatibility: CompatibilityVerdict,
    full_compatibility: CompatibilityVerdict,
}

// Helper function to check schema compatibility
fn check_schema_compatibility(
    old_schema: &serde_json::Value,
    new_schema: &serde_json::Value,
) -> CompatibilityResult {
    let (backward_compatibility, _) = check_backward_compatibility(old_schema, new_schema);
    let (forward_compatibility, _) = check_forward_compatibility(old_schema, new_schema);
    let full_compatibility =
        CompatibilityVerdict::full(backward_compatibility, forward_compatibility);

    CompatibilityResult {
        backward_compatibility,
        forward_compatibility,
        full_compatibility,
    }
}

#[test]
fn test_compatibility_verdict_serialization_and_full_derivation() {
    assert_eq!(
        serde_json::to_value(CompatibilityVerdict::Compatible).expect("serialize verdict"),
        json!("compatible")
    );
    assert_eq!(
        serde_json::to_value(CompatibilityVerdict::Incompatible).expect("serialize verdict"),
        json!("incompatible")
    );
    assert_eq!(
        serde_json::to_value(CompatibilityVerdict::Unknown).expect("serialize verdict"),
        json!("unknown")
    );
    assert_eq!(CompatibilityVerdict::Unknown.to_string(), "unknown");

    assert_eq!(
        CompatibilityVerdict::full(
            CompatibilityVerdict::Compatible,
            CompatibilityVerdict::Compatible
        ),
        CompatibilityVerdict::Compatible
    );
    assert_eq!(
        CompatibilityVerdict::full(
            CompatibilityVerdict::Compatible,
            CompatibilityVerdict::Unknown
        ),
        CompatibilityVerdict::Unknown
    );
    assert_eq!(
        CompatibilityVerdict::full(
            CompatibilityVerdict::Unknown,
            CompatibilityVerdict::Incompatible
        ),
        CompatibilityVerdict::Incompatible
    );
}

#[test]
fn test_check_schema_compatibility_identical() {
    let schema1 = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&schema1, &schema1);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_compatible());
    assert!(result.full_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_added_optional_property() {
    let old_schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        }
    });

    let new_schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    // An open model already accepted arbitrary `email` values; declaring
    // it narrows that set.
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_added_required_property() {
    let old_schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        },
        "required": ["name"]
    });

    let new_schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"}
        },
        "required": ["name", "email"]
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    // Adding required property is not backward compatible
    assert!(result.backward_compatibility.is_incompatible());
}

#[test]
fn test_check_schema_compatibility_removed_property() {
    let old_schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"}
        }
    });

    let new_schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());
}

#[test]
fn test_check_schema_compatibility_enum_expansion() {
    let old_schema = json!({
        "type": "string",
        "enum": ["active", "inactive"]
    });

    let new_schema = json!({
        "type": "string",
        "enum": ["active", "inactive", "pending"]
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());
    assert!(result.full_compatibility.is_incompatible());
}

#[test]
fn test_check_schema_compatibility_enum_reduction() {
    let old_schema = json!({
        "type": "string",
        "enum": ["active", "inactive", "pending"]
    });

    let new_schema = json!({
        "type": "string",
        "enum": ["active", "inactive"]
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
    assert!(result.full_compatibility.is_incompatible());
}

#[test]
fn test_check_schema_compatibility_type_change() {
    let old_schema = json!({
        "type": "string"
    });

    let new_schema = json!({
        "type": "number"
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_incompatible());
}

#[test]
fn test_check_schema_compatibility_constraint_tightening() {
    let old_schema = json!({
        "type": "number",
        "minimum": 0
    });

    let new_schema = json!({
        "type": "number",
        "minimum": 10
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_constraint_relaxing() {
    let old_schema = json!({
        "type": "number",
        "maximum": 100
    });

    let new_schema = json!({
        "type": "number",
        "maximum": 200
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    // Relaxing maximum is backward compatible
    assert!(result.backward_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_nested_objects() {
    let old_schema = json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                }
            }
        }
    });

    let new_schema = json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "email": {"type": "string"}
                }
            }
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_string_length_constraints() {
    let old_schema = json!({
        "type": "string",
        "minLength": 1,
        "maxLength": 100
    });

    let new_schema = json!({
        "type": "string",
        "minLength": 5,
        "maxLength": 50
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_array_length_constraints() {
    let old_schema = json!({
        "type": "array",
        "minItems": 1,
        "maxItems": 10
    });

    let new_schema = json!({
        "type": "array",
        "minItems": 2,
        "maxItems": 5
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
}

#[test]
fn test_compatibility_result_default() {
    let result = CompatibilityResult::default();
    assert!(result.backward_compatibility.is_unknown());
    assert!(result.forward_compatibility.is_unknown());
    assert!(result.full_compatibility.is_unknown());
}

#[test]
fn test_compatibility_result_fully_compatible() {
    let result = CompatibilityResult {
        backward_compatibility: CompatibilityVerdict::Compatible,
        forward_compatibility: CompatibilityVerdict::Compatible,
        full_compatibility: CompatibilityVerdict::Compatible,
    };
    assert!(result.full_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_enum_reordered() {
    let old_schema = json!({
        "type": "string",
        "enum": ["a", "b", "c"]
    });

    let new_schema = json!({
        "type": "string",
        "enum": ["c", "a", "b"]
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_compatible());
    assert!(result.full_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_nested_required_added() {
    let old_schema = json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                },
                "required": ["name"]
            }
        },
        "required": ["user"]
    });

    let new_schema = json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "email": {"type": "string"}
                },
                "required": ["name", "email"]
            }
        },
        "required": ["user"]
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    // Adding nested required is not backward compatible
    assert!(result.backward_compatibility.is_incompatible());
}

#[test]
fn test_check_schema_compatibility_allof_flatten_equivalence() {
    let direct = json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "value": {"type": "number"}
        },
        "required": ["id"]
    });

    let via_allof = json!({
        "allOf": [
            {
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            },
            {
                "type": "object",
                "properties": {"value": {"type": "number"}}
            }
        ]
    });

    // Either direction should be fully compatible
    let r1 = check_schema_compatibility(&direct, &via_allof);
    assert!(r1.backward_compatibility.is_compatible());
    assert!(r1.forward_compatibility.is_compatible());
    assert!(r1.full_compatibility.is_compatible());

    let r2 = check_schema_compatibility(&via_allof, &direct);
    assert!(r2.backward_compatibility.is_compatible());
    assert!(r2.forward_compatibility.is_compatible());
    assert!(r2.full_compatibility.is_compatible());
}

#[test]
fn test_check_schema_compatibility_removed_required() {
    let old_schema = json!({
        "type": "object",
        "properties": {"name": {"type": "string"}},
        "required": ["name"]
    });

    let new_schema = json!({
        "type": "object",
        "properties": {"name": {"type": "string"}}
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    // Removing required is forward-incompatible
    assert!(result.forward_compatibility.is_incompatible());
}

#[test]
fn test_closed_model_optional_addition_is_not_fully_compatible() {
    let old_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": {"type": "string"}
        }
    });
    let new_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());
    assert!(result.full_compatibility.is_incompatible());
}

#[test]
fn test_additional_properties_change_without_declared_properties_is_detected() {
    let old_schema = json!({"type": "object"});
    let new_schema = json!({
        "type": "object",
        "additionalProperties": false
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
    assert!(result.full_compatibility.is_incompatible());
}

#[test]
fn test_required_change_without_declared_properties_is_detected() {
    let old_schema = json!({"type": "object"});
    let new_schema = json!({
        "type": "object",
        "required": ["value"]
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
    assert!(result.full_compatibility.is_incompatible());
}

#[test]
fn test_removing_enum_constraint_is_not_fully_compatible() {
    let old_schema = json!({
        "type": "object",
        "properties": {
            "status": {"type": "string", "enum": ["active", "inactive"]}
        }
    });
    let new_schema = json!({
        "type": "object",
        "properties": {
            "status": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());
    assert!(result.full_compatibility.is_incompatible());
}

#[test]
fn test_adding_enum_constraint_is_forward_only() {
    let old_schema = json!({"type": "string"});
    let new_schema = json!({
        "type": "string",
        "enum": ["active", "inactive"]
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
    assert!(result.full_compatibility.is_incompatible());
}

#[test]
fn test_adding_and_removing_const_are_directional() {
    let added = property_change(
        json!({"type": "integer"}),
        json!({"type": "integer", "const": 1}),
    );
    assert!(added.backward_compatibility.is_incompatible());
    assert!(added.forward_compatibility.is_compatible());

    let removed = property_change(
        json!({"type": "integer", "const": 1}),
        json!({"type": "integer"}),
    );
    assert!(removed.backward_compatibility.is_compatible());
    assert!(removed.forward_compatibility.is_incompatible());
}

/// `const` and `enum` constrain the same thing, so a revision that moves
/// between the two spellings must be read as one value set.
#[test]
fn test_const_and_enum_form_one_value_set() {
    // Valid({"const": 1}) = Valid({"enum": [1]}) = {1}.
    let rewritten = property_change(json!({"const": 1}), json!({"enum": [1]}));
    assert!(rewritten.full_compatibility.is_compatible());

    let rewritten_back = property_change(json!({"enum": [1]}), json!({"const": 1}));
    assert!(rewritten_back.full_compatibility.is_compatible());

    // Widening the singleton into a larger set is backward-only.
    let widened = property_change(json!({"const": 1}), json!({"enum": [1, 2]}));
    assert!(widened.backward_compatibility.is_compatible());
    assert!(widened.forward_compatibility.is_incompatible());

    // Narrowing an enum down to one of its members is forward-only.
    let narrowed = property_change(json!({"enum": [1, 2]}), json!({"const": 1}));
    assert!(narrowed.backward_compatibility.is_incompatible());
    assert!(narrowed.forward_compatibility.is_compatible());

    // A value outside the old set is incompatible in either direction.
    let moved = property_change(json!({"const": 1}), json!({"enum": [2]}));
    assert!(moved.backward_compatibility.is_incompatible());
    assert!(moved.forward_compatibility.is_incompatible());

    // Both keywords at once accept only what satisfies both.
    let intersected = property_change(json!({"const": 1, "enum": [1, 2]}), json!({"const": 1}));
    assert!(intersected.full_compatibility.is_compatible());
}

/// JSON Schema compares values by mathematical value, so the integer and
/// float spellings of one number denote the same instance.
#[test]
fn test_value_sets_use_json_schema_equality() {
    let respelled = property_change(json!({"const": 1}), json!({"enum": [1.0]}));
    assert!(respelled.full_compatibility.is_compatible());

    // The rule applies at any depth inside a composite value.
    let nested = property_change(
        json!({"const": {"a": [1, {"b": 2}]}}),
        json!({"const": {"a": [1.0, {"b": 2.0}]}}),
    );
    assert!(nested.full_compatibility.is_compatible());

    // Narrowing still has to be seen through the respelling.
    let narrowed = property_change(json!({"enum": [1, 2]}), json!({"const": 2.0}));
    assert!(narrowed.backward_compatibility.is_incompatible());
    assert!(narrowed.forward_compatibility.is_compatible());

    // Equal mathematical value is not equal representation of anything else:
    // a different number, a different type, or a differing member count all
    // remain distinct values.
    for (old_value, new_value) in [
        (json!(1), json!(1.5)),
        (json!(1), json!("1")),
        (json!(1), json!(true)),
        (json!([1]), json!([1, 1])),
        (json!({"a": 1}), json!({"a": 1, "b": 1})),
    ] {
        let moved = property_change(json!({"const": old_value}), json!({"const": new_value}));
        assert!(
            moved.backward_compatibility.is_incompatible(),
            "{old_value} vs {new_value}"
        );
        assert!(
            moved.forward_compatibility.is_incompatible(),
            "{old_value} vs {new_value}"
        );
    }

    // The same equality decides whether a narrowing keyword changed at all.
    let respelled_multiple_of =
        property_change(json!({"multipleOf": 5}), json!({"multipleOf": 5.0}));
    assert!(respelled_multiple_of.full_compatibility.is_compatible());
}

/// Comparing a mixed integer/float pair has to be exact: rounding both sides
/// to `f64` would erase the difference between `2^53 + 1` and `2^53`.
#[test]
fn test_value_set_equality_is_exact_across_number_types() {
    // 9007199254740993 is 2^53 + 1, which no `f64` represents.
    let rounded = property_change(
        json!({"const": 9_007_199_254_740_993_i64}),
        json!({"enum": [9_007_199_254_740_992.0_f64]}),
    );
    assert!(rounded.backward_compatibility.is_incompatible());
    assert!(rounded.forward_compatibility.is_incompatible());

    // 2^53 itself is exactly representable, so its two spellings are one
    // value and the comparison must still see that.
    let exact = property_change(
        json!({"const": 9_007_199_254_740_992_i64}),
        json!({"enum": [9_007_199_254_740_992.0_f64]}),
    );
    assert!(exact.full_compatibility.is_compatible());

    // The same number kept as an integer on both sides.
    let integral = property_change(
        json!({"const": 9_007_199_254_740_993_i64}),
        json!({"enum": [9_007_199_254_740_993_i64]}),
    );
    assert!(integral.full_compatibility.is_compatible());

    // A `u64` above `i64::MAX` and a negative number share no
    // representation to be compared through, and are not equal.
    let mixed_signedness = property_change(
        json!({"const": 18_446_744_073_709_551_615_u64}),
        json!({"const": -1_i64}),
    );
    assert!(mixed_signedness.backward_compatibility.is_incompatible());
    assert!(mixed_signedness.forward_compatibility.is_incompatible());
}

#[test]
fn test_boolean_schemas_follow_set_inclusion() {
    let narrowed = check_schema_compatibility(&json!(true), &json!(false));
    assert!(narrowed.backward_compatibility.is_incompatible());
    assert!(narrowed.forward_compatibility.is_compatible());

    let widened = check_schema_compatibility(&json!(false), &json!(true));
    assert!(widened.backward_compatibility.is_compatible());
    assert!(widened.forward_compatibility.is_incompatible());

    // Object spellings of the boolean schemas have identical semantics.
    let equivalent = check_schema_compatibility(&json!(true), &json!({}));
    assert!(equivalent.full_compatibility.is_compatible());
}

#[test]
fn test_closed_model_optional_removal_is_forward_only() {
    let old_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"}
        }
    });
    let new_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
    assert!(result.full_compatibility.is_incompatible());
}

#[test]
fn test_unevaluated_properties_closes_2020_12_object() {
    let old_schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "unevaluatedProperties": false,
        "properties": {"name": {"type": "string"}}
    });
    let new_schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "unevaluatedProperties": false,
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());
}

#[test]
fn test_unevaluated_properties_is_ignored_by_draft_07() {
    let old_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "unevaluatedProperties": false,
        "properties": {"name": {"type": "string"}}
    });
    let new_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "unevaluatedProperties": false,
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
}

/// A partially open level still constrains the names it does not declare,
/// so a property added there is decidable against `additionalProperties`
/// rather than merely undecided.
#[test]
fn test_property_added_to_partial_level_is_checked_against_additional_properties() {
    let old_schema = json!({
        "type": "object",
        "properties": {
            "details": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            }
        }
    });
    let new_schema = json!({
        "type": "object",
        "properties": {
            "details": {
                "type": "object",
                "additionalProperties": {"type": "string"},
                "properties": {"count": {"type": "integer"}}
            }
        }
    });

    let (backward, backward_errors) = check_backward_compatibility(&old_schema, &new_schema);
    let (forward, forward_errors) = check_forward_compatibility(&old_schema, &new_schema);
    assert_eq!(
        backward,
        CompatibilityVerdict::Incompatible,
        "{backward_errors:?}"
    );
    assert_eq!(
        forward,
        CompatibilityVerdict::Incompatible,
        "{forward_errors:?}"
    );
    assert!(
        backward_errors
            .iter()
            .any(|error| error.contains("$.details.count")),
        "{backward_errors:?}"
    );
    assert!(
        forward_errors
            .iter()
            .any(|error| error.contains("$.details.count")),
        "{forward_errors:?}"
    );
}

#[test]
fn test_dialect_change_is_not_proven_compatible() {
    let old_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "string"
    });
    let new_schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "string"
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_unknown());
    assert!(result.forward_compatibility.is_unknown());
}

#[test]
fn test_all_of_inherited_closure_controls_property_addition() {
    let old_schema = json!({
        "allOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "properties": {"name": {"type": "string"}}
            }
        ]
    });
    let new_schema = json!({
        "allOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "name": {"type": "string"},
                    "email": {"type": "string"}
                }
            }
        ]
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());
}

#[test]
fn test_all_of_intersects_duplicate_property_schemas() {
    let schema = json!({
        "allOf": [
            {
                "type": "object",
                "properties": {
                    "value": {"type": "string", "minLength": 1}
                }
            },
            {
                "type": "object",
                "properties": {
                    "value": {"type": "string", "maxLength": 10}
                }
            }
        ]
    });

    let flattened = flatten_schema(&schema);
    assert_eq!(
        flattened.pointer("/properties/value/minLength"),
        Some(&json!(1))
    );
    assert_eq!(
        flattened.pointer("/properties/value/maxLength"),
        Some(&json!(10))
    );
}

#[test]
fn test_definitions_container_change_alone_is_fully_compatible() {
    // `definitions` is reachable only through `$ref` and never contributes
    // to Valid(S), so adding an entry nothing references changes nothing.
    let old_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "definitions": {
            "Used": {"type": "object", "additionalProperties": false}
        },
        "properties": {"u": {"type": "object", "additionalProperties": false}}
    });
    let new_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "definitions": {
            "Used": {"type": "object", "additionalProperties": false},
            "NeverReferenced": {"type": "string"}
        },
        "properties": {"u": {"type": "object", "additionalProperties": false}}
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.full_compatibility.is_compatible());
}

#[test]
fn test_resolved_nested_definition_addition_is_backward_only() {
    // The shape `resolve_schema_refs` produces for a macro-generated
    // document: the referenced level is inlined and closed, and the
    // residual `definitions` container must not double-count the change.
    let level = |extra: bool| {
        let mut props = json!({"label": {"type": "string"}});
        if extra {
            props["note"] = json!({"type": "string"});
        }
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": props,
            "required": ["label"]
        })
    };
    let document = |extra: bool| {
        json!({
            "type": "object",
            "additionalProperties": false,
            "definitions": {"Nested": level(extra)},
            "properties": {"nested": level(extra)},
            "required": ["nested"]
        })
    };

    let result = check_schema_compatibility(&document(false), &document(true));
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());
}

#[test]
fn test_differing_unresolved_ref_is_reported_as_unresolved() {
    let old_schema = json!({
        "type": "object",
        "properties": {"target": {"$ref": "gts://gts.x.core.a.b.v1~"}}
    });
    let new_schema = json!({
        "type": "object",
        "properties": {"target": {"$ref": "gts://gts.x.core.a.b.v2~"}}
    });

    let (is_backward, backward_errors) = check_backward_compatibility(&old_schema, &new_schema);
    assert!(is_backward.is_unknown());
    assert!(
        backward_errors
            .iter()
            .any(|error| error.contains("$.target") && error.contains("unresolved '$ref'")),
        "{backward_errors:?}"
    );
}

#[test]
fn test_identical_unresolved_ref_needs_no_resolution() {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {"target": {"$ref": "gts://gts.x.core.a.b.v1~"}}
    });

    let result = check_schema_compatibility(&schema, &schema);
    assert!(result.full_compatibility.is_compatible());
}

fn property_change(old_property: Value, new_property: Value) -> CompatibilityResult {
    let document = |property: Value| {
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"value": property},
            "required": ["value"]
        })
    };
    check_schema_compatibility(&document(old_property), &document(new_property))
}

/// Bound keywords must be compared whenever present, never gated on `type`.
/// Gating on the old schema's `type` reported a real narrowing as fully
/// compatible whenever `type` was absent or written as an array.
#[test]
fn test_numeric_bounds_are_checked_without_a_type_keyword() {
    let result = property_change(json!({"minimum": 0}), json!({"minimum": 5}));
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());

    let result = property_change(
        json!({"type": ["integer"], "minimum": 0}),
        json!({"type": ["integer"], "minimum": 5}),
    );
    assert!(result.backward_compatibility.is_incompatible());
    assert!(result.forward_compatibility.is_compatible());
}

#[test]
fn test_exclusive_and_size_bounds_are_directional() {
    for (min_key, max_key) in [
        ("exclusiveMinimum", "exclusiveMaximum"),
        ("minProperties", "maxProperties"),
    ] {
        let relaxed = property_change(json!({max_key: 10}), json!({max_key: 100}));
        assert!(
            relaxed.backward_compatibility.is_compatible(),
            "relaxing {max_key}"
        );
        assert!(
            relaxed.forward_compatibility.is_incompatible(),
            "relaxing {max_key}"
        );

        let tightened = property_change(json!({min_key: 1}), json!({min_key: 5}));
        assert!(
            tightened.backward_compatibility.is_incompatible(),
            "tightening {min_key}"
        );
        assert!(
            tightened.forward_compatibility.is_compatible(),
            "tightening {min_key}"
        );
    }
}

#[test]
fn test_inclusive_and_exclusive_bounds_are_compared_together() {
    let lower = property_change(
        json!({"type": "number", "minimum": 0}),
        json!({"type": "number", "exclusiveMinimum": 0}),
    );
    assert!(lower.backward_compatibility.is_incompatible());
    assert!(lower.forward_compatibility.is_compatible());

    let upper = property_change(
        json!({"type": "number", "maximum": 10}),
        json!({"type": "number", "exclusiveMaximum": 10}),
    );
    assert!(upper.backward_compatibility.is_incompatible());
    assert!(upper.forward_compatibility.is_compatible());
}

/// `-0.0` and `0.0` denote the same JSON number, so respelling a bound
/// changes no accepted instance.
#[test]
fn test_signed_zero_bounds_are_equal() {
    let lower = property_change(
        json!({"type": "number", "minimum": -0.0}),
        json!({"type": "number", "minimum": 0.0}),
    );
    assert!(lower.full_compatibility.is_compatible());

    let upper = property_change(
        json!({"type": "number", "maximum": -0.0}),
        json!({"type": "number", "maximum": 0.0}),
    );
    assert!(upper.full_compatibility.is_compatible());
}

/// Draft-04 spells `exclusiveMinimum` as a boolean modifier, which a numeric
/// comparison would silently ignore.
#[test]
fn test_boolean_exclusive_minimum_is_not_silently_ignored() {
    let result = property_change(
        json!({"type": "integer", "minimum": 1, "exclusiveMinimum": false}),
        json!({"type": "integer", "minimum": 1, "exclusiveMinimum": true}),
    );
    assert!(result.backward_compatibility.is_unknown());
    assert!(result.forward_compatibility.is_unknown());
}

#[test]
fn test_type_is_compared_as_a_set() {
    // Dropping `null` from an `Option<T>` union narrows the accepted set.
    let narrowed = property_change(
        json!({"type": ["string", "null"]}),
        json!({"type": "string"}),
    );
    assert!(narrowed.backward_compatibility.is_incompatible());
    assert!(narrowed.forward_compatibility.is_compatible());

    // Member order carries no meaning.
    let reordered = property_change(
        json!({"type": ["string", "null"]}),
        json!({"type": ["null", "string"]}),
    );
    assert!(reordered.full_compatibility.is_compatible());

    // Widening a union accepts everything the old union did.
    let widened = property_change(
        json!({"type": "string"}),
        json!({"type": ["string", "null"]}),
    );
    assert!(widened.backward_compatibility.is_compatible());
    assert!(widened.forward_compatibility.is_incompatible());

    // `integer` remains a subset of `number` inside a union.
    let promoted = property_change(
        json!({"type": ["integer", "null"]}),
        json!({"type": ["number", "null"]}),
    );
    assert!(promoted.backward_compatibility.is_compatible());
    assert!(promoted.forward_compatibility.is_incompatible());
}

#[test]
fn test_enum_and_const_imply_effective_types() {
    let enum_narrowed = property_change(json!({"type": "string"}), json!({"enum": ["a"]}));
    assert!(enum_narrowed.backward_compatibility.is_incompatible());
    assert!(enum_narrowed.forward_compatibility.is_compatible());

    let const_narrowed = property_change(json!({"type": "string"}), json!({"const": "a"}));
    assert!(const_narrowed.backward_compatibility.is_incompatible());
    assert!(const_narrowed.forward_compatibility.is_compatible());

    // JSON Schema treats mathematically integral JSON numbers as integers,
    // regardless of whether the source text contains a decimal point.
    let integral_number = property_change(json!({"type": "integer"}), json!({"const": 1.0}));
    assert!(integral_number.backward_compatibility.is_incompatible());
    assert!(integral_number.forward_compatibility.is_compatible());

    // A tiny nonzero fraction is not an integer, however close to one it
    // lands: `{"const": 1e-20}` is the sole value the new schema accepts and
    // `{"type": "integer"}` rejects it.
    let tiny_fraction = property_change(json!({"type": "integer"}), json!({"const": 1e-20}));
    assert!(tiny_fraction.backward_compatibility.is_incompatible());
    assert!(tiny_fraction.forward_compatibility.is_incompatible());
}

#[test]
fn test_narrowing_keyword_presence_is_directional() {
    for keyword in ["pattern", "format", "multipleOf"] {
        let value = if keyword == "multipleOf" {
            json!(5)
        } else if keyword == "format" {
            json!("date-time")
        } else {
            json!("^a+$")
        };

        let added = property_change(json!({}), json!({keyword: value.clone()}));
        assert!(
            added.backward_compatibility.is_incompatible(),
            "adding {keyword}"
        );
        assert!(
            added.forward_compatibility.is_compatible(),
            "adding {keyword}"
        );

        let removed = property_change(json!({keyword: value}), json!({}));
        assert!(
            removed.backward_compatibility.is_compatible(),
            "removing {keyword}"
        );
        assert!(
            removed.forward_compatibility.is_incompatible(),
            "removing {keyword}"
        );
    }
}

/// Two different regexes cannot be ordered by inclusion, so neither
/// direction is provable - and the diagnostic must say so rather than imply
/// the change is breaking.
#[test]
fn test_changed_pattern_is_reported_as_unprovable() {
    let (_, errors) = check_backward_compatibility(
        &json!({"type": "string", "pattern": "^a+$"}),
        &json!({"type": "string", "pattern": "^[ab]+$"}),
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("cannot be proven")),
        "{errors:?}"
    );
}

#[test]
fn test_unique_items_defaults_to_false() {
    let enabled = property_change(
        json!({"type": "array"}),
        json!({"type": "array", "uniqueItems": true}),
    );
    assert!(enabled.backward_compatibility.is_incompatible());
    assert!(enabled.forward_compatibility.is_compatible());

    let disabled = property_change(
        json!({"type": "array", "uniqueItems": true}),
        json!({"type": "array", "uniqueItems": false}),
    );
    assert!(disabled.backward_compatibility.is_compatible());
    assert!(disabled.forward_compatibility.is_incompatible());

    // Spelling out the default changes no accepted instance.
    let no_op = property_change(
        json!({"type": "array", "uniqueItems": false}),
        json!({"type": "array"}),
    );
    assert!(no_op.full_compatibility.is_compatible());
}

/// An omitted `$schema` means "the dialect the implementation applies", so
/// starting to declare a dialect that was already in effect is not a change.
#[test]
fn test_declaring_a_previously_omitted_dialect_is_compatible() {
    let result = check_schema_compatibility(
        &json!({"type": "object", "additionalProperties": false}),
        &json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "additionalProperties": false
        }),
    );
    assert!(result.full_compatibility.is_compatible());
}

/// The `unevaluatedProperties` decision must follow the dialect that is in
/// effect, including when only one definition spells it out.
#[test]
fn test_omitted_dialect_inherits_unevaluated_support() {
    let result = check_schema_compatibility(
        &json!({
            "type": "object",
            "unevaluatedProperties": false,
            "properties": {"name": {"type": "string"}}
        }),
        &json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "unevaluatedProperties": false,
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        }),
    );
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());
}

/// With no `$schema` anywhere the dialect is the one this implementation
/// applies when validating instances, which is Draft 2020-12 - so
/// `unevaluatedProperties` closes the level here too.
#[test]
fn test_undeclared_dialect_evaluates_unevaluated_properties() {
    let old_schema = json!({
        "type": "object",
        "unevaluatedProperties": false,
        "properties": {"name": {"type": "string"}}
    });
    let new_schema = json!({
        "type": "object",
        "unevaluatedProperties": false,
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"}
        }
    });

    let result = check_schema_compatibility(&old_schema, &new_schema);
    assert!(result.backward_compatibility.is_compatible());
    assert!(result.forward_compatibility.is_incompatible());

    // The instance validator this crate builds must agree with the verdict.
    let validator = jsonschema::validator_for(&old_schema).expect("compile schema");
    assert!(!validator.is_valid(&json!({"name": "n", "email": "e"})));

    // The same dialect decides the reported content model of a level.
    let levels = classify_object_levels(&old_schema);
    assert_eq!(
        levels.first().map(|level| level.content_model),
        Some(ContentModel::Closed)
    );
}

#[test]
fn test_boolean_equivalent_property_schemas_classify_semantically() {
    let additional_open = json!({
        "type": "object",
        "additionalProperties": {}
    });
    let additional_closed = json!({
        "type": "object",
        "additionalProperties": {"not": {}}
    });
    let property_names_open = json!({
        "type": "object",
        "propertyNames": {}
    });
    let property_names_closed = json!({
        "type": "object",
        "propertyNames": {"not": {}}
    });
    let closed_fallback_with_name_constraint = json!({
        "type": "object",
        "additionalProperties": {"not": {}},
        "propertyNames": {"type": "string"}
    });
    let closed_names_with_pattern = json!({
        "type": "object",
        "propertyNames": {"not": {}},
        "patternProperties": {".*": {}}
    });
    let open_pattern = json!({
        "type": "object",
        "patternProperties": {"^x-": {}}
    });
    let closed_pattern = json!({
        "type": "object",
        "additionalProperties": {"not": {}},
        "patternProperties": {"^x-": {"not": {}}}
    });
    let explicit_open_additional_precedes_unevaluated = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": {},
        "unevaluatedProperties": {"not": {}}
    });

    for (schema, expected) in [
        (additional_open, ContentModel::Open),
        (additional_closed, ContentModel::Closed),
        (property_names_open, ContentModel::Open),
        (property_names_closed, ContentModel::Closed),
        (closed_fallback_with_name_constraint, ContentModel::Closed),
        (closed_names_with_pattern, ContentModel::Closed),
        (open_pattern, ContentModel::Open),
        (closed_pattern, ContentModel::Closed),
        (
            explicit_open_additional_precedes_unevaluated,
            ContentModel::Open,
        ),
    ] {
        let levels = classify_object_levels(&schema);
        assert_eq!(
            levels.first().map(|level| level.content_model),
            Some(expected)
        );
    }
}

#[test]
fn test_boolean_equivalent_additional_properties_drive_compatibility() {
    let added_property = |additional_properties: Value| {
        check_schema_compatibility(
            &json!({
                "type": "object",
                "additionalProperties": additional_properties
            }),
            &json!({
                "type": "object",
                "additionalProperties": additional_properties,
                "properties": {"name": {"type": "string"}}
            }),
        )
    };

    let open = added_property(json!({}));
    assert!(open.backward_compatibility.is_incompatible());
    assert!(open.forward_compatibility.is_compatible());

    let closed = added_property(json!({"not": {}}));
    assert!(closed.backward_compatibility.is_compatible());
    assert!(closed.forward_compatibility.is_incompatible());
}

/// §4.4 requires the content model to be read per object level from the
/// resolved effective schema, and §4.4.1's closed-envelope shape puts the
/// level that decides evolvability inside an extension container rather
/// than at the document root.
#[test]
fn test_classify_object_levels_reports_every_level() {
    let schema = json!({
        "$schema": "http://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "envelope_field": {"type": "string"},
            "payload": {
                "type": "object",
                "properties": {
                    "own": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {"a": {"type": "string"}}
                    }
                }
            },
            "labels": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            },
            "closed_by_unevaluated": {
                "type": "object",
                "unevaluatedProperties": false,
                "properties": {"b": {"type": "string"}}
            },
            "rows": {
                "type": "array",
                "items": {"type": "object", "properties": {"c": {"type": "string"}}}
            }
        }
    });

    let levels: HashMap<String, ContentModel> = classify_object_levels(&schema)
        .into_iter()
        .map(|level| (level.path, level.content_model))
        .collect();

    assert_eq!(levels.get("$"), Some(&ContentModel::Closed));
    assert_eq!(levels.get("$.payload"), Some(&ContentModel::Open));
    assert_eq!(levels.get("$.payload.own"), Some(&ContentModel::Closed));
    assert_eq!(levels.get("$.labels"), Some(&ContentModel::Partial));
    assert_eq!(
        levels.get("$.closed_by_unevaluated"),
        Some(&ContentModel::Closed)
    );
    assert_eq!(levels.get("$.rows[]"), Some(&ContentModel::Open));
    // A scalar property is not an object level.
    assert!(!levels.contains_key("$.envelope_field"));

    // Evolvability is exactly closure.
    assert!(ContentModel::Closed.is_evolvable_in_place());
    assert!(!ContentModel::Open.is_evolvable_in_place());
    assert!(!ContentModel::Partial.is_evolvable_in_place());
}

/// A level closed only through `allOf` composition must classify as closed,
/// not as the open level it looks like in isolation.
#[test]
fn test_classify_object_levels_uses_the_effective_schema() {
    let schema = json!({
        "allOf": [
            {"type": "object", "additionalProperties": false},
            {"type": "object", "properties": {"a": {"type": "string"}}}
        ]
    });

    let levels = classify_object_levels(&schema);
    assert_eq!(
        levels.first().map(|level| level.content_model),
        Some(ContentModel::Closed)
    );
}

#[test]
fn test_diagnostics_carry_the_schema_location_and_kind() {
    let old_schema = json!({
        "type": "object",
        "properties": {
            "payload": {"type": "object", "properties": {"a": {"type": "string"}}}
        }
    });
    let new_schema = json!({
        "type": "object",
        "properties": {
            "payload": {
                "type": "object",
                "properties": {"a": {"type": "string"}, "b": {"type": "string"}}
            }
        }
    });

    let (compatible, diagnostics) = check_backward_diagnostics(&old_schema, &new_schema);
    assert!(compatible.is_incompatible());
    let finding = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.path == "$.payload")
        .expect("the offending level must be named, not the document root");
    assert_eq!(finding.finding, CompatibilityFinding::PropertyAdded);
    assert_eq!(
        finding.to_string(),
        "Schema at '$.payload' adds property 'b' in a open model"
    );
}

/// A caller that fails closed treats both alike, but an owner needs to tell
/// "we cannot decide this" from "this is known to break".
#[test]
fn test_undecidable_changes_are_reported_as_not_provable() {
    let (_, diagnostics) = check_backward_diagnostics(
        &json!({"type": "string", "pattern": "^a+$"}),
        &json!({"type": "string", "pattern": "^[ab]+$"}),
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.finding == CompatibilityFinding::NotProvable),
        "{diagnostics:?}"
    );
}

/// An unprovable `allOf` intersection is the checker's own bookkeeping and
/// must never surface as a keyword: `flatten_schema` is public and its
/// output feeds instance casting and `additionalProperties` comparisons,
/// where a synthetic keyword reads as a real constraint difference.
#[test]
fn test_unprovable_intersection_leaves_no_synthetic_keyword() {
    let flattened = flatten_schema(&json!({
        "allOf": [
            {"type": "object", "additionalProperties": {"type": "string"}},
            {"type": "object", "additionalProperties": {"type": "number"}}
        ]
    }));

    let keys: Vec<&String> = flattened
        .as_object()
        .expect("flattening object branches yields an object")
        .keys()
        .collect();
    assert!(
        keys.iter().all(|key| !key.starts_with("x-gts-internal")),
        "{keys:?}"
    );
}

/// The undecidable branch must stay local: reporting it must not swallow a
/// sibling that is decidably broken, or "unknown" would mask "incompatible".
#[test]
fn test_unprovable_property_does_not_mask_sibling_incompatibility() {
    let schema_with = |sibling: Value| {
        json!({
            "type": "object",
            "allOf": [
                {"properties": {"undecidable": {"type": "string"}}},
                {"properties": {"undecidable": {"type": "integer"}}}
            ],
            "properties": {"sibling": sibling}
        })
    };

    let (verdict, diagnostics) = check_backward_diagnostics(
        &schema_with(json!({"type": "string"})),
        &schema_with(json!({"type": "number"})),
    );

    assert!(verdict.is_incompatible(), "{diagnostics:?}");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == "$.undecidable"
                && diagnostic.finding == CompatibilityFinding::NotProvable),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == "$.sibling"),
        "{diagnostics:?}"
    );
}
