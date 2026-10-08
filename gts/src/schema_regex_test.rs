use serde_json::json;

use super::*;

const UNSUPPORTED: &str = "a(?=b)";

fn check_one(schema: &Value) -> Result<(), String> {
    let uri = schema
        .get("$id")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_BASE_URI);
    let registry = Registry::new()
        .add(uri, schema)
        .and_then(jsonschema::RegistryBuilder::prepare)
        .expect("registry");
    check(&registry, &[(uri, schema)])
}

fn with_dialect(dialect: &str, body: &Value) -> Value {
    let mut schema = body.clone();
    schema["$schema"] = json!(dialect);
    schema
}

const DRAFT_07: &str = "http://json-schema.org/draft-07/schema#";
const DRAFT_2019_09: &str = "https://json-schema.org/draft/2019-09/schema";
const DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";

#[test]
fn checks_every_dialect_defined_position() {
    let unsupported = json!({"pattern": UNSUPPORTED});
    let common = [
        json!({"not": {"not": unsupported}}),
        json!({"if": false, "then": unsupported}),
        json!({"if": true, "else": unsupported}),
        json!({"anyOf": [true, unsupported]}),
        json!({"oneOf": [true, {"not": unsupported}]}),
        json!({"allOf": [unsupported]}),
        json!({"properties": {"a": unsupported}}),
        json!({"patternProperties": {UNSUPPORTED: true}}),
        json!({"patternProperties": {"^a$": unsupported}}),
        json!({"propertyNames": unsupported}),
        json!({"additionalProperties": unsupported}),
        json!({"contains": unsupported}),
        json!({"items": unsupported}),
        json!({"x-gts-traits-schema": {"properties": {"v": unsupported}}}),
        json!({
            "x-gts-traits-schema": {"$ref": "#/custom/traits"},
            "custom": {"traits": {"properties": {"v": unsupported}}}
        }),
        json!({"anyOf": [true, {"$ref": "#/custom/unused"}], "custom": {"unused": unsupported}}),
    ];
    for dialect in [DRAFT_07, DRAFT_2019_09, DRAFT_2020_12] {
        for body in &common {
            let schema = with_dialect(dialect, body);
            assert!(check_one(&schema).is_err(), "{schema}");
        }
    }
    for body in [
        json!({"definitions": {"x": unsupported}}),
        json!({"dependencies": {"x": unsupported}}),
        json!({"items": [true, unsupported]}),
        json!({"additionalItems": unsupported}),
    ] {
        let schema = with_dialect(DRAFT_07, &body);
        assert!(check_one(&schema).is_err(), "{schema}");
    }
    for dialect in [DRAFT_2019_09, DRAFT_2020_12] {
        for body in [
            json!({"$defs": {"x": unsupported}}),
            json!({"dependentSchemas": {"x": unsupported}}),
            json!({"unevaluatedProperties": unsupported}),
            json!({"unevaluatedItems": unsupported}),
            json!({"contentSchema": unsupported}),
        ] {
            let schema = with_dialect(dialect, &body);
            assert!(check_one(&schema).is_err(), "{schema}");
        }
    }
    let schema = with_dialect(DRAFT_2020_12, &json!({"prefixItems": [true, unsupported]}));
    assert!(check_one(&schema).is_err(), "{schema}");
}

#[test]
fn leaves_literal_data_and_unknown_keywords_alone() {
    let literal = json!({"pattern": UNSUPPORTED});
    let schema = json!({
        "default": literal,
        "examples": [literal],
        "custom": literal,
        "properties": {
            "pattern": {"type": "string"},
            "c": {"const": literal},
            "e": {"enum": [literal]}
        }
    });
    for dialect in [DRAFT_07, DRAFT_2019_09, DRAFT_2020_12] {
        assert_eq!(check_one(&with_dialect(dialect, &schema)), Ok(()));
    }
    // Keywords other dialects define.
    for body in [
        json!({"prefixItems": [literal]}),
        json!({"dependentSchemas": {"x": literal}}),
        json!({"$defs": {"x": literal}}),
        json!({"unevaluatedProperties": literal}),
    ] {
        assert_eq!(check_one(&with_dialect(DRAFT_07, &body)), Ok(()), "{body}");
    }
    for dialect in [DRAFT_2019_09, DRAFT_2020_12] {
        for body in [
            json!({"definitions": {"x": literal}}),
            json!({"dependencies": {"x": literal}}),
        ] {
            assert_eq!(check_one(&with_dialect(dialect, &body)), Ok(()), "{body}");
        }
    }
    assert_eq!(
        check_one(&with_dialect(
            DRAFT_2019_09,
            &json!({"prefixItems": [literal]})
        )),
        Ok(())
    );
    assert_eq!(
        check_one(&with_dialect(
            DRAFT_2020_12,
            &json!({"additionalItems": literal})
        )),
        Ok(())
    );
    // `x-gts-traits-schema` is a schema only at the document root.
    let nested = json!({"properties": {"a": {"x-gts-traits-schema": literal}}});
    assert_eq!(check_one(&nested), Ok(()));
}

#[test]
fn ignores_the_siblings_of_a_draft_07_ref() {
    let schema = with_dialect(
        DRAFT_07,
        &json!({
            "$ref": "#/definitions/x",
            "pattern": UNSUPPORTED,
            "properties": {"a": {"pattern": UNSUPPORTED}},
            "definitions": {"x": true}
        }),
    );
    assert_eq!(check_one(&schema), Ok(()));
    // Later dialects apply them.
    let schema = with_dialect(
        DRAFT_2020_12,
        &json!({"$ref": "#/$defs/x", "pattern": UNSUPPORTED, "$defs": {"x": true}}),
    );
    assert!(check_one(&schema).is_err());
}

#[test]
fn follows_references_through_embedded_resources_and_cycles() {
    let schema = with_dialect(
        DRAFT_2020_12,
        &json!({
            "$id": "https://example.com/root",
            "properties": {"a": {"$ref": "#/$defs/inner"}, "self": {"$ref": "#"}},
            "$defs": {
                "inner": {
                    "$id": "dir/inner",
                    "not": {"$ref": "#/custom/text"},
                    "custom": {"text": {"pattern": UNSUPPORTED}}
                }
            }
        }),
    );
    let error = check_one(&schema).expect_err("reached through the embedded resource");
    assert!(error.contains("unsupported regular expression"), "{error}");
}

#[test]
fn checks_every_document_given() {
    let target = json!({"$id": "gts://target", "anyOf": [true, {"pattern": UNSUPPORTED}]});
    let root = json!({"$id": "gts://root", "type": "object"});
    let registry = Registry::new()
        .extend([("gts://target", &target), ("gts://root", &root)])
        .and_then(jsonschema::RegistryBuilder::prepare)
        .expect("registry");
    assert!(
        check(
            &registry,
            &[("gts://root", &root), ("gts://target", &target)]
        )
        .is_err()
    );
    assert_eq!(check(&registry, &[("gts://root", &root)]), Ok(()));
}

#[test]
fn reports_the_location_and_a_bounded_excerpt() {
    let long = format!("{}(?=b)", "a".repeat(200));
    let schema = json!({"properties": {"a/b": {"pattern": long}}});
    let error = check_one(&schema).expect_err("unsupported");
    assert!(error.contains("#/properties/a~1b/pattern"), "{error}");
    assert!(
        error.contains(&format!("'{}...'", "a".repeat(80))),
        "{error}"
    );
}

#[test]
fn applies_a_relative_embedded_id_once_whichever_way_it_is_entered() {
    let inner = |pattern: &str| {
        json!({
            "$id": "dir/inner",
            "anyOf": [true, {"$ref": "#/custom/bad"}],
            "custom": {"bad": {"pattern": pattern}}
        })
    };
    for pattern in [r"abc\z", "abc$"] {
        let supported = pattern == "abc$";
        // Through the reference first, then structurally.
        let by_reference = with_dialect(
            DRAFT_2020_12,
            &json!({
                "$id": "gts://gts.x.review._.relative_resource.v1~",
                "$ref": "#/$defs/inner",
                "$defs": {"inner": inner(pattern)}
            }),
        );
        // Structurally first: `$defs` sorts before `properties`.
        let structurally = with_dialect(
            DRAFT_2020_12,
            &json!({
                "$id": "gts://gts.x.review._.relative_resource.v1~",
                "properties": {"a": {"$ref": "#/$defs/inner"}},
                "$defs": {"inner": inner(pattern)}
            }),
        );
        for schema in [by_reference, structurally] {
            assert_eq!(check_one(&schema).is_ok(), supported, "{schema}");
        }
    }
}

#[test]
fn follows_the_keywords_each_legacy_draft_defines() {
    let unsupported = json!({"pattern": UNSUPPORTED});
    let draft_04 = "http://json-schema.org/draft-04/schema#";
    let draft_06 = "http://json-schema.org/draft-06/schema#";
    for (dialect, body, checked) in [
        (draft_04, json!({"if": unsupported}), false),
        (draft_04, json!({"contains": unsupported}), false),
        (draft_04, json!({"propertyNames": unsupported}), false),
        (draft_06, json!({"if": unsupported}), false),
        (draft_06, json!({"then": unsupported}), false),
        (draft_06, json!({"contains": unsupported}), true),
        (draft_06, json!({"propertyNames": unsupported}), true),
        (DRAFT_07, json!({"if": unsupported}), true),
    ] {
        let schema = with_dialect(dialect, &body);
        assert_eq!(check_one(&schema).is_err(), checked, "{schema}");
    }
}
