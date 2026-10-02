#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use serde_json::json;

/// Fails a backtracking engine on long runs of `a` before its second
/// alternative matches them followed by `!`.
const EXHAUSTING: &str = "^(?:((a|aa)(?=a?))+$|a+!$)";

const DRAFT4: &str = "http://json-schema.org/draft-04/schema#";
const DRAFT7: &str = "http://json-schema.org/draft-07/schema#";
const DRAFT2019: &str = "https://json-schema.org/draft/2019-09/schema";
const DRAFT2020: &str = "https://json-schema.org/draft/2020-12/schema";

fn long() -> String {
    format!("{}!", "a".repeat(64))
}

fn short() -> String {
    "aaaa!".to_owned()
}

fn pattern() -> Value {
    json!({"pattern": EXHAUSTING})
}

fn exhausted(schema: &Value, instance: &Value) -> Result<(), RegexCheckError> {
    exhausted_match(schema, &[], instance)
}

/// The validator compiles `schema`, and the replay reports an exhausted match.
#[track_caller]
fn assert_exhausted(schema: &Value, instance: &Value) {
    jsonschema::validator_for(schema).unwrap_or_else(|e| panic!("{schema} must compile: {e}"));
    let result = exhausted(schema, instance);
    assert!(
        matches!(result, Err(RegexCheckError::Exhausted(_))),
        "{schema} on {instance}: {result:?}"
    );
}

/// The validator compiles `schema`, and the replay finds nothing to report.
#[track_caller]
fn assert_unaffected(schema: &Value, instance: &Value) {
    jsonschema::validator_for(schema).unwrap_or_else(|e| panic!("{schema} must compile: {e}"));
    let result = exhausted(schema, instance);
    assert_eq!(result, Ok(()), "{schema} on {instance}");
}

#[track_caller]
fn assert_unchecked(result: &Result<(), RegexCheckError>) {
    assert!(
        matches!(result, Err(RegexCheckError::Unchecked(_))),
        "{result:?}"
    );
}

#[test]
fn jsonschema_still_drops_regex_engine_errors() {
    // This module works around `jsonschema` 0.57 inverting an engine failure
    // under `not`. When this fails, the dependency reports the failure
    // itself: remove the module once the regression tests pass without it.
    let schema = json!({"not": {"pattern": EXHAUSTING}});
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&json!(long())));
}

#[test]
fn reports_the_exhausted_pattern() {
    let error = exhausted(&pattern(), &json!(long())).unwrap_err();
    assert!(matches!(error, RegexCheckError::Exhausted(_)), "{error:?}");
    assert!(error.to_string().contains(EXHAUSTING), "{error}");
    assert_eq!(exhausted(&pattern(), &json!(short())), Ok(()));
}

/// Schemas and instances where the validator performs an exhausting match.
fn exhausting_cases() -> Vec<(Value, Value)> {
    let p = pattern();
    vec![
        (json!({"$schema": DRAFT2020, "not": p}), json!(long())),
        (
            json!({
                "$schema": DRAFT2020,
                "patternProperties": {EXHAUSTING: true},
                "additionalProperties": false
            }),
            json!({long(): 1}),
        ),
        (
            json!({"$schema": DRAFT2020, "propertyNames": p}),
            json!({long(): 1}),
        ),
        (
            json!({"$schema": DRAFT2020, "properties": {"a": p}}),
            json!({"a": long()}),
        ),
        (
            json!({"$schema": DRAFT2020, "properties": {"a": true}, "additionalProperties": p}),
            json!({"a": 1, "b": long()}),
        ),
        (json!({"$schema": DRAFT2020, "items": p}), json!([long()])),
        (
            json!({"$schema": DRAFT2020, "prefixItems": [p]}),
            json!([long()]),
        ),
        (
            json!({"$schema": DRAFT2020, "prefixItems": [true], "items": p}),
            json!([1, long()]),
        ),
        (json!({"$schema": DRAFT7, "items": p}), json!([long()])),
        (
            json!({"$schema": DRAFT7, "items": [true, p]}),
            json!([1, long()]),
        ),
        (
            json!({"$schema": DRAFT7, "items": [true], "additionalItems": p}),
            json!([1, long()]),
        ),
        (
            json!({"$schema": DRAFT2019, "items": [true], "additionalItems": p}),
            json!([1, long()]),
        ),
        (
            json!({"$schema": DRAFT2020, "contains": p}),
            json!([long()]),
        ),
        // `oneOf` evaluates branches until a second one holds.
        (
            json!({"$schema": DRAFT2020, "oneOf": [true, p]}),
            json!(long()),
        ),
        (
            json!({"$schema": DRAFT2020, "if": true, "then": p}),
            json!(long()),
        ),
        (
            json!({"$schema": DRAFT2020, "if": false, "else": p}),
            json!(long()),
        ),
        (
            json!({"$schema": DRAFT7, "if": {}, "then": p}),
            json!(long()),
        ),
        (
            json!({"$schema": DRAFT2020, "dependentSchemas": {"a": {"properties": {"b": p}}}}),
            json!({"a": 1, "b": long()}),
        ),
        (
            json!({"$schema": DRAFT7, "dependencies": {"a": {"properties": {"b": p}}}}),
            json!({"a": 1, "b": long()}),
        ),
        (
            json!({"$schema": DRAFT2019, "unevaluatedProperties": p}),
            json!({"a": long()}),
        ),
        (
            json!({"$schema": DRAFT2020, "unevaluatedItems": p}),
            json!([long()]),
        ),
        (
            json!({
                "$schema": DRAFT2019,
                "$ref": "#/$defs/ok",
                "$defs": {"ok": true},
                "pattern": EXHAUSTING
            }),
            json!(long()),
        ),
        (
            json!({
                "$schema": DRAFT2019,
                "type": "object",
                "properties": {"a": {"$recursiveRef": "#"}},
                "propertyNames": {"maxLength": 1},
                "additionalProperties": p
            }),
            json!({"a": {"b": long()}}),
        ),
    ]
}

#[test]
fn replays_every_place_the_validator_matches_a_regex() {
    for (schema, instance) in exhausting_cases() {
        assert_exhausted(&schema, &instance);
    }
}

#[test]
fn gts_validators_and_the_replay_apply_dependencies_through_draft_7_only() {
    // `jsonschema` 0.57 applies `dependencies` past draft 7 too; GTS
    // validators follow the dialect, so the replay can skip it there.
    let constraint = json!({"a": {"type": "string"}});
    let replayed = json!({"a": {"properties": {"b": pattern()}}});
    for (draft, applies) in [(DRAFT7, true), (DRAFT2019, false), (DRAFT2020, false)] {
        let schema = json!({"$schema": draft, "dependencies": constraint});
        let validator = crate::json_schema::validator_for(&schema).unwrap();
        assert_eq!(!validator.is_valid(&json!({"a": 1})), applies, "{draft}");

        let schema = json!({"$schema": draft, "dependencies": replayed});
        let result = exhausted(&schema, &json!({"a": 1, "b": long()}));
        assert_eq!(result.is_err(), applies, "{draft}: {result:?}");
    }
}

/// Schemas and instances where the validator performs no exhausting match.
fn unaffected_cases() -> Vec<(Value, Value)> {
    let p = pattern();
    vec![
        // `additionalProperties` covers only what the others leave.
        (
            json!({"$schema": DRAFT2020, "properties": {"a": true}, "additionalProperties": p}),
            json!({"a": long()}),
        ),
        (
            json!({
                "$schema": DRAFT2020,
                "patternProperties": {"^a$": true},
                "additionalProperties": p
            }),
            json!({"a": long()}),
        ),
        // 2020-12 `items` starts after `prefixItems`.
        (
            json!({"$schema": DRAFT2020, "prefixItems": [true], "items": p}),
            json!([long()]),
        ),
        // `additionalItems` needs an `items` tuple, and covers its tail only.
        (
            json!({"$schema": DRAFT7, "items": true, "additionalItems": p}),
            json!([long()]),
        ),
        (
            json!({"$schema": DRAFT7, "additionalItems": p}),
            json!([long()]),
        ),
        (
            json!({"$schema": DRAFT7, "items": [true], "additionalItems": p}),
            json!([long()]),
        ),
        (
            json!({"$schema": DRAFT2020, "prefixItems": [true], "additionalItems": p}),
            json!([1, long()]),
        ),
        // Keywords from later dialects.
        (
            json!({"$schema": DRAFT7, "prefixItems": [p]}),
            json!([long()]),
        ),
        (
            json!({"$schema": DRAFT2019, "prefixItems": [p]}),
            json!([long()]),
        ),
        (
            json!({"$schema": DRAFT7, "dependentSchemas": {"a": {"properties": {"b": p}}}}),
            json!({"a": 1, "b": long()}),
        ),
        (
            json!({"$schema": DRAFT7, "unevaluatedProperties": p}),
            json!({"a": long()}),
        ),
        (
            json!({"$schema": DRAFT7, "unevaluatedItems": p}),
            json!([long()]),
        ),
        (
            json!({"$schema": DRAFT4, "if": true, "then": p}),
            json!(long()),
        ),
        (json!({"$schema": DRAFT4, "contains": p}), json!([long()])),
        (
            json!({"$schema": DRAFT4, "propertyNames": p}),
            json!({long(): 1}),
        ),
        (
            json!({
                "$schema": DRAFT7,
                "$dynamicRef": "#/definitions/text",
                "definitions": {"text": p}
            }),
            json!(long()),
        ),
        (
            json!({
                "$schema": DRAFT2019,
                "$dynamicRef": "#/$defs/text",
                "$defs": {"text": p}
            }),
            json!(long()),
        ),
        (
            json!({
                "$schema": DRAFT2020,
                "properties": {"a": {"$recursiveRef": "#"}},
                "additionalProperties": p
            }),
            json!({"a": {"b": long()}}),
        ),
        // Before 2019-09, `$ref` makes its siblings inert.
        (
            json!({
                "$schema": DRAFT7,
                "$ref": "#/definitions/ok",
                "definitions": {"ok": true},
                "pattern": EXHAUSTING
            }),
            json!(long()),
        ),
        // `then` and `else` follow from `if` alone.
        (
            json!({"$schema": DRAFT2020, "if": true, "then": true, "else": p}),
            json!(long()),
        ),
        (
            json!({"$schema": DRAFT2020, "if": false, "then": p}),
            json!(long()),
        ),
        (json!({"$schema": DRAFT2020, "then": p}), json!(long())),
        (json!({"$schema": DRAFT2020, "else": p}), json!(long())),
        // Annotations hold data, not subschemas.
        (
            json!({
                "$schema": DRAFT2020,
                "default": p,
                "examples": [p],
                "x-note": {"properties": {"a": p}}
            }),
            json!({"a": long()}),
        ),
    ]
}

#[test]
fn skips_what_the_validator_does_not_evaluate() {
    for (schema, instance) in unaffected_cases() {
        assert_unaffected(&schema, &instance);
    }
}

/// `schema` with `extra` annotations added to each of its schema objects.
fn widened(schema: &Value, extra: usize) -> Value {
    let Value::Object(map) = schema else {
        return schema.clone();
    };
    let mut wide = Map::new();
    for (keyword, value) in map {
        let value = match (keyword.as_str(), value) {
            (
                "properties" | "patternProperties" | "$defs" | "definitions" | "dependentSchemas"
                | "dependencies",
                Value::Object(members),
            ) => Value::Object(
                members
                    .iter()
                    .map(|(name, subschema)| (name.clone(), widened(subschema, extra)))
                    .collect(),
            ),
            ("allOf" | "anyOf" | "oneOf" | "prefixItems" | "items", Value::Array(subschemas)) => {
                Value::Array(
                    subschemas
                        .iter()
                        .map(|subschema| widened(subschema, extra))
                        .collect(),
                )
            }
            (
                "not"
                | "if"
                | "then"
                | "else"
                | "items"
                | "additionalItems"
                | "unevaluatedItems"
                | "contains"
                | "additionalProperties"
                | "unevaluatedProperties"
                | "propertyNames",
                subschema,
            ) => widened(subschema, extra),
            _ => value.clone(),
        };
        wide.insert(keyword.clone(), value);
    }
    for index in 0..extra {
        wide.insert(format!("x-note-{index:02}"), json!(index));
    }
    Value::Object(wide)
}

#[test]
fn wide_schema_objects_replay_like_narrow_ones() {
    // Twenty annotations make every schema object wide, so its keywords are
    // searched by name instead of scanned.
    for (schema, instance) in exhausting_cases() {
        assert_exhausted(&widened(&schema, 20), &instance);
    }
    for (schema, instance) in unaffected_cases() {
        assert_unaffected(&widened(&schema, 20), &instance);
    }
}

#[test]
fn keywords_found_by_name_match_a_scan_at_every_switch_point() {
    let text = "x".to_owned();
    let (string, object, array) = (json!("x"), json!({}), json!([]));
    let subjects = [
        (Subject::Name(&text), &STRING_KEYWORDS[..]),
        (Subject::Value(&string), &STRING_KEYWORDS[..]),
        (Subject::Value(&object), &OBJECT_KEYWORDS[..]),
        (Subject::Value(&array), &ARRAY_KEYWORDS[..]),
    ];
    for (subject, specific) in subjects {
        let applicable: Vec<&str> = GENERAL_KEYWORDS.iter().chain(specific).copied().collect();
        // One keyword fewer than the switch point, at it, and past it.
        for (dropped, annotations) in [(1, 0), (0, 0), (0, 1), (0, 2)] {
            let mut node = Map::new();
            for name in &applicable[dropped..] {
                node.insert((*name).to_owned(), json!(true));
            }
            for index in 0..annotations {
                node.insert(format!("x-note-{index}"), json!(true));
            }
            let mut keywords = Keywords::of(&node, subject);
            for name in &applicable {
                let found = keywords.slot(name).is_some_and(|slot| slot.is_some());
                assert_eq!(found, node.contains_key(*name), "{name} in {node:?}");
            }
        }
    }
}

#[test]
fn descends_only_into_matching_property_names() {
    let schema = json!({"patternProperties": {"^x$": {"pattern": EXHAUSTING}}});
    assert_eq!(exhausted(&schema, &json!({"y": long()})), Ok(()));
    assert_exhausted(&schema, &json!({"x": long()}));
}

#[test]
fn follows_local_and_resource_references() {
    let local = json!({
        "definitions": {"text": {"pattern": EXHAUSTING}},
        "properties": {"a": {"$ref": "#/definitions/text"}}
    });
    assert_exhausted(&local, &json!({"a": long()}));

    let library = json!({"$defs": {"text": {"pattern": EXHAUSTING}}});
    let user = json!({"properties": {"a": {"$ref": "gts://lib#/$defs/text"}}});
    let resources = [("gts://lib".to_owned(), &library)];
    let result = exhausted_match(&user, &resources, &json!({"a": long()}));
    assert!(
        matches!(result, Err(RegexCheckError::Exhausted(_))),
        "{result:?}"
    );
    assert_eq!(
        exhausted_match(&user, &resources, &json!({"a": short()})),
        Ok(())
    );
}

#[test]
fn follows_anchors() {
    let anchor = json!({
        "$schema": DRAFT2020,
        "$defs": {"text": {"$anchor": "text", "pattern": EXHAUSTING}},
        "properties": {"a": {"$ref": "#text"}}
    });
    assert_exhausted(&anchor, &json!({"a": long()}));

    let plain_name = json!({
        "$schema": DRAFT7,
        "definitions": {"text": {"$id": "#text", "pattern": EXHAUSTING}},
        "properties": {"a": {"$ref": "#text"}}
    });
    assert_exhausted(&plain_name, &json!({"a": long()}));
}

#[test]
fn follows_escaped_json_pointers() {
    let schema = json!({
        "$schema": DRAFT2020,
        "properties": {
            "slash": {"$ref": "#/$defs/a~1b"},
            "tilde": {"$ref": "#/$defs/c~0d"},
            "percent": {"$ref": "#/$defs/50%25"}
        },
        "$defs": {
            "a/b": {"pattern": EXHAUSTING},
            "c~d": {"pattern": EXHAUSTING},
            "50%": {"pattern": EXHAUSTING}
        }
    });
    for property in ["slash", "tilde", "percent"] {
        assert_exhausted(&schema, &json!({property: long()}));
    }
    assert_eq!(
        exhausted(
            &schema,
            &json!({"slash": "b", "tilde": "b", "percent": "b"})
        ),
        Ok(())
    );
}

#[test]
fn resolves_references_against_embedded_resources() {
    // `#/$defs/text` and `inner` resolve against the resource declaring
    // them, not against the document root.
    let schema = json!({
        "$schema": DRAFT2020,
        "$id": "https://example.com/root",
        "properties": {"x": {"$ref": "inner"}},
        "$defs": {
            "inner": {
                "$id": "inner",
                "properties": {"a": {"$ref": "#/$defs/text"}},
                "$defs": {"text": {"pattern": EXHAUSTING}}
            }
        }
    });
    assert_exhausted(&schema, &json!({"x": {"a": long()}}));
    assert_eq!(exhausted(&schema, &json!({"x": {"a": short()}})), Ok(()));
}

#[test]
fn applies_an_id_once_when_a_reference_enters_its_resource() {
    // The lookup of `#/$defs/inner` already stands in `dir/inner`; applying
    // the `$id` again would look for `dir/dir/inner`.
    let schema = json!({
        "$schema": DRAFT2020,
        "properties": {"value": {"$ref": "#/$defs/inner"}},
        "$defs": {
            "inner": {
                "$id": "dir/inner",
                "not": {"$ref": "#/$defs/text"},
                "$defs": {"text": {"pattern": EXHAUSTING}}
            }
        }
    });
    assert_exhausted(&schema, &json!({"value": long()}));
    assert_eq!(exhausted(&schema, &json!({"value": "b"})), Ok(()));
}

/// A GTS validator over `schema` and `resources`, as the store builds one.
fn gts_validator(schema: &Value, resources: &[(String, &Value)]) -> jsonschema::Validator {
    crate::json_schema::gts_validator_for_type(schema, None, resources, None)
        .unwrap_or_else(|e| panic!("{schema} must compile: {e}"))
}

#[test]
fn applies_a_relative_root_id_as_the_validator_does() {
    // `jsonschema` 0.57 applies a relative root `$id` twice: `p` resolves to
    // `dir/dir/p`, whose pattern the validator inverts under `not`.
    let schema = json!({
        "$schema": DRAFT2020,
        "$id": "dir/root",
        "not": {"$ref": "p"},
        "$defs": {
            "single": {"$id": "json-schema:///dir/p", "type": "string"},
            "double": {"$id": "json-schema:///dir/dir/p", "pattern": EXHAUSTING}
        }
    });
    let validator = crate::json_schema::validator_for(&schema).unwrap();
    assert!(
        validator.is_valid(&json!(long())),
        "the validator reaches `double`"
    );
    assert!(matches!(
        exhausted(&schema, &json!(long())),
        Err(RegexCheckError::Exhausted(_))
    ));
    let (verdict, _) =
        crate::schema_evolution::check_accepted_set_inclusion(&json!({"enum": [long()]}), &schema);
    assert_ne!(
        verdict,
        crate::schema_evolution::CompatibilityVerdict::Compatible
    );
}

#[test]
fn reaches_local_targets_under_a_relative_root_id() {
    let schema = json!({
        "$schema": DRAFT2020,
        "$id": "dir/root",
        "properties": {"a": {"not": {"$ref": "#/$defs/text"}}},
        "$defs": {"text": pattern()}
    });
    assert_exhausted(&schema, &json!({"a": long()}));
    assert_eq!(exhausted(&schema, &json!({"a": "b"})), Ok(()));

    let schema = json!({
        "$schema": DRAFT2020,
        "$id": "dir/root",
        "$dynamicRef": "#text",
        "$defs": {"text": {"$dynamicAnchor": "text", "not": pattern()}}
    });
    assert_exhausted(&schema, &json!(long()));
}

#[test]
fn follows_a_reference_outside_the_schema_positions() {
    // A pointer may name any value the validator then compiles as a schema;
    // its pattern is compiled on first use.
    let schema = json!({
        "$schema": DRAFT2020,
        "properties": {"a": {"not": {"$ref": "#/default/text"}}},
        "default": {"text": pattern()}
    });
    assert_exhausted(&schema, &json!({"a": long()}));
}

#[test]
fn a_reference_to_a_dynamic_anchor_enters_every_target_its_scope_may_select() {
    // `referencing` 0.57 resolves `$ref: "#node"` dynamically too: through
    // `s`, `t`'s children may land on `s`, whose `name` the validator
    // inverts. Which compiled scope the validator keeps depends on the order
    // it meets them, so the replay must report the failure in either.
    let t = json!({
        "$schema": DRAFT2020,
        "$id": "https://example.com/t",
        "$dynamicAnchor": "node",
        "properties": {"children": {"items": {"$ref": "#node"}}}
    });
    let s = json!({
        "$schema": DRAFT2020,
        "$id": "https://example.com/s",
        "$dynamicAnchor": "node",
        "$ref": "t",
        "properties": {"name": {"not": {"pattern": EXHAUSTING}}}
    });
    let resources = [
        ("https://example.com/t".to_owned(), &t),
        ("https://example.com/s".to_owned(), &s),
    ];
    for order in [["s", "t"], ["t", "s"]] {
        let root = json!({
            "$schema": DRAFT2020,
            "$id": "https://example.com/r",
            "allOf": [{"$ref": order[0]}, {"$ref": order[1]}]
        });
        let validator = gts_validator(&root, &resources);
        let instance = json!({"children": [{"name": long()}]});
        if order[0] == "s" {
            assert!(
                !validator.is_valid(&json!({"children": [{"name": short()}]})),
                "the validator reaches `s` through the dynamic scope"
            );
            assert!(validator.is_valid(&instance), "and inverts its failure");
        }
        let result = exhausted_match(&root, &resources, &instance);
        assert!(
            matches!(result, Err(RegexCheckError::Exhausted(_))),
            "{order:?}: {result:?}"
        );
    }
}

#[test]
fn a_selected_dynamic_anchor_resolves_from_the_reference_target() {
    // `s`'s anchor has no `$id`, so once selected it stands in `t`, the
    // target of the reference: its `#/$defs/text` is `t`'s pattern, not
    // `s`'s integer.
    let t = json!({
        "$schema": DRAFT2020,
        "$id": "https://example.com/t",
        "$dynamicAnchor": "node",
        "properties": {"children": {"items": {"$dynamicRef": "#node"}}},
        "$defs": {"text": {"pattern": EXHAUSTING}}
    });
    let s = json!({
        "$schema": DRAFT2020,
        "$id": "https://example.com/s",
        "$ref": "t",
        "$defs": {
            "override": {
                "$dynamicAnchor": "node",
                "properties": {"name": {"not": {"$ref": "#/$defs/text"}}}
            },
            "text": {"type": "integer"}
        }
    });
    let resources = [
        ("https://example.com/t".to_owned(), &t),
        ("https://example.com/s".to_owned(), &s),
    ];
    let root = json!({
        "$schema": DRAFT2020,
        "$id": "https://example.com/r",
        "allOf": [{"$ref": "s"}, {"$ref": "t"}]
    });
    let validator = gts_validator(&root, &resources);
    let instance = json!({"children": [{"name": long()}]});
    assert!(
        validator.is_valid(&instance),
        "the validator inverts `t`'s pattern"
    );
    let result = exhausted_match(&root, &resources, &instance);
    assert!(
        matches!(result, Err(RegexCheckError::Exhausted(_))),
        "{result:?}"
    );
}

#[test]
fn enters_every_dynamic_reference_target() {
    // The dynamic scope makes `$dynamicRef` reach the extension, whose
    // pattern the static target lacks.
    let tree = json!({
        "$schema": DRAFT2020,
        "$id": "https://example.com/tree",
        "$dynamicAnchor": "node",
        "properties": {"children": {"items": {"$dynamicRef": "#node"}}}
    });
    let strict = json!({
        "$schema": DRAFT2020,
        "$id": "https://example.com/strict",
        "$dynamicAnchor": "node",
        "$ref": "tree",
        "properties": {"name": {"pattern": EXHAUSTING}}
    });
    let resources = [("https://example.com/tree".to_owned(), &tree)];
    let instance = json!({"children": [{"name": long()}]});
    let result = exhausted_match(&strict, &resources, &instance);
    assert!(
        matches!(result, Err(RegexCheckError::Exhausted(_))),
        "{result:?}"
    );
    let instance = json!({"children": [{"name": short()}]});
    assert_eq!(exhausted_match(&strict, &resources, &instance), Ok(()));
}

#[test]
fn dynamic_anchors_count_only_in_schema_positions() {
    let p = pattern();
    let data = json!({
        "$id": "https://example.com/data",
        "$dynamicAnchor": "text",
        "pattern": EXHAUSTING
    });
    for (annotation, value) in [
        ("default", data.clone()),
        ("const", data.clone()),
        ("x-note", data.clone()),
        ("examples", json!([data])),
    ] {
        let schema = json!({
            "$schema": DRAFT2020,
            "$dynamicRef": "#text",
            "$defs": {"ok": {"$dynamicAnchor": "text"}},
            annotation: value
        });
        assert_unaffected(&schema, &json!(long()));
    }
    let schema = json!({
        "$schema": DRAFT2020,
        "$dynamicRef": "#text",
        "$defs": {"ok": {"$dynamicAnchor": "text"}},
        "enum": [{"$dynamicAnchor": "text", "pattern": EXHAUSTING}, p]
    });
    assert_unaffected(&schema, &json!(long()));
}

#[test]
fn a_dynamic_reference_to_a_plain_anchor_stays_static() {
    // `#text` names an `$anchor`, so no dynamic scope can redirect it, even
    // with a same-named `$dynamicAnchor` elsewhere or a different one on the
    // target itself.
    for target in [
        json!({"$anchor": "text"}),
        json!({"$anchor": "text", "$dynamicAnchor": "other"}),
    ] {
        let schema = json!({
            "$schema": DRAFT2020,
            "$dynamicRef": "#text",
            "$defs": {
                "ok": target,
                "unused": {
                    "$id": "https://example.com/unused",
                    "$dynamicAnchor": "text",
                    "pattern": EXHAUSTING
                }
            }
        });
        assert_unaffected(&schema, &json!(long()));
    }
    // A JSON Pointer is static too.
    let schema = json!({
        "$schema": DRAFT2020,
        "$dynamicRef": "#/$defs/ok",
        "$defs": {
            "ok": {"$dynamicAnchor": "text"},
            "unused": {
                "$id": "https://example.com/unused",
                "$dynamicAnchor": "text",
                "pattern": EXHAUSTING
            }
        }
    });
    assert_unaffected(&schema, &json!(long()));
}

#[test]
fn finds_dynamic_anchors_under_escaped_keys() {
    let schema = json!({
        "$schema": DRAFT2020,
        "$dynamicRef": "#text",
        "$defs": {
            "ok": {"$dynamicAnchor": "text"},
            "a/b~c%d": {
                "$id": "https://example.com/escaped",
                "$dynamicAnchor": "text",
                "pattern": EXHAUSTING
            }
        }
    });
    assert_exhausted(&schema, &json!(long()));
}

#[test]
fn enters_every_recursive_reference_target() {
    let tree = json!({
        "$schema": DRAFT2019,
        "$id": "https://example.com/tree",
        "$recursiveAnchor": true,
        "properties": {"children": {"items": {"$recursiveRef": "#"}}}
    });
    let strict = json!({
        "$schema": DRAFT2019,
        "$id": "https://example.com/strict",
        "$recursiveAnchor": true,
        "$ref": "tree",
        "properties": {"name": {"pattern": EXHAUSTING}}
    });
    let resources = [("https://example.com/tree".to_owned(), &tree)];
    let instance = json!({"children": [{"name": long()}]});
    let result = exhausted_match(&strict, &resources, &instance);
    assert!(
        matches!(result, Err(RegexCheckError::Exhausted(_))),
        "{result:?}"
    );
}

#[test]
fn a_recursive_reference_follows_only_its_dynamic_scope() {
    // `tree` declares no `$recursiveAnchor`, so `$recursiveRef` stays on it
    // and never reaches `strict`'s pattern from a child.
    let tree = json!({
        "$schema": DRAFT2019,
        "$id": "https://example.com/tree",
        "properties": {"children": {"items": {"$recursiveRef": "#"}}}
    });
    let strict = json!({
        "$schema": DRAFT2019,
        "$id": "https://example.com/strict",
        "$recursiveAnchor": true,
        "$ref": "tree",
        "properties": {"name": {"pattern": EXHAUSTING}}
    });
    let resources = [("https://example.com/tree".to_owned(), &tree)];
    let instance = json!({"name": short(), "children": [{"name": long()}]});
    assert_eq!(exhausted_match(&strict, &resources, &instance), Ok(()));
}

#[test]
fn replays_a_subschema_again_under_another_dynamic_scope() {
    // `tree` is reached directly and through `strict`; only the second makes
    // its `$recursiveRef` land on `strict`. Either may be replayed first.
    let tree = json!({
        "$schema": DRAFT2019,
        "$id": "https://example.com/tree",
        "$recursiveAnchor": true,
        "properties": {"children": {"items": {"$recursiveRef": "#"}}}
    });
    let strict = json!({
        "$schema": DRAFT2019,
        "$id": "https://example.com/strict",
        "$recursiveAnchor": true,
        "$ref": "tree",
        "properties": {"name": {"pattern": EXHAUSTING}}
    });
    let resources = [
        ("https://example.com/tree".to_owned(), &tree),
        ("https://example.com/strict".to_owned(), &strict),
    ];
    let instance = json!({"children": [{"name": long()}]});
    for order in [["tree", "strict"], ["strict", "tree"]] {
        let root = json!({
            "$schema": DRAFT2019,
            "$id": "https://example.com/root",
            "allOf": [{"$ref": order[0]}, {"$ref": order[1]}]
        });
        let result = exhausted_match(&root, &resources, &instance);
        assert!(
            matches!(result, Err(RegexCheckError::Exhausted(_))),
            "{order:?}: {result:?}"
        );
    }
}

#[test]
fn ends_reference_cycles() {
    let schema = json!({
        "pattern": EXHAUSTING,
        "properties": {"next": {"$ref": "#"}},
        "allOf": [{"$ref": "#"}]
    });
    let nested = json!({"next": {"next": long()}});
    assert_exhausted(&schema, &nested);
    assert_eq!(
        exhausted(&schema, &json!({"next": {"next": short()}})),
        Ok(())
    );
}

#[test]
fn ends_reference_cycles_across_resources() {
    for (draft, anchor) in [(DRAFT2020, json!(null)), (DRAFT2019, json!(true))] {
        let mut a = json!({
            "$schema": draft,
            "$id": "https://example.com/a",
            "allOf": [{"$ref": "b"}],
            "pattern": EXHAUSTING
        });
        let mut b = json!({
            "$schema": draft,
            "$id": "https://example.com/b",
            "allOf": [{"$ref": "a"}]
        });
        if anchor.is_boolean() {
            a["$recursiveAnchor"] = anchor.clone();
            b["$recursiveAnchor"] = anchor;
        }
        let resources = [("https://example.com/b".to_owned(), &b)];
        assert_eq!(
            exhausted_match(&a, &resources, &json!(short())),
            Ok(()),
            "{draft}"
        );
        let result = exhausted_match(&a, &resources, &json!(long()));
        assert!(
            matches!(result, Err(RegexCheckError::Exhausted(_))),
            "{draft}: {result:?}"
        );
    }
}

#[test]
fn skips_schemas_without_regexes() {
    let schema = json!({"properties": {"a": {"type": "string"}}});
    assert_eq!(exhausted(&schema, &json!({"a": long()})), Ok(()));
}

#[test]
fn a_long_reference_chain_through_a_deep_instance_keeps_a_bounded_stack() {
    // 200 local references per level, 60 levels: the validator handles it on
    // a 2 MiB stack, and so must the replay.
    let mut defs = Map::new();
    for index in 0..199 {
        defs.insert(
            format!("s{index}"),
            json!({"$ref": format!("#/$defs/s{}", index + 1)}),
        );
    }
    defs.insert(
        "s199".to_owned(),
        json!({"pattern": "x", "items": {"$ref": "#"}}),
    );
    let schema = json!({
        "$schema": DRAFT2020,
        "anyOf": [true, {"$ref": "#/$defs/s0"}],
        "$defs": defs
    });
    let mut instance = json!("x");
    for _ in 0..60 {
        instance = json!([instance]);
    }
    let result = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            let validator = jsonschema::validator_for(&schema).unwrap();
            assert!(validator.is_valid(&instance));
            exhausted(&schema, &instance)
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(result, Ok(()));
}

#[test]
fn subschema_visits_spend_the_budget() {
    // One step for the root, then a visit and a match per item.
    let schema = json!({"items": {"pattern": "x"}});
    let instance = json!(vec!["x"; 10]);
    let guard = RegexGuard::new(&schema, &[]).unwrap();
    assert_eq!(guard.with_budget(21).check(&instance), Ok(()));
    let guard = RegexGuard::new(&schema, &[]).unwrap();
    let result = guard.with_budget(20).check(&instance);
    assert_unchecked(&result);
    assert!(result.unwrap_err().to_string().contains("20 steps"));
}

#[test]
fn regex_matches_spend_the_budget() {
    // A single visit, but a match per property name.
    let schema = json!({"patternProperties": {"^x": true}});
    let instance = Value::Object((0..50).map(|i| (format!("k{i}"), json!(1))).collect());
    let guard = RegexGuard::new(&schema, &[]).unwrap();
    assert_eq!(guard.with_budget(51).check(&instance), Ok(()));
    let guard = RegexGuard::new(&schema, &[]).unwrap();
    assert_unchecked(&guard.with_budget(50).check(&instance));
}

#[test]
fn resolving_references_spends_the_budget() {
    // Twenty dynamic references, each with twenty same-named candidates that
    // hold nothing to replay: resolving them is work all the same.
    let mut defs = Map::new();
    let mut refs = Vec::new();
    for index in 0..20 {
        let id = format!("https://example.com/r{index}");
        defs.insert(
            format!("r{index}"),
            json!({"$id": id, "$dynamicAnchor": "node"}),
        );
        refs.push(json!({"$dynamicRef": format!("{id}#node")}));
    }
    let schema = json!({"$schema": DRAFT2020, "pattern": "x", "allOf": refs, "$defs": defs});
    jsonschema::validator_for(&schema).unwrap();
    let guard = RegexGuard::new(&schema, &[]).unwrap().with_budget(100);
    assert_unchecked(&guard.check(&json!(0)));
}

#[test]
fn a_resolved_reference_is_replayed_for_every_subject() {
    let schema = json!({
        "$schema": DRAFT2020,
        "items": {"$ref": "#/$defs/text"},
        "$defs": {"text": {"not": pattern()}}
    });
    for instance in [
        json!(["b", long(), "b"]),
        json!([long(), "b"]),
        json!(["b", "b", long()]),
    ] {
        assert_exhausted(&schema, &instance);
    }
    assert_eq!(exhausted(&schema, &json!(["b", "b"])), Ok(()));
}

#[test]
fn work_past_the_budget_is_refused_before_it_is_queued() {
    let schema = json!({"items": {"pattern": "x"}});
    let instance = json!(vec!["x"; 100]);
    let guard = RegexGuard::new(&schema, &[]).unwrap().with_budget(50);
    assert_unchecked(&guard.check(&instance));
}

#[test]
fn an_object_past_the_budget_is_refused_before_it_is_classified() {
    let schema = json!({"additionalProperties": {"pattern": "x"}});
    let instance = Value::Object((0..100).map(|i| (format!("k{i}"), json!("x"))).collect());
    let guard = RegexGuard::new(&schema, &[]).unwrap().with_budget(50);
    assert_unchecked(&guard.check(&instance));
}

#[test]
fn an_unresolved_active_reference_fails_the_check() {
    let cases = [
        json!({"$ref": "#/$defs/missing", "properties": {"a": {"pattern": "x"}}}),
        json!({"anyOf": [true, {"$ref": "#/$defs/missing"}], "pattern": "x"}),
        json!({"$ref": "gts://elsewhere", "pattern": "x"}),
        json!({"$schema": DRAFT2020, "$dynamicRef": "#missing", "pattern": "x"}),
        json!({"$schema": DRAFT7, "properties": {"a": {"$ref": "#/definitions/missing"}}, "pattern": "x"}),
    ];
    for schema in cases {
        assert_unchecked(&exhausted(&schema, &json!({"a": 1})));
    }
}

#[test]
fn an_inactive_reference_is_never_resolved() {
    let cases = [
        json!({"$schema": DRAFT7, "$dynamicRef": "#missing", "pattern": "x"}),
        json!({"$schema": DRAFT2020, "$recursiveRef": "#missing", "pattern": "x"}),
        json!({
            "$schema": DRAFT7,
            "$ref": "#/definitions/ok",
            "definitions": {"ok": true},
            "allOf": [{"$ref": "#/definitions/missing"}],
            "pattern": "x"
        }),
        json!({"$schema": DRAFT2020, "then": {"$ref": "#/$defs/missing"}, "pattern": "x"}),
        json!({"$schema": DRAFT2020, "default": {"$ref": "#/$defs/missing"}, "pattern": "x"}),
    ];
    for schema in cases {
        assert_unaffected(&schema, &json!("x"));
    }
}

#[test]
fn documents_the_validator_cannot_register_fail_the_check() {
    let library = json!({"pattern": "x"});
    let resources = [("not a uri".to_owned(), &library)];
    assert!(RegexGuard::new(&pattern(), &resources).is_err());
    assert_unchecked(&exhausted_match(&pattern(), &resources, &json!("x")));

    let schema = json!({"$defs": {"bad": {"$id": "http://[::1", "pattern": "x"}}});
    assert_unchecked(&exhausted(&schema, &json!("x")));
}

#[test]
fn a_pattern_compiled_late_is_compiled_once_for_every_check() {
    let schema = json!({
        "$schema": DRAFT2020,
        "properties": {"a": {"not": {"$ref": "#/default/text"}}},
        "default": {"text": pattern()}
    });
    let guard = RegexGuard::new(&schema, &[]).unwrap();
    for _ in 0..2 {
        assert!(matches!(
            guard.check(&json!({"a": long()})),
            Err(RegexCheckError::Exhausted(_))
        ));
    }
    let prepared = guard.prepared.as_ref().unwrap();
    assert_eq!(
        prepared
            .late_compilations
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[test]
fn a_prepared_guard_checks_each_instance_afresh() {
    let schema = json!({"properties": {"a": {"not": {"pattern": EXHAUSTING}}}});
    let guard = RegexGuard::new(&schema, &[]).unwrap();
    for _ in 0..2 {
        assert_eq!(guard.check(&json!({"a": "b"})), Ok(()));
        assert!(matches!(
            guard.check(&json!({"a": long()})),
            Err(RegexCheckError::Exhausted(_))
        ));
    }
}

/// Accepted, temporary over-approximations: each rejects an instance the
/// validator accepts, and only when a regex match exhausts the engine.
mod conservative {
    use super::*;

    #[track_caller]
    fn assert_rejected_only_on_failure(schema: &Value, failing: &Value, passing: &Value) {
        let validator = jsonschema::validator_for(schema).unwrap();
        assert!(validator.is_valid(failing), "{schema} accepts {failing}");
        assert_exhausted(schema, failing);
        assert_eq!(exhausted(schema, passing), Ok(()), "{schema} on {passing}");
    }

    #[test]
    fn replays_any_of_branches_past_one_that_holds() {
        let schema = json!({"$schema": DRAFT2020, "anyOf": [true, pattern()]});
        assert_rejected_only_on_failure(&schema, &json!(long()), &json!("b"));
    }

    #[test]
    fn replays_one_of_branches_past_a_second_that_holds() {
        let schema = json!({"$schema": DRAFT2020, "not": {"oneOf": [true, true, pattern()]}});
        assert_rejected_only_on_failure(&schema, &json!(long()), &json!("b"));
    }

    #[test]
    fn replays_contains_past_an_item_that_matches() {
        let schema = json!({"$schema": DRAFT2020, "contains": pattern()});
        assert_rejected_only_on_failure(&schema, &json!([short(), long()]), &json!([short(), "b"]));
    }

    #[test]
    fn replays_both_branches_of_a_nontrivial_if() {
        let schema = json!({
            "$schema": DRAFT2020,
            "if": {"maxLength": 1},
            "then": {"pattern": EXHAUSTING}
        });
        assert_rejected_only_on_failure(&schema, &json!(long()), &json!("bb"));
    }

    #[test]
    fn replays_unevaluated_keywords_over_every_value() {
        let p = pattern();
        for draft in [DRAFT2019, DRAFT2020] {
            let schema = json!({
                "$schema": draft,
                "properties": {"a": true},
                "unevaluatedProperties": p
            });
            assert_rejected_only_on_failure(&schema, &json!({"a": long()}), &json!({"a": "b"}));
        }
        let schema = json!({"$schema": DRAFT2020, "prefixItems": [true], "unevaluatedItems": p});
        assert_rejected_only_on_failure(&schema, &json!([long()]), &json!(["b"]));
    }

    #[test]
    fn enters_every_same_named_dynamic_anchor() {
        let schema = json!({
            "$schema": DRAFT2020,
            "$dynamicRef": "#text",
            "$defs": {
                "ok": {"$dynamicAnchor": "text"},
                "unused": {
                    "$id": "https://example.com/unused",
                    "$dynamicAnchor": "text",
                    "pattern": EXHAUSTING
                }
            }
        });
        assert_rejected_only_on_failure(&schema, &json!(long()), &json!("b"));
    }
}
