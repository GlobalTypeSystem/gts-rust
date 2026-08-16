#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use serde_json::json;
/// Asserts that some error names both the offending location and the keyword
/// the test is about.
///
/// Admission fails closed and turns every diagnostic - including
/// `NotProvable` - into an error string, so a bare "the vector is not empty"
/// passes on any undecidable result and proves nothing about the rule under
/// test.
#[track_caller]
fn assert_reports(errors: &[String], path: &str, keyword: &str) {
    assert!(
        errors
            .iter()
            .any(|error| error.contains(path) && error.contains(keyword)),
        "expected an error naming '{path}' and '{keyword}': {errors:?}"
    );
}

// -- effective schema --------------------------------------------------

/// Closedness must survive `allOf` composition: a permissive overlay may
/// not reopen a branch that closed `additionalProperties`, or a derivation
/// could smuggle properties past a closed base.
#[test]
fn test_flatten_keeps_closed_additional_properties_through_allof() {
    let schema = json!({
        "type": "object",
        "additionalProperties": true,
        "allOf": [
            {"additionalProperties": false},
            {"additionalProperties": true}
        ]
    });

    assert_eq!(
        flatten_schema(&schema).get("additionalProperties"),
        Some(&Value::Bool(false))
    );
}

/// The same, spelled through schemas that are only boolean-*equivalent*.
#[test]
fn test_flatten_keeps_boolean_equivalent_closed_additional_properties() {
    let schema = json!({
        "type": "object",
        "allOf": [
            {"additionalProperties": {"not": {}}},
            {"additionalProperties": {}}
        ]
    });

    let flattened = flatten_schema(&schema);
    assert_eq!(
        flattened
            .get("additionalProperties")
            .and_then(boolean_schema_value),
        Some(false),
        "{flattened}"
    );
}

// -- validate_derivation_compatibility ------------------------------------

#[test]
fn test_partially_open_base_accepts_compatible_derived_property() {
    let base = json!({
        "type": "object",
        "additionalProperties": {"type": "string"}
    });
    let derived = json!({
        "type": "object",
        "additionalProperties": {"type": "string"},
        "properties": {
            "foo": {"type": "string", "maxLength": 5}
        }
    });

    let errors = validate_derivation_compatibility(&base, &derived, "base", "derived");
    assert!(
        errors.is_empty(),
        "compatible refinement must be accepted: {errors:?}"
    );
}

#[test]
fn test_partially_open_base_rejects_incompatible_derived_property() {
    let base = json!({
        "type": "object",
        "additionalProperties": {"type": "string"}
    });
    let derived = json!({
        "type": "object",
        "additionalProperties": {"type": "string"},
        "properties": {
            "foo": {"type": "integer"}
        }
    });

    let errors = validate_derivation_compatibility(&base, &derived, "base", "derived");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("foo") && error.contains("changes type")),
        "incompatible refinement must be rejected: {errors:?}"
    );
}

#[test]
fn test_boolean_equivalent_additional_properties_control_derivation() {
    let open_base = json!({
        "type": "object",
        "additionalProperties": {}
    });
    let closed_base = json!({
        "type": "object",
        "additionalProperties": {"not": {}}
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "foo": {"type": "integer"}
        }
    });

    assert!(validate_derivation_compatibility(&open_base, &derived, "base", "derived").is_empty());
    let errors = validate_derivation_compatibility(&closed_base, &derived, "base", "derived");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("foo") && error.contains("closed")),
        "false-equivalent additionalProperties must close the model: {errors:?}"
    );
}

#[test]
fn test_compatible_tightening() {
    let base = json!({
        "type": "object",
        "properties": {
            "v": {"type": "string", "maxLength": 100}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "v": {"type": "string", "maxLength": 50}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert!(errs.is_empty(), "tightening should be ok: {errs:?}");
}

/// GTS pins no draft (spec sec 11), so a derivation may declare a newer dialect
/// than the base it tightens - the dialect difference alone is not an admission
/// failure.
#[test]
fn test_newer_dialect_alone_is_admitted() {
    let base = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"v": {"type": "string", "maxLength": 100}}
    });
    let derived = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {"v": {"type": "string", "maxLength": 50}}
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert!(
        errs.is_empty(),
        "a newer dialect is not a failure: {errs:?}"
    );
}

/// The exemption is scoped to the dialect diagnostic: a real violation
/// alongside a dialect change is still reported.
#[test]
fn test_newer_dialect_does_not_excuse_loosening() {
    let base = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"v": {"type": "string", "maxLength": 100}}
    });
    let derived = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {"v": {"type": "string", "maxLength": 200}}
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert_reports(&errs, "$.v", "maxLength");
    assert!(
        !errs.iter().any(|e| e.contains("dialect")),
        "the dialect change itself must stay exempt: {errs:?}"
    );
}

#[test]
fn test_incompatible_loosening_max_length() {
    let base = json!({
        "type": "object",
        "properties": {
            "v": {"type": "string", "maxLength": 100}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "v": {"type": "string", "maxLength": 200}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert_reports(&errs, "$.v", "maxLength");
}

#[test]
fn test_incompatible_loosening_maximum() {
    let base = json!({
        "type": "object",
        "properties": {
            "n": {"type": "integer", "maximum": 100}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "n": {"type": "integer", "maximum": 200}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert_reports(&errs, "$.n", "maximum");
}

#[test]
fn test_incompatible_loosening_minimum() {
    let base = json!({
        "type": "object",
        "properties": {
            "n": {"type": "integer", "minimum": 10}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "n": {"type": "integer", "minimum": 5}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert_reports(&errs, "$.n", "minimum");
}

#[test]
fn test_enum_expansion_fails() {
    let base = json!({
        "type": "object",
        "properties": {
            "s": {"type": "string", "enum": ["a", "b"]}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "s": {"type": "string", "enum": ["a", "b", "c"]}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert_reports(&errs, "$.s", "enum");
}

#[test]
fn test_enum_subset_ok() {
    let base = json!({
        "type": "object",
        "properties": {
            "s": {"type": "string", "enum": ["a", "b", "c"]}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "s": {"type": "string", "enum": ["a"]}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn test_additional_properties_false_blocks_new_prop() {
    let base = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}},
        "additionalProperties": false
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "a": {"type": "string"},
            "b": {"type": "string"}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert_reports(&errs, "'b'", "closed");
}

#[test]
fn test_value_compatibility_catches_closed_descendant_branch_orphan() {
    let base = json!({
        "type": "object",
        "properties": {
            "routing": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"}
                }
            }
        }
    });
    let derived = json!({
        "type": "object",
        "allOf": [
            base,
            {
                "type": "object",
                "properties": {
                    "routing": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "target": {"type": "string"}
                        }
                    }
                }
            }
        ]
    });

    let errs = validate_derivation_compatibility(&base, &derived, "base", "derived");
    assert!(
        errs.iter()
            .any(|e| e.contains("routing.source") && e.contains("additionalProperties")),
        "closed descendant branch should not orphan an ancestor property: {errs:?}"
    );
}

#[test]
fn test_closed_descendant_branch_fails_when_depth_guard_is_hit() {
    fn nested_object(depth: usize) -> Value {
        let mut schema = json!({"type": "object", "properties": {}});
        for _ in 0..depth {
            schema = json!({
                "type": "object",
                "properties": {
                    "child": schema
                }
            });
        }
        schema
    }

    let base = nested_object(MAX_RECURSION_DEPTH);
    let derived = nested_object(MAX_RECURSION_DEPTH);

    let errs = validate_closed_descendant_branches(&base, &derived, "base", "derived");
    assert!(
        errs.iter()
            .any(|err| err.contains("exceeded maximum nesting depth")),
        "depth guard should fail closed instead of silently accepting: {errs:?}"
    );
}

#[test]
fn test_additional_properties_inherited_via_allof_not_loosening() {
    // Derived omits `additionalProperties` at its own root but its
    // properties set is identical to base's (typical shape produced
    // by the macro emitter after the allOf+$ref refactor — the
    // derived overlay nests its new fields under base's generic
    // slot, leaving the top-level property set unchanged).
    //
    // Per JSON Schema allOf composition, the base's
    // `additionalProperties: false` is inherited via $ref, so this
    // shape is **not** loosening and OP#12 must not flag it.
    let base = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}},
        "additionalProperties": false
    });
    let derived = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}}
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(
        errs.is_empty(),
        "Derived inheriting closedness via $ref should not be flagged: {errs:?}"
    );
}

#[test]
fn test_additional_properties_explicit_true_still_loosens() {
    // A direct derived schema that has no inherited closed branch and
    // explicitly says `additionalProperties: true` loosens a closed base.
    let base = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}},
        "additionalProperties": false
    });
    let derived = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}},
        "additionalProperties": true
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(
        errs.iter()
            .any(|e| e.contains("loosens additionalProperties")),
        "Explicit additionalProperties: true must still flag as loosening: {errs:?}"
    );
}

#[test]
fn test_open_base_allows_new_prop() {
    let base = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}}
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "a": {"type": "string"},
            "b": {"type": "string"}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(errs.is_empty(), "{errs:?}");
}

/// The base declares no `required`, so switching the property off is the only
/// thing the derivation changes and the only thing that may be reported.
#[test]
fn test_property_disabled_fails() {
    let base = json!({
        "type": "object",
        "properties": {"x": {"type": "string"}}
    });
    let derived = json!({
        "type": "object",
        "properties": {"x": false}
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(
        errs.iter()
            .any(|e| e.contains("disables property defined in") && e.contains("'x'")),
        "{errs:?}"
    );
}

#[test]
fn test_nested_object_loosening_caught() {
    let base = json!({
        "type": "object",
        "properties": {
            "inner": {
                "type": "object",
                "properties": {
                    "v": {"type": "integer", "maximum": 10}
                }
            }
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "inner": {
                "type": "object",
                "properties": {
                    "v": {"type": "integer", "maximum": 20}
                }
            }
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert_reports(&errs, "$.inner.v", "maximum");
}

#[test]
fn test_boolean_true_schema_loosens_constrained_property() {
    // Derived replaces a constrained property with boolean `true` schema
    // (which accepts anything), silently loosening the contract.
    let base = json!({
        "type": "object",
        "properties": {
            "age": {"type": "integer", "maximum": 120}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "age": true
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert_reports(&errs, "$.age", "boolean schema");
}

#[test]
fn test_boolean_true_schema_loosens_typed_property() {
    // A boolean `true` derived property removes the base type constraint.
    let base = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "name": true
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert_reports(&errs, "$.name", "boolean schema");
}

#[test]
fn test_enum_tightening_allows_omitting_bounds() {
    // Derived introduces enum, which is strictly tighter than maxLength.
    // Omitting maxLength when adding enum is NOT loosening.
    let base = json!({
        "type": "object",
        "properties": {
            "tier": {"type": "string", "maxLength": 100}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "tier": {"type": "string", "enum": ["gold", "platinum"]}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(
        errs.is_empty(),
        "enum tightening should allow omitting maxLength: {errs:?}"
    );
}

#[test]
fn test_const_tightening_allows_omitting_bounds_and_pattern() {
    // Derived introduces const, which is the tightest possible constraint.
    // Omitting bounds and pattern when adding const is NOT loosening.
    let base = json!({
        "type": "object",
        "properties": {
            "v": {"type": "string", "maxLength": 100, "pattern": "^[a-z]+$"}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "v": {"type": "string", "const": "hello"}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(
        errs.is_empty(),
        "const tightening should allow omitting maxLength and pattern: {errs:?}"
    );
}

#[test]
fn test_enum_tightening_allows_omitting_numeric_bounds() {
    // Derived introduces enum for an integer property, omitting min/max.
    let base = json!({
        "type": "object",
        "properties": {
            "priority": {"type": "integer", "minimum": 0, "maximum": 100}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "priority": {"type": "integer", "enum": [1, 5, 10]}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(
        errs.is_empty(),
        "enum tightening should allow omitting min/max: {errs:?}"
    );
}

#[test]
fn test_omitting_bounds_without_enum_or_const_still_fails() {
    // Derived omits maxLength without adding enum or const — still loosening.
    let base = json!({
        "type": "object",
        "properties": {
            "code": {"type": "string", "maxLength": 100}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "code": {"type": "string"}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert_reports(&errs, "$.code", "maxLength");
}

#[test]
fn test_derived_const_must_be_in_base_enum() {
    // Base has enum, derived narrows to const — but const value must be in base enum.
    let base = json!({
        "type": "object",
        "properties": {
            "status": {"type": "string", "enum": ["active", "inactive"]}
        }
    });
    let derived_ok = json!({
        "type": "object",
        "properties": {
            "status": {"type": "string", "const": "active"}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived_ok, "b", "d");
    assert!(errs.is_empty(), "const in base enum should be ok: {errs:?}");

    let derived_bad = json!({
        "type": "object",
        "properties": {
            "status": {"type": "string", "const": "deleted"}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived_bad, "b", "d");
    assert_reports(&errs, "$.status", "enum");
}

#[test]
fn test_const_violates_minimum() {
    // Base has minimum 42, derived sets const 32 — must fail.
    let base = json!({
        "type": "object",
        "properties": {
            "score": {"type": "integer", "minimum": 42}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "score": {"type": "integer", "const": 32}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert_reports(&errs, "$.score", "minimum");
}

#[test]
fn test_const_satisfies_minimum() {
    // Base has minimum 42, derived sets const 50 — should pass.
    let base = json!({
        "type": "object",
        "properties": {
            "score": {"type": "integer", "minimum": 42}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "score": {"type": "integer", "const": 50}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert!(
        errs.is_empty(),
        "const 50 >= minimum 42 should pass: {errs:?}"
    );
}

#[test]
fn test_enum_value_violates_maximum() {
    // Base has maximum 100, derived enum includes 200 — must fail.
    let base = json!({
        "type": "object",
        "properties": {
            "score": {"type": "integer", "maximum": 100}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "score": {"type": "integer", "enum": [10, 50, 200]}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert_reports(&errs, "$.score", "maximum");
}

#[test]
fn test_enum_values_within_bounds() {
    // Base has minimum 10 and maximum 100, all enum values within range — should pass.
    let base = json!({
        "type": "object",
        "properties": {
            "score": {"type": "integer", "minimum": 10, "maximum": 100}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "score": {"type": "integer", "enum": [10, 50, 100]}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert!(
        errs.is_empty(),
        "all enum values in range should pass: {errs:?}"
    );
}

#[test]
fn test_const_string_violates_max_length() {
    // Base has maxLength 5, derived const is "toolong" (7 chars) — must fail.
    let base = json!({
        "type": "object",
        "properties": {
            "code": {"type": "string", "maxLength": 5}
        }
    });
    let derived = json!({
        "type": "object",
        "properties": {
            "code": {"type": "string", "const": "toolong"}
        }
    });
    let errs = validate_derivation_compatibility(&base, &derived, "base~", "derived~");
    assert_reports(&errs, "$.code", "maxLength");
}

/// Admission must fail closed on a document nested past the shared bound.
///
/// `declared_schema` returns such a schema unreduced and `flatten_schema`
/// stops merging its deepest branches, so the disabled-property rule alone
/// could miss a violation down there. It never gets the chance: the bounded
/// walks report the nesting first and every diagnostic becomes an error, so a
/// derivation this deep is refused rather than admitted unchecked.
#[test]
fn test_derivation_nested_beyond_the_bound_is_refused() {
    let nested_all_of = |leaf: Value| {
        let mut schema = leaf;
        for _ in 0..70 {
            schema = json!({"allOf": [schema]});
        }
        schema
    };
    let base = nested_all_of(json!({"type": "object", "properties": {"x": {"type": "string"}}}));
    // Disables a base property - the violation the shallow case reports by name.
    let derived = nested_all_of(json!({"type": "object", "properties": {"x": false}}));

    let errs = validate_derivation_compatibility(&base, &derived, "b", "d");
    assert!(
        errs.iter().any(|error| error.contains("nesting depth")),
        "the bound must be reported, not silently applied: {errs:?}"
    );

    // The same violation without the nesting is still named precisely, so the
    // test above is about the bound and not about a broken rule.
    let errs = validate_derivation_compatibility(
        &json!({"type": "object", "properties": {"x": {"type": "string"}}}),
        &json!({"type": "object", "properties": {"x": false}}),
        "b",
        "d",
    );
    assert!(
        errs.iter()
            .any(|error| error.contains("disables property defined in") && error.contains("'x'")),
        "{errs:?}"
    );
}
