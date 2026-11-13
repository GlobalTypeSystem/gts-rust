#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;

const TYPE_ID: &str = "gts.x.t.base.v1~x.t.child.v1~";

fn entry(path: Vec<&'static str>, helper: Value) -> AncestorOverride {
    AncestorOverride {
        ancestor_type_id: "gts.x.t.base.v1~",
        path,
        generic_field: Some("payload"),
        helper,
    }
}

#[allow(clippy::needless_pass_by_value)]
fn helper(properties: Value) -> Value {
    json!({ "type": "object", "properties": properties })
}

/// Parent with one root field `f` declared as `field`.
#[allow(clippy::needless_pass_by_value)]
fn parent_with(field: Value) -> Value {
    json!({
        "$id": "gts://gts.x.t.base.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": { "f": field, "payload": { "type": "object" } }
    })
}

fn apply_helper(parent: &Value, helper: Value) -> Result<Value, OverrideError> {
    let mut overlay = json!({ "type": "object", "properties": {} });
    let overrides = [entry(vec![], helper)];
    validate_ancestor_overrides(parent, TYPE_ID, &overrides)?;
    apply_ancestor_overrides(&mut overlay, &overrides);
    Ok(overlay)
}

fn apply(parent: &Value, helper_properties: Value) -> Result<Value, OverrideError> {
    Ok(apply_helper(parent, helper(helper_properties))?["properties"]["f"].clone())
}

fn rejected(parent: &Value, helper_properties: Value) -> String {
    apply(parent, helper_properties).unwrap_err().to_string()
}

/// Overrides `f: inherited` with `f: replacement` and, separately, admits a
/// derived document declaring `f: replacement` under a base declaring
/// `f: inherited` through ordinary derivation. Both must agree: they are one
/// question for one engine.
fn paired(inherited: &Value, replacement: &Value) -> bool {
    let parent = parent_with(inherited.clone());
    let overridden = apply(&parent, json!({ "f": replacement }));
    if let Ok(emitted) = &overridden {
        assert_eq!(emitted, replacement, "the replacement is emitted as given");
    }
    let derived = json!({
        "$id": format!("gts://{TYPE_ID}"),
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": { "f": replacement, "payload": { "type": "object" } }
    });
    let derivation = crate::schema_derivation::validate_derivation(
        &parent,
        &derived,
        "gts.x.t.base.v1~",
        TYPE_ID,
    );
    assert_eq!(
        overridden.is_ok(),
        derivation.is_empty(),
        "override {overridden:?} vs derivation {derivation:?} for {inherited} -> {replacement}"
    );
    overridden.is_ok()
}

// --- resolve_type_schema ----------------------------------------------------

#[test]
fn resolve_replaces_parent_ref_and_keeps_all_of_branches() {
    let own = json!({
        "$id": "gts://gts.x.t.base.v1~x.t.child.v1~",
        "allOf": [
            { "$ref": "gts://gts.x.t.base.v1~" },
            { "properties": { "f": { "allOf": [{ "pattern": "^a" }] } } }
        ]
    });
    let parent =
        json!({ "$id": "gts://gts.x.t.base.v1~", "properties": { "f": { "type": "string" } } });
    let resolved = resolve_type_schema(&own, Some(parent));
    assert_eq!(
        resolved["allOf"][0],
        json!({ "properties": { "f": { "type": "string" } } })
    );
    assert_eq!(
        resolved["allOf"][1]["properties"]["f"]["allOf"][0]["pattern"],
        "^a"
    );
}

#[test]
fn resolve_never_reads_literal_data_as_a_schema() {
    let own = json!({
        "definitions": { "X": { "type": "string" } },
        "properties": {
            "c": { "const": { "$ref": "#/definitions/X" } },
            "e": { "enum": [{ "$ref": "#/definitions/X" }] },
            "d": { "default": { "$ref": "#/definitions/X" }, "examples": [{ "$ref": "#/definitions/X" }] },
            "r": { "$ref": "#/definitions/X" }
        }
    });
    let resolved = resolve_type_schema(&own, None);
    let properties = &resolved["properties"];
    assert_eq!(
        properties["c"]["const"],
        json!({ "$ref": "#/definitions/X" })
    );
    assert_eq!(
        properties["e"]["enum"][0],
        json!({ "$ref": "#/definitions/X" })
    );
    assert_eq!(
        properties["d"]["default"],
        json!({ "$ref": "#/definitions/X" })
    );
    assert_eq!(
        properties["d"]["examples"][0],
        json!({ "$ref": "#/definitions/X" })
    );
    assert_eq!(properties["r"], json!({ "type": "string" }));
}

#[test]
fn resolve_keeps_a_boolean_target_boolean() {
    let own = json!({
        "definitions": { "Never": false },
        "properties": { "n": { "$ref": "#/definitions/Never" } }
    });
    assert_eq!(
        resolve_type_schema(&own, None)["properties"]["n"],
        json!(false)
    );
}

#[test]
fn resolve_follows_escaped_and_nested_pointers() {
    let own = json!({
        "definitions": {
            "a/b~c": { "type": "integer" },
            "Outer": { "properties": { "inner": { "type": "boolean" } } }
        },
        "properties": {
            "escaped": { "$ref": "#/definitions/a~1b~0c" },
            "nested": { "$ref": "#/definitions/Outer/properties/inner" }
        }
    });
    let resolved = resolve_type_schema(&own, None);
    assert_eq!(
        resolved["properties"]["escaped"],
        json!({ "type": "integer" })
    );
    assert_eq!(
        resolved["properties"]["nested"],
        json!({ "type": "boolean" })
    );
}

#[test]
fn resolve_leaves_refs_with_assertion_siblings_and_recursion_in_place() {
    let own = json!({
        "definitions": {
            "S": { "type": "string" },
            "Node": { "type": "object", "properties": { "next": { "$ref": "#/definitions/Node" } } }
        },
        "properties": {
            "sibling": { "$ref": "#/definitions/S", "maxLength": 3 },
            "annotated": { "$ref": "#/definitions/S", "description": "kept bare" },
            "node": { "$ref": "#/definitions/Node" }
        }
    });
    let resolved = resolve_type_schema(&own, None);
    let properties = &resolved["properties"];
    assert_eq!(properties["sibling"]["$ref"], "#/definitions/S");
    assert_eq!(properties["annotated"], json!({ "type": "string" }));
    assert_eq!(
        properties["node"]["properties"]["next"]["$ref"],
        "#/definitions/Node"
    );
}

#[test]
fn resolve_leaves_a_root_self_reference_in_place() {
    // Schemars emits `Vec<Self>` items as `{"$ref": "#"}`.
    let own =
        json!({ "properties": { "children": { "type": "array", "items": { "$ref": "#" } } } });
    let resolved = resolve_type_schema(&own, None);
    assert_eq!(
        resolved["properties"]["children"]["items"],
        json!({ "$ref": "#" })
    );
}

#[test]
fn overriding_a_self_recursive_field_is_rejected() {
    let parent = parent_with(json!({ "type": "array", "items": { "$ref": "#" } }));
    let parent = resolve_type_schema(&parent, None);
    let error = rejected(
        &parent,
        json!({ "f": { "type": "array", "items": { "type": "number" } } }),
    );
    assert!(error.contains("recursive or unresolved `$ref`"), "{error}");
}

#[test]
fn overriding_a_field_that_reaches_a_leftover_ref_is_rejected() {
    let parent = parent_with(
        json!({ "type": "object", "properties": { "next": { "$ref": "#/definitions/Node" } } }),
    );
    let error = rejected(&parent, json!({ "f": { "type": "object" } }));
    assert!(error.contains("recursive or unresolved `$ref`"), "{error}");
}

// --- helper shape -----------------------------------------------------------

#[test]
fn helper_root_must_be_a_plain_object_schema() {
    let parent = parent_with(json!({ "type": "string" }));
    for (helper, needle) in [
        (
            json!({ "type": "string", "enum": ["a"] }),
            "struct with named fields",
        ),
        (
            json!({ "type": "object", "properties": { "f": {} }, "anyOf": [] }),
            "root keyword 'anyOf'",
        ),
        (
            json!({ "type": "object", "properties": { "f": {} }, "definitions": {} }),
            "recursive",
        ),
        (
            json!({ "type": "object", "properties": { "f": {} }, "additionalProperties": { "type": "string" } }),
            "additionalProperties",
        ),
        (
            json!({ "type": "object", "properties": {} }),
            "declares no fields",
        ),
    ] {
        let error = apply_helper(&parent, helper).unwrap_err().to_string();
        assert!(error.contains(needle), "{needle}: {error}");
    }
}

#[test]
fn helper_root_closedness_is_not_carried_over_but_nested_closedness_is() {
    let parent =
        parent_with(json!({ "type": "object", "properties": { "a": { "type": "string" } } }));
    let field = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": { "a": { "type": "string", "maxLength": 1 } }
    });
    let overlay = apply_helper(
        &parent,
        json!({ "type": "object", "additionalProperties": false, "properties": { "f": field } }),
    )
    .unwrap();
    assert!(overlay.get("additionalProperties").is_none(), "{overlay}");
    assert_eq!(overlay["properties"]["f"], field);
}

#[test]
fn annotations_are_not_part_of_the_replacement() {
    let parent = parent_with(json!({ "type": "string" }));
    let merged = apply(
        &parent,
        json!({ "f": { "type": "string", "description": "doc", "default": "" } }),
    )
    .unwrap();
    assert_eq!(merged, json!({ "type": "string" }));
}

#[test]
fn undeclared_fields_and_the_generic_slot_are_rejected() {
    let parent = parent_with(json!({ "type": "string" }));
    let error = apply_helper(&parent, helper(json!({ "g": { "type": "string" } })))
        .unwrap_err()
        .to_string();
    assert!(error.contains("field '/g': is not declared"), "{error}");
    let error = apply_helper(&parent, helper(json!({ "payload": { "type": "object" } })))
        .unwrap_err()
        .to_string();
    assert!(error.contains("generic slot"), "{error}");
}

// --- inclusion ----------------------------------------------------------------

#[test]
fn equal_and_narrower_replacements_are_admitted() {
    let inherited = json!({ "type": "string", "minLength": 1, "maxLength": 10, "pattern": "^a" });
    assert!(paired(&inherited, &inherited));
    assert!(paired(
        &inherited,
        &json!({ "type": "string", "minLength": 2, "maxLength": 5, "pattern": "^a", "format": "uuid" })
    ));
    assert!(paired(
        &json!({ "type": "number", "minimum": 0 }),
        &json!({ "type": "integer", "minimum": 1 })
    ));
}

#[test]
fn dropped_bounds_are_not_retained_but_rejected() {
    let inherited = json!({ "type": "string", "maxLength": 10, "pattern": "^a" });
    assert!(!paired(
        &inherited,
        &json!({ "type": "string", "pattern": "^a" })
    ));
    assert!(!paired(
        &inherited,
        &json!({ "type": "string", "maxLength": 5 })
    ));
    let error = rejected(
        &parent_with(inherited),
        json!({ "f": { "type": "string", "pattern": "^a" } }),
    );
    assert!(
        error.contains("widens the inherited declaration"),
        "{error}"
    );
}

/// Canary: inclusion is not satisfiability. An empty replacement is a subset
/// of anything and is admitted, by overrides and derivation alike.
#[test]
fn a_contradictory_replacement_is_admitted_as_an_empty_subset() {
    let inherited = json!({ "type": "string", "maxLength": 10 });
    assert!(paired(
        &inherited,
        &json!({ "type": "string", "minLength": 5, "maxLength": 3 })
    ));
}

#[test]
fn widening_is_rejected() {
    let inherited = json!({ "type": "string", "maxLength": 10 });
    assert!(!paired(
        &inherited,
        &json!({ "type": "string", "maxLength": 11 })
    ));
    assert!(!paired(
        &json!({ "type": "integer" }),
        &json!({ "type": "number" })
    ));
    // 2^53 + 1 vs 2^53: equal as f64, but the replacement is looser.
    let inherited = json!({ "type": "integer", "maximum": 9_007_199_254_740_992_u64 });
    assert!(!paired(
        &inherited,
        &json!({ "type": "integer", "maximum": 9_007_199_254_740_993_u64 })
    ));
    assert!(paired(
        &inherited,
        &json!({ "type": "integer", "maximum": 9_007_199_254_740_991_u64 })
    ));
    // Counts compare exactly as well.
    for (min, max) in [("minLength", "maxLength"), ("minItems", "maxItems")] {
        let inherited = json!({ max: 9_007_199_254_740_992_u64 });
        assert!(
            !paired(&inherited, &json!({ max: 9_007_199_254_740_993_u64 })),
            "{max}"
        );
        assert!(
            paired(&inherited, &json!({ max: 9_007_199_254_740_991_u64 })),
            "{max}"
        );
        let inherited = json!({ min: 9_007_199_254_740_993_u64 });
        assert!(
            !paired(&inherited, &json!({ min: 9_007_199_254_740_992_u64 })),
            "{min}"
        );
        assert!(
            paired(&inherited, &json!({ min: 9_007_199_254_740_994_u64 })),
            "{min}"
        );
    }
}

#[test]
fn a_finite_enum_proves_inclusion_without_restating_keywords() {
    let inherited = json!({ "type": "string", "maxLength": 3, "pattern": "^[a-z]+$" });
    assert!(paired(&inherited, &json!({ "enum": ["abc", "de"] })));
    assert!(!paired(&inherited, &json!({ "enum": ["abcd"] })));
    assert!(!paired(&inherited, &json!({ "enum": ["AB"] })));
    assert!(paired(
        &json!({ "enum": [1, { "a": [2] }] }),
        &json!({ "enum": [1.0, { "a": [2.0] }] })
    ));
    assert!(!paired(&json!({ "enum": [1, 2] }), &json!({ "enum": [3] })));
}

#[test]
fn patterns_other_than_the_inherited_one_are_not_proven() {
    let inherited = json!({ "type": "string", "pattern": "^us" });
    for pattern in [".*", "^eu", "^us-east"] {
        assert!(
            !paired(&inherited, &json!({ "type": "string", "pattern": pattern })),
            "{pattern}"
        );
        let error = rejected(
            &parent_with(inherited.clone()),
            json!({ "f": { "type": "string", "pattern": pattern } }),
        );
        assert!(error.contains("is not proven to be included"), "{error}");
    }
    // Dropping the pattern is a definite widening.
    assert!(!paired(&inherited, &json!({ "type": "string" })));
    // An enum discharges it.
    assert!(paired(&inherited, &json!({ "enum": ["us-east"] })));
    assert!(!paired(&inherited, &json!({ "enum": ["eu-west"] })));
}

#[test]
fn entity_reference_overrides_must_keep_or_narrow_the_inherited_scope() {
    const ROOT: &str = "gts.x.test.ovr_refs.target.v1~";
    const CHILD: &str = "gts.x.test.ovr_refs.target.v1~x.test.ovr_refs.child.v1~";
    const OTHER: &str = "gts.x.test.ovr_refs.other.v1~";

    for keyword in ["x-gts-type-ref", "x-gts-instance-ref"] {
        let field = |scope: &str| json!({ "type": "string", keyword: scope });
        assert!(paired(&field(ROOT), &field(ROOT)), "{keyword}: same scope");
        assert!(
            paired(&field(ROOT), &field(CHILD)),
            "{keyword}: narrower scope"
        );

        for replacement in [
            field(ROOT),
            field("gts.*"),
            field(OTHER),
            json!({ "type": "string" }),
        ] {
            assert!(
                !paired(&field(CHILD), &replacement),
                "{keyword}: {replacement} must not replace {CHILD}"
            );
            let error = rejected(&parent_with(field(CHILD)), json!({ "f": replacement }));
            assert!(error.contains("field '/f'"), "{error}");
            assert!(error.contains(keyword), "{error}");
        }
    }
}

#[test]
fn multiple_of_narrows_by_divisibility() {
    let inherited = json!({ "type": "integer", "multipleOf": 2 });
    assert!(paired(
        &inherited,
        &json!({ "type": "integer", "multipleOf": 4 })
    ));
    assert!(!paired(
        &json!({ "type": "integer", "multipleOf": 4 }),
        &json!({ "type": "integer", "multipleOf": 2 })
    ));
    assert!(!paired(
        &json!({ "type": "integer", "multipleOf": 4 }),
        &json!({ "type": "integer", "multipleOf": 6 })
    ));
}

#[test]
fn unique_items_cannot_be_dropped() {
    let inherited = json!({ "type": "array", "uniqueItems": true });
    assert!(!paired(&inherited, &json!({ "type": "array" })));
    assert!(paired(
        &json!({ "type": "array" }),
        &json!({ "type": "array", "uniqueItems": true })
    ));
}

#[test]
fn array_items_are_replaced_whole() {
    let inherited = json!({ "type": "array", "items": { "type": "string", "maxLength": 32 } });
    assert!(paired(
        &inherited,
        &json!({ "type": "array", "maxItems": 4, "items": { "type": "string", "maxLength": 8 } })
    ));
    assert!(!paired(
        &inherited,
        &json!({ "type": "array", "items": { "type": "string" } })
    ));
}

#[test]
fn a_nested_object_is_replaced_whole() {
    let inherited = json!({
        "type": "object",
        "properties": {
            "city": { "type": "string", "maxLength": 100 },
            "country": { "type": "string", "minLength": 2, "maxLength": 3 }
        },
        "required": ["city", "country"]
    });
    let full = json!({
        "type": "object",
        "properties": {
            "city": { "type": "string", "maxLength": 100 },
            "country": { "type": "string", "minLength": 2, "maxLength": 2 }
        },
        "required": ["city", "country"]
    });
    assert!(paired(&inherited, &full));

    // Omitting a required child, or a child's bound, is not inherited back.
    let mut no_city_required = full.clone();
    no_city_required["required"] = json!(["country"]);
    assert!(!paired(&inherited, &no_city_required));
    let mut no_city = full.clone();
    no_city["properties"]
        .as_object_mut()
        .unwrap()
        .remove("city");
    no_city["required"] = json!(["country"]);
    assert!(!paired(&inherited, &no_city));
    let mut unbounded_city = full;
    unbounded_city["properties"]["city"] = json!({ "type": "string" });
    assert!(!paired(&inherited, &unbounded_city));
}

/// Omissions are not inherited, but they need not widen: closing the object
/// excludes an optional field the replacement leaves out. This is inclusion
/// only: `Type::validate_overrides` still rejects the closed replacement
/// through derivation admission (see
/// `chain_admission_rejects_what_strict_replacement_admits`).
#[test]
fn a_closed_replacement_may_leave_out_an_optional_field() {
    let inherited = json!({
        "type": "object",
        "properties": { "a": { "type": "string" }, "b": { "type": "string" } },
        "required": ["a"]
    });
    let closed = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": { "a": { "type": "string" } },
        "required": ["a"]
    });
    assert!(paired(&inherited, &closed));
    let mut open = closed;
    open.as_object_mut().unwrap().remove("additionalProperties");
    assert!(!paired(&inherited, &open));
}

/// Ordinary derivation of a *composed* overlay folds the inherited nested
/// `properties`/`required` back in, so it would admit a partial nested
/// object; the override is checked before that fold.
#[test]
fn a_partial_nested_object_is_rejected_before_any_fold() {
    let inherited = json!({
        "type": "object",
        "properties": { "city": { "type": "string", "maxLength": 100 } },
        "required": ["city"]
    });
    let partial = json!({
        "type": "object",
        "properties": { "country": { "type": "string" } },
        "required": ["country"]
    });
    let error = rejected(&parent_with(inherited.clone()), json!({ "f": partial }));
    assert!(error.contains("field '/f'"), "{error}");

    let composed = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "allOf": [parent_with(inherited.clone()), { "properties": { "f": partial } }]
    });
    assert!(
        crate::schema_derivation::validate_derivation(
            &parent_with(inherited),
            &composed,
            "gts.x.t.base.v1~",
            TYPE_ID
        )
        .is_empty()
    );
}

#[test]
fn every_inherited_declaration_is_a_conjunct() {
    // The nested `allOf` pattern is folded away by the declared view.
    let parent = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "allOf": [
            { "properties": { "f": { "type": "string", "pattern": "^[a-z]+$" } } },
            { "properties": { "f": { "type": "string", "maxLength": 8, "allOf": [{ "pattern": "^eu" }] } } }
        ]
    });
    apply(&parent, json!({ "f": { "enum": ["euwest"] } })).unwrap();
    for value in ["uswest", "eu-west", "eunorthern"] {
        let error = rejected(&parent, json!({ "f": { "enum": [value] } }));
        assert!(
            error.contains("widens the inherited declaration"),
            "{value}: {error}"
        );
    }
    // Restating only the declared view of the field misses `^eu`.
    let error = rejected(
        &parent,
        json!({ "f": { "type": "string", "maxLength": 8, "pattern": "^[a-z]+$" } }),
    );
    assert!(error.contains("\"^eu\""), "{error}");
    // Two different patterns in one declaration are never flattened together.
    let error = rejected(
        &parent_with(json!({ "type": "string", "format": "uuid" })),
        json!({ "f": { "const": "not-a-uuid" } }),
    );
    assert!(error.contains("widens"), "{error}");
}

#[test]
fn a_level_that_could_hide_a_declaration_is_rejected() {
    let mut parent = parent_with(json!({ "type": "string" }));
    parent["anyOf"] = json!([{ "properties": { "f": { "maxLength": 3 } } }, true]);
    let error = rejected(&parent, json!({ "f": { "type": "string" } }));
    assert!(error.contains("uses 'anyOf'"), "{error}");
}

#[test]
fn a_sibling_additional_properties_constrains_the_field() {
    let parent = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "allOf": [
            { "properties": { "f": { "type": "string" } } },
            { "additionalProperties": { "maxLength": 3 } }
        ]
    });
    let error = rejected(&parent, json!({ "f": { "type": "string" } }));
    assert!(error.contains("maxLength"), "{error}");
    apply(
        &parent,
        json!({ "f": { "type": "string", "maxLength": 2 } }),
    )
    .unwrap();

    // Also on the way to a nested level, and when closed.
    let parent = json!({
        "type": "object",
        "allOf": [
            { "properties": { "f": { "type": "string" } } },
            { "additionalProperties": false }
        ]
    });
    let error = rejected(&parent, json!({ "f": { "type": "string" } }));
    assert!(error.contains("widens"), "{error}");
    // Only a `properties` entry declares a field.
    let parent = json!({ "type": "object", "additionalProperties": { "type": "string" } });
    let error = rejected(&parent, json!({ "f": { "type": "string" } }));
    assert!(error.contains("is not declared"), "{error}");
}

#[test]
fn a_false_level_on_the_path_is_rejected() {
    let parent = json!({
        "type": "object",
        "properties": { "payload": { "allOf": [
            { "properties": { "x": { "type": "string" } } },
            false
        ] } }
    });
    let error = validate_ancestor_overrides(
        &parent,
        TYPE_ID,
        &[AncestorOverride {
            ancestor_type_id: "gts.x.t.mid.v1~",
            path: vec!["payload"],
            generic_field: None,
            helper: helper(json!({ "x": { "type": "string" } })),
        }],
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("is `false`"), "{error}");
}

#[test]
fn deep_inherited_nesting_fails_closed() {
    let mut parent = parent_with(json!({ "type": "string", "maxLength": 3 }));
    for _ in 0..crate::schema_evolution::MAX_RECURSION_DEPTH {
        parent = json!({ "allOf": [parent] });
    }
    let error = rejected(
        &parent,
        json!({ "f": { "type": "string", "maxLength": 3 } }),
    );
    assert!(error.contains("nests deeper"), "{error}");
}

#[test]
fn a_false_field_schema_is_rejected_as_admission_would() {
    let error = rejected(
        &parent_with(json!({ "type": "string" })),
        json!({ "f": false }),
    );
    assert!(
        error.contains("would disable the inherited field"),
        "{error}"
    );
}

#[test]
fn self_operands_keep_their_declaring_document() {
    // `/$id` is the parent in the inherited field and this type in the
    // replacement; detached, both would read as the same literal.
    let inherited = json!({ "type": "string", "x-gts-pattern-ref": "/$id" });
    paired(&inherited, &inherited);
    assert!(paired(
        &inherited,
        &json!({ "type": "string", "x-gts-pattern-ref": "gts.x.t.base.v1~" })
    ));
    assert!(!paired(
        &inherited,
        &json!({ "type": "string", "x-gts-pattern-ref": "gts.x.t.other.v1~" })
    ));
}

// --- required and null ----------------------------------------------------------

fn level_parent(required: &[&str]) -> Value {
    let mut parent = parent_with(json!({ "type": "string", "maxLength": 8 }));
    parent["required"] = json!(required);
    parent
}

#[test]
fn an_optional_field_may_become_required() {
    let overlay = apply_helper(
        &level_parent(&[]),
        json!({ "type": "object", "properties": { "f": { "type": "string", "maxLength": 4 } }, "required": ["f"] }),
    )
    .unwrap();
    assert_eq!(overlay["required"], json!(["f"]));
}

#[test]
fn a_required_field_cannot_become_optional() {
    let error = apply_helper(
        &level_parent(&["f"]),
        helper(json!({ "f": { "type": "string", "maxLength": 4 } })),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("cannot make a field optional"), "{error}");
}

#[test]
fn a_nullable_replacement_needs_a_nullable_field() {
    let error = rejected(
        &parent_with(json!({ "type": "string" })),
        json!({ "f": { "type": ["string", "null"] } }),
    );
    assert!(error.contains("admits null"), "{error}");
    let error = rejected(
        &parent_with(json!({ "type": "object" })),
        json!({ "f": { "anyOf": [{ "type": "object" }, { "type": "null" }] } }),
    );
    assert!(error.contains("admits null"), "{error}");
}

#[test]
fn a_nullable_object_may_stay_nullable_or_become_non_null() {
    let inherited = json!({ "anyOf": [
        { "type": "object", "properties": {
            "a": { "type": "string", "maxLength": 3 }
        }, "required": ["a"] },
        { "type": "null" }
    ] });
    let narrowed = json!({ "type": "object", "properties": {
        "a": { "type": "string", "maxLength": 2 }
    }, "required": ["a"] });
    assert!(paired(&inherited, &narrowed));
    assert!(paired(
        &inherited,
        &json!({ "anyOf": [narrowed, { "type": "null" }] })
    ));
    // Non-null, but without the nested bound: rejected.
    assert!(!paired(
        &inherited,
        &json!({ "type": "object", "properties": { "a": { "type": "string" } }, "required": ["a"] })
    ));
    assert!(!paired(
        &inherited,
        &json!({ "type": "object", "properties": {
            "a": { "type": "string", "maxLength": 4 }
        }, "required": ["a"] })
    ));
    assert!(!paired(
        &inherited,
        &json!({ "type": "object", "properties": {
            "a": { "type": "string", "enum": ["long"] }
        }, "required": ["a"] })
    ));
}

#[test]
fn overrides_land_at_the_ancestor_path_with_required_union() {
    let parent = json!({
        "type": "object",
        "properties": {
            "payload": {
                "type": "object",
                "properties": { "region": { "type": "string" }, "data": { "type": "object" } },
                "required": ["region"]
            }
        }
    });
    let mut overlay = json!({
        "type": "object",
        "properties": { "payload": { "type": "object", "required": ["data"], "properties": { "data": {} } } }
    });
    let entry = AncestorOverride {
        ancestor_type_id: "gts.x.t.mid.v1~",
        path: vec!["payload"],
        generic_field: Some("data"),
        helper: json!({
            "type": "object",
            "properties": { "region": { "type": "string", "maxLength": 5 } },
            "required": ["region"]
        }),
    };
    validate_ancestor_overrides(&parent, TYPE_ID, std::slice::from_ref(&entry)).unwrap();
    apply_ancestor_overrides(&mut overlay, &[entry]);
    let level = &overlay["properties"]["payload"];
    assert_eq!(
        level["properties"]["region"],
        json!({ "type": "string", "maxLength": 5 })
    );
    assert_eq!(level["required"], json!(["data", "region"]));
}

#[test]
fn error_names_ancestor_and_wire_path() {
    let parent = json!({
        "type": "object",
        "properties": { "payload": { "type": "object", "properties": { "x": { "type": "string", "maxLength": 3 } } } }
    });
    let error = validate_ancestor_overrides(
        &parent,
        TYPE_ID,
        &[AncestorOverride {
            ancestor_type_id: "gts.x.t.mid.v1~",
            path: vec!["payload"],
            generic_field: None,
            helper: helper(json!({ "x": { "type": "string", "maxLength": 4 } })),
        }],
    )
    .unwrap_err();
    assert_eq!(error.ancestor, "gts.x.t.mid.v1~");
    assert_eq!(error.path, "/payload/x");
    assert!(
        error
            .to_string()
            .starts_with("overrides for ancestor 'gts.x.t.mid.v1~'")
    );
}

// --- validate_derivation_chain ---------------------------------------------------

const BASE_ID: &str = "gts.x.t.ovr.base.v1~";
const CHILD_ID: &str = "gts.x.t.ovr.base.v1~x.t.ovr.child.v1~";
const MID_ID: &str = "gts.x.t.ovr.base.v1~x.t.ovr.mid.v1~";
const LEAF_ID: &str = "gts.x.t.ovr.base.v1~x.t.ovr.mid.v1~x.t.ovr.leaf.v1~";

/// `id`'s document: `allOf` of its parent's `$ref` and `overlay`.
#[allow(clippy::needless_pass_by_value)]
fn derived_doc(id: &str, overlay: Value) -> Value {
    json!({
        "$id": format!("gts://{id}"),
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "allOf": [{ "$ref": format!("gts://{}", parent_uri(id).unwrap()) }, overlay]
    })
}

/// [`parent_with`] registered as [`BASE_ID`].
#[allow(clippy::needless_pass_by_value)]
fn base_with(field: Value) -> Value {
    let mut base = parent_with(field);
    base["$id"] = json!(format!("gts://{BASE_ID}"));
    base
}

fn nested_parent() -> Value {
    base_with(json!({
        "type": "object",
        "properties": { "a": { "type": "string" }, "b": { "type": "string" } },
        "required": ["a"]
    }))
}

/// Replaces `f` with a closed object that drops the optional `b`.
fn closed_f_override() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": { "a": { "type": "string" } },
        "required": ["a"]
    })
}

#[test]
fn chain_admission_rejects_what_strict_replacement_admits() {
    let parent = nested_parent();
    validate_ancestor_overrides(
        &parent,
        CHILD_ID,
        &[entry(vec![], helper(json!({ "f": closed_f_override() })))],
    )
    .unwrap();
    let child = derived_doc(
        CHILD_ID,
        json!({ "properties": { "f": closed_f_override() } }),
    );
    let error = validate_derivation_chain(CHILD_ID, &[parent, child]).unwrap_err();
    assert_eq!(error.ancestor, BASE_ID);
    assert_eq!(error.path, "/");
    assert!(error.message.contains("f.b"), "{error}");
}

#[test]
fn chain_admission_admits_a_valid_chain() {
    let narrowed = json!({
        "type": "object",
        "properties": { "a": { "type": "string", "maxLength": 3 }, "b": { "type": "string" } },
        "required": ["a"]
    });
    let chain = [
        nested_parent(),
        derived_doc(MID_ID, json!({ "properties": { "f": narrowed } })),
        derived_doc(
            LEAF_ID,
            json!({ "properties": { "payload": { "type": "object" } } }),
        ),
    ];
    validate_derivation_chain(LEAF_ID, &chain).unwrap();
}

#[test]
fn chain_admission_reports_an_intermediate_ancestor() {
    let chain = [
        nested_parent(),
        derived_doc(
            MID_ID,
            json!({ "properties": { "f": closed_f_override() } }),
        ),
        derived_doc(
            LEAF_ID,
            json!({ "properties": { "payload": { "type": "object" } } }),
        ),
    ];
    let error = validate_derivation_chain(LEAF_ID, &chain).unwrap_err();
    assert_eq!(error.ancestor, MID_ID);
    assert!(error.message.contains(MID_ID), "{error}");
    assert!(error.message.contains("f.b"), "{error}");
}

/// Only OP#12: trait values and entity-reference targets are not checked.
#[test]
fn chain_admission_runs_no_trait_or_reference_validation() {
    let mut base =
        base_with(json!({ "type": "string", "x-gts-type-ref": "gts.x.t.ovr.missing.v1~" }));
    base["x-gts-traits-schema"] = json!({
        "type": "object",
        "properties": { "retention": { "type": "string" } }
    });
    let child = derived_doc(
        CHILD_ID,
        json!({ "x-gts-traits": { "retention": 5 }, "properties": { "payload": { "type": "object" } } }),
    );
    let chain = [base, child];
    validate_derivation_chain(CHILD_ID, &chain).unwrap();

    let mut store = crate::GtsStore::new();
    for schema in &chain {
        let id = crate::GtsStore::declared_type_id(schema).unwrap();
        store.register_schema(&id, schema).unwrap();
    }
    assert!(store.validate_schema(CHILD_ID).is_err());
}

const TRAIT_ID: &str = "gts.x.t.ovr.retention.v1~";

/// A store holding `extra` and `chain`, as full registration sees them.
fn registry(chain: &[Value], extra: &[Value]) -> crate::GtsStore {
    let mut store = crate::GtsStore::new();
    for schema in extra.iter().chain(chain) {
        let id = crate::GtsStore::declared_type_id(schema).unwrap();
        store.register_schema(&id, schema).unwrap();
    }
    store
}

/// The base's trait schema `$ref`s a separately registered trait type, as
/// `traits_schema = T` emits; the child supplies its value.
fn trait_ref_chain(child_overlay: Value) -> [Value; 2] {
    let mut base = nested_parent();
    base["x-gts-traits-schema"] = json!({
        "type": "object",
        "allOf": [{ "$ref": format!("gts://{TRAIT_ID}") }]
    });
    let mut child = derived_doc(CHILD_ID, child_overlay);
    child["x-gts-traits"] = json!({ "retention": "P1D" });
    [base, child]
}

fn retention_trait() -> Value {
    json!({
        "$id": format!("gts://{TRAIT_ID}"),
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": { "retention": { "type": "string" } },
        "required": ["retention"]
    })
}

#[test]
fn chain_admission_ignores_trait_types_outside_the_chain() {
    let mut chain = trait_ref_chain(json!({ "properties": { "payload": { "type": "object" } } }));
    chain[0]["x-gts-abstract"] = json!(true);
    validate_derivation_chain(CHILD_ID, &chain).unwrap();
    registry(&chain, &[retention_trait()])
        .validate_schema(CHILD_ID)
        .unwrap();
}

#[test]
fn chain_admission_with_trait_metadata_still_rejects_payload_and_final() {
    let chain = trait_ref_chain(json!({ "properties": { "f": closed_f_override() } }));
    let error = validate_derivation_chain(CHILD_ID, &chain).unwrap_err();
    assert!(error.message.contains("f.b"), "{error}");

    let [mut base, child] =
        trait_ref_chain(json!({ "properties": { "payload": { "type": "object" } } }));
    base["x-gts-final"] = json!(true);
    let error = validate_derivation_chain(CHILD_ID, &[base, child]).unwrap_err();
    assert!(error.message.contains("is final"), "{error}");
}

/// Trait-schema compatibility is OP#13: full registration rejects what chain
/// admission leaves unchecked.
#[test]
fn chain_admission_leaves_trait_schema_compatibility_to_registration() {
    let mut base = nested_parent();
    base["x-gts-traits-schema"] = json!({
        "type": "object",
        "properties": { "retention": { "type": "string" } }
    });
    let mut child = derived_doc(
        CHILD_ID,
        json!({ "properties": { "payload": { "type": "object" } } }),
    );
    child["x-gts-traits-schema"] = json!({
        "type": "object",
        "properties": { "retention": { "type": ["string", "integer"] } }
    });
    let chain = [base, child];
    validate_derivation_chain(CHILD_ID, &chain).unwrap();
    let error = registry(&chain, &[])
        .validate_schema(CHILD_ID)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("incompatible with ancestor trait schema"),
        "{error}"
    );
}

// --- const helpers -------------------------------------------------------------

#[test]
fn strict_ancestor_check() {
    assert!(is_strict_ancestor_type_id(
        "gts.a.b.c.d.v1~",
        "gts.a.b.c.d.v1~e.f.g.h.v1~"
    ));
    assert!(!is_strict_ancestor_type_id(
        "gts.a.b.c.d.v1~",
        "gts.a.b.c.d.v1~"
    ));
    assert!(!is_strict_ancestor_type_id("", "gts.a.b.c.d.v1~"));
    assert!(!is_strict_ancestor_type_id(
        "gts.a.b.c.d.v1~x.y.z.w.v1~",
        "gts.a.b.c.d.v1~e.f.g.h.v1~"
    ));
    assert!(const_str_eq("abc", "abc"));
    assert!(!const_str_eq("abc", "abd"));
}
