#![allow(clippy::unwrap_used, clippy::expect_used)]

use gts::{
    CompilePayloadValidatorError, GtsRefValidation, GtsStore, PayloadValidationError, ResolvedType,
    StoreError,
};
use serde_json::{Value, json};

const TYPE_ID: &str = "gts.x.test.snapshot.base.v1~";
const CHILD_ID: &str = "gts.x.test.snapshot.base.v1~x.test.snapshot.child.v1~";
const DRAFT7: &str = "http://json-schema.org/draft-07/schema#";
const DRAFT2019: &str = "https://json-schema.org/draft/2019-09/schema";
const DRAFT2020: &str = "https://json-schema.org/draft/2020-12/schema";

fn schema() -> Value {
    json!({
        "$id": format!("gts://{TYPE_ID}"),
        "$schema": DRAFT7,
        "type": "object",
        "required": ["name"],
        "properties": {"name": {"type": "string", "minLength": 2}}
    })
}

/// Compiles `snapshot`'s payload validator and validates one payload with it.
fn validate(snapshot: &ResolvedType, payload: &Value) -> Result<(), PayloadValidationError> {
    snapshot
        .compile_payload_validator()
        .expect("snapshot compiles")
        .validate(payload)
}

fn resolve(document: &Value) -> ResolvedType {
    let mut store = GtsStore::new();
    store.register_schema(TYPE_ID, document).unwrap();
    store
        .validate_schema_with(TYPE_ID, GtsRefValidation::None)
        .unwrap()
}

#[test]
fn resolved_payload_checks_required_fields_and_constraints() {
    let snapshot = resolve(&schema());
    validate(&snapshot, &json!({"name": "Ada"})).unwrap();
    for invalid in [json!({}), json!({"name": 42}), json!({"name": "A"})] {
        assert!(
            matches!(
                validate(&snapshot, &invalid),
                Err(PayloadValidationError::Constraints(_))
            ),
            "{invalid}"
        );
    }
}

#[test]
fn resolved_payload_matches_store_validation_without_registry_checks() {
    let mut document = schema();
    document["properties"]["name"]["format"] = json!("email");
    let mut store = GtsStore::new();
    store.register_schema(TYPE_ID, &document).unwrap();
    let snapshot = store
        .validate_schema_with(TYPE_ID, GtsRefValidation::None)
        .unwrap();
    for payload in [
        json!({"name": "a@example.com"}),
        json!({"name": "invalid"}),
        json!({}),
    ] {
        assert_eq!(
            validate(&snapshot, &payload).is_ok(),
            store
                .validate_payload_with(TYPE_ID, &payload, GtsRefValidation::None)
                .is_ok(),
            "{payload}"
        );
    }
}

#[test]
fn resolved_payload_rejects_abstract_snapshot_even_without_schema_annotation() {
    let mut document = schema();
    document["x-gts-abstract"] = json!(true);
    let mut snapshot = resolve(&document);
    snapshot
        .schema
        .as_object_mut()
        .unwrap()
        .remove("x-gts-abstract");
    let error = snapshot.compile_payload_validator().unwrap_err();
    assert!(
        matches!(error, CompilePayloadValidatorError::Abstract(_)),
        "{error}"
    );
}

#[test]
fn resolved_payload_validates_derived_type_after_store_is_dropped() {
    let mut base = schema();
    base["x-gts-abstract"] = json!(true);
    base["properties"]["name"]["x-gts-type-ref"] = json!("/$id");
    let snapshot = {
        let mut store = GtsStore::new();
        store.register_schema(TYPE_ID, &base).unwrap();
        store
            .register_schema(
                CHILD_ID,
                &json!({
                    "$id": format!("gts://{CHILD_ID}"), "$schema": DRAFT7,
                    "allOf": [{"$ref": format!("gts://{TYPE_ID}")}]
                }),
            )
            .unwrap();
        store
            .validate_schema_with(CHILD_ID, GtsRefValidation::None)
            .unwrap()
    };
    assert!(!snapshot.is_abstract);
    validate(&snapshot, &json!({"name": CHILD_ID})).unwrap();
    assert!(validate(&snapshot, &json!({"name": TYPE_ID})).is_err());
}

#[test]
fn resolved_payload_enforces_reference_syntax_and_scope_without_target_lookup() {
    let mut document = schema();
    document["properties"]["name"]["x-gts-type-ref"] = json!("gts.x.test.target.item.v1~");
    let snapshot = resolve(&document);
    validate(&snapshot, &json!({"name": "gts.x.test.target.item.v1~"})).unwrap();
    for invalid in ["not-a-gts-id", TYPE_ID] {
        assert!(
            matches!(
                validate(&snapshot, &json!({"name": invalid})),
                Err(PayloadValidationError::References(_))
            ),
            "{invalid}"
        );
    }
}

#[test]
fn resolved_payload_reference_checks_follow_combinator_semantics() {
    let mut document = schema();
    document["properties"]["name"] = json!({"type": "string", "not": {"x-gts-type-ref": "/$id"}});
    let snapshot = resolve(&document);
    validate(&snapshot, &json!({"name": "ordinary text"})).unwrap();
    assert!(validate(&snapshot, &json!({"name": TYPE_ID})).is_err());
}

#[test]
fn resolved_payload_enforces_pattern_reference_coverage() {
    let mut document = schema();
    document["properties"]["name"]["x-gts-pattern-ref"] = json!("gts.x.test.target.*");
    let snapshot = resolve(&document);
    validate(&snapshot, &json!({"name": "gts.x.test.target.item.v1~"})).unwrap();
    assert!(validate(&snapshot, &json!({"name": TYPE_ID})).is_err());
}

#[test]
fn resolved_payload_asserts_gts_formats_in_every_supported_dialect() {
    for dialect in [DRAFT7, DRAFT2019, DRAFT2020] {
        let mut document = schema();
        document["$schema"] = json!(dialect);
        document["properties"]["name"]["format"] = json!("email");
        let snapshot = resolve(&document);
        validate(&snapshot, &json!({"name": "a@example.com"})).unwrap();
        assert!(
            validate(&snapshot, &json!({"name": "invalid"})).is_err(),
            "{dialect}"
        );
    }
}

#[test]
fn resolved_payload_preserves_dialect_specific_keywords() {
    let mut document = schema();
    document["$schema"] = json!(DRAFT2020);
    document["properties"]["name"] =
        json!({"type": "array", "prefixItems": [{"type": "integer"}], "items": false});
    let snapshot = resolve(&document);
    validate(&snapshot, &json!({"name": [42]})).unwrap();
    for invalid in [json!({"name": ["42"]}), json!({"name": [42, 43]})] {
        assert!(validate(&snapshot, &invalid).is_err(), "{invalid}");
    }
}

#[test]
fn resolved_payload_keeps_schema_errors_distinct_from_success() {
    let mut snapshot = resolve(&schema());
    snapshot.schema["properties"]["name"]["type"] = json!("not-a-json-schema-type");
    let error = snapshot.compile_payload_validator().unwrap_err();
    assert!(
        matches!(error, CompilePayloadValidatorError::InvalidSchema { .. }),
        "{error}"
    );
}

#[test]
fn resolved_payload_rejects_unavailable_external_schema_resources() {
    let mut snapshot = resolve(&schema());
    snapshot.schema["properties"]["name"] = json!({"$ref": "gts://gts.x.test.missing.schema.v1~"});
    let error = snapshot.compile_payload_validator().unwrap_err();
    assert!(
        matches!(error, CompilePayloadValidatorError::InvalidSchema { .. }),
        "{error}"
    );
}

#[test]
fn resolved_payload_supports_recursion_contained_in_the_snapshot() {
    let document = json!({
        "$id": format!("gts://{TYPE_ID}"), "$schema": DRAFT7,
        "type": "object", "required": ["value"],
        "properties": {"value": {"type": "integer"}, "child": {"$ref": "#"}}
    });
    let snapshot = resolve(&document);
    validate(&snapshot, &json!({"value": 1, "child": {"value": 2}})).unwrap();
    assert!(validate(&snapshot, &json!({"value": 1, "child": {"value": "wrong"}})).is_err());
}

#[test]
fn resolved_payload_does_not_classify_an_unexplained_rejection() {
    // Explaining a rejection of this recursive root is not affordable, so the
    // reference violation behind it must not be reported as a constraint one.
    let document = json!({
        "$id": format!("gts://{TYPE_ID}"), "$schema": DRAFT7,
        "$ref": "#/definitions/node",
        "definitions": {"node": {"type": "object", "properties": {
            "id": {"type": "string", "x-gts-type-ref": "gts.x.test.target.item.v1~"},
            "child": {"$ref": "#/definitions/node"}
        }}}
    });
    let snapshot = resolve(&document);
    validate(&snapshot, &json!({"id": "gts.x.test.target.item.v1~"})).unwrap();
    let error = validate(&snapshot, &json!({"id": "bad"})).unwrap_err();
    assert!(
        matches!(error, PayloadValidationError::Unexplained(_)),
        "{error:?}"
    );
}

#[test]
fn resolved_payload_does_not_insert_defaults_or_change_inputs() {
    let mut document = schema();
    document["properties"]["name"]["default"] = json!("Ada");
    let snapshot = resolve(&document);
    let payload = json!({});
    let schema_before = snapshot.schema.clone();
    assert!(validate(&snapshot, &payload).is_err());
    assert_eq!(payload, json!({}));
    assert_eq!(snapshot.schema, schema_before);
}

#[test]
fn resolved_payload_rejects_patterns_outside_the_gts_regex_profile() {
    let mut snapshot = resolve(&schema());
    snapshot.schema["properties"]["name"]["pattern"] = json!("a(?=b)");
    let error = snapshot.compile_payload_validator().unwrap_err();
    assert!(
        matches!(
            &error,
            CompilePayloadValidatorError::InvalidSchema { reason, .. }
                if reason.contains("unsupported regular expression")
        ),
        "{error}"
    );
}

/// Asserts that a snapshot of `type_id` accepts exactly what the store accepts.
fn assert_matches_store(documents: &[(&str, Value)], type_id: &str, payloads: &[Value]) {
    let mut store = GtsStore::new();
    for (id, document) in documents {
        store.register_schema(id, document).unwrap();
    }
    let snapshot = store
        .validate_schema_with(type_id, GtsRefValidation::None)
        .unwrap();
    let validator = snapshot.compile_payload_validator().unwrap();
    for payload in payloads {
        let expected = store
            .validate_payload_with(type_id, payload, GtsRefValidation::None)
            .map_err(|error| error.to_string());
        let actual = validator
            .validate(payload)
            .map_err(|error| error.to_string());
        assert_eq!(
            actual.is_ok(),
            expected.is_ok(),
            "{type_id} {payload}: snapshot {actual:?}, store {expected:?}, schema {}",
            snapshot.schema
        );
    }
}

fn with_properties(dialect: &str, definitions: &Value, properties: &Value) -> Value {
    let definitions_keyword = if dialect == DRAFT7 {
        "definitions"
    } else {
        "$defs"
    };
    json!({
        "$id": format!("gts://{TYPE_ID}"), "$schema": dialect, "type": "object",
        definitions_keyword: definitions, "properties": properties
    })
}

#[test]
fn resolved_payload_applies_ref_siblings_as_each_dialect_does() {
    for dialect in [DRAFT7, DRAFT2019, DRAFT2020] {
        let pointer = if dialect == DRAFT7 {
            "#/definitions"
        } else {
            "#/$defs"
        };
        let document = with_properties(
            dialect,
            &json!({"s": {"type": "string"}, "open": true}),
            &json!({
                "n": {"$ref": format!("{pointer}/s"), "minLength": 5},
                "r": {"$ref": format!("{pointer}/s"), "x-gts-type-ref": "gts.x.test.target.item.v1~"},
                "b": {"$ref": format!("{pointer}/open"), "minLength": 5}
            }),
        );
        assert_matches_store(
            &[(TYPE_ID, document)],
            TYPE_ID,
            &[
                json!({"n": "ab"}),
                json!({"n": "abcdef"}),
                json!({"r": "text"}),
                json!({"r": "gts.x.test.target.item.v1~"}),
                json!({"b": "ab"}),
            ],
        );
    }
}

#[test]
fn resolved_payload_applies_siblings_of_a_type_ref_as_each_dialect_does() {
    let other = "gts.x.test.snapshot.other.v1~";
    for dialect in [DRAFT7, DRAFT2019, DRAFT2020] {
        let referenced = json!({
            "$id": format!("gts://{other}"), "$schema": dialect,
            "type": "object", "properties": {"v": {"type": "integer"}}
        });
        let document = with_properties(
            dialect,
            &json!({}),
            &json!({"x": {"$ref": format!("gts://{other}"), "required": ["v"]}}),
        );
        assert_matches_store(
            &[(other, referenced), (TYPE_ID, document)],
            TYPE_ID,
            &[
                json!({"x": {}}),
                json!({"x": {"v": 1}}),
                json!({"x": {"v": "1"}}),
            ],
        );
    }
}

#[test]
fn resolved_payload_keeps_the_dialect_of_a_root_ref() {
    for dialect in [DRAFT7, DRAFT2019, DRAFT2020] {
        let (keyword, pointer) = if dialect == DRAFT7 {
            ("definitions", "#/definitions/t")
        } else {
            ("$defs", "#/$defs/t")
        };
        let document = json!({
            "$id": format!("gts://{TYPE_ID}"), "$schema": dialect,
            keyword: {"t": {"type": "object", "properties": {"v": {"type": "integer"}}}},
            "$ref": pointer, "required": ["v"]
        });
        assert_matches_store(
            &[(TYPE_ID, document)],
            TYPE_ID,
            &[json!({}), json!({"v": 1}), json!({"v": "1"})],
        );
    }
}

#[test]
fn resolved_payload_lets_unevaluated_siblings_see_the_ref_target() {
    for dialect in [DRAFT2019, DRAFT2020] {
        let document = with_properties(
            dialect,
            &json!({
                "t": {"properties": {"a": true}},
                "closed": {"properties": {"a": true}, "unevaluatedProperties": false}
            }),
            &json!({
                "n": {
                    "$ref": "#/$defs/t",
                    "allOf": [{"properties": {"b": true}}],
                    "unevaluatedProperties": false
                },
                "k": {"$ref": "#/$defs/closed", "properties": {"z": true}}
            }),
        );
        assert_matches_store(
            &[(TYPE_ID, document)],
            TYPE_ID,
            &[
                json!({"n": {"a": 1}}),
                json!({"n": {"b": 1}}),
                json!({"n": {"c": 1}}),
                json!({"k": {"a": 1}}),
                json!({"k": {"z": 1}}),
            ],
        );
    }
}

#[test]
fn resolved_payload_lets_unevaluated_items_see_the_ref_target() {
    let document = with_properties(
        DRAFT2020,
        &json!({"tuple": {"prefixItems": [true]}}),
        &json!({"i": {"$ref": "#/$defs/tuple", "unevaluatedItems": false}}),
    );
    assert_matches_store(
        &[(TYPE_ID, document)],
        TYPE_ID,
        &[json!({"i": [1]}), json!({"i": [1, 2]})],
    );
}

#[test]
fn resolved_payload_reads_a_draft07_root_ref_under_draft07() {
    // Tuple `items` means one thing in Draft-07 and another in 2020-12, so a
    // lost `$schema` would change the verdict.
    let tuple = json!({
        "$id": format!("gts://{TYPE_ID}"), "$schema": DRAFT7,
        "definitions": {"t": {"type": "array", "items": [{"type": "integer"}], "additionalItems": false}},
        "$ref": "#/definitions/t", "maxItems": 0
    });
    let open = json!({
        "$id": format!("gts://{TYPE_ID}"), "$schema": DRAFT7,
        "definitions": {"t": true},
        "$ref": "#/definitions/t", "type": "object"
    });
    for (document, payloads) in [
        (tuple, vec![json!([1]), json!(["1"]), json!([1, 2])]),
        (open, vec![json!({}), json!("text")]),
    ] {
        assert_matches_store(&[(TYPE_ID, document)], TYPE_ID, &payloads);
    }
}

#[test]
fn compiled_payload_validator_outlives_and_ignores_its_snapshot() {
    let mut snapshot = resolve(&schema());
    let validator = snapshot.compile_payload_validator().unwrap();
    snapshot.schema["properties"]["name"]["minLength"] = json!(10);
    drop(snapshot);
    let shared = validator.clone();
    let worker = std::thread::spawn(move || shared.validate(&json!({"name": "Ada"})));
    worker.join().unwrap().unwrap();
    assert_eq!(validator.type_id().as_ref(), TYPE_ID);
    for invalid in [json!({}), json!({"name": "A"})] {
        assert!(
            matches!(
                validator.validate(&invalid),
                Err(PayloadValidationError::Constraints(_))
            ),
            "{invalid}"
        );
    }
}

#[test]
fn compiling_a_payload_validator_rejects_an_abstract_type() {
    let mut document = schema();
    document["x-gts-abstract"] = json!(true);
    let error = resolve(&document).compile_payload_validator().unwrap_err();
    assert!(
        matches!(error, CompilePayloadValidatorError::Abstract(_)),
        "{error}"
    );
    assert!(error.to_string().contains("abstract"), "{error}");
}

#[test]
fn payload_validator_errors_convert_into_store_errors() {
    fn check(snapshot: &ResolvedType, payload: &Value) -> Result<(), StoreError> {
        snapshot.compile_payload_validator()?.validate(payload)?;
        Ok(())
    }
    let mut abstract_type = schema();
    abstract_type["x-gts-abstract"] = json!(true);
    let snapshot = resolve(&schema());

    check(&snapshot, &json!({"name": "Ada"})).unwrap();
    for error in [
        check(&resolve(&abstract_type), &json!({"name": "Ada"})).unwrap_err(),
        check(&snapshot, &json!({})).unwrap_err(),
    ] {
        assert!(matches!(error, StoreError::ValidationError(_)), "{error}");
    }
}

#[test]
fn resolved_payload_follows_a_draft07_recursive_root_ref_into_its_definitions() {
    // The kept root pointer reaches into `definitions`, a sibling Draft-07
    // otherwise ignores, so the type reference there must still be inlined.
    let other = "gts.x.test.snapshot.other.v1~";
    let referenced = json!({"$id": format!("gts://{other}"), "$schema": DRAFT7, "type": "string"});
    let document = json!({
        "$id": format!("gts://{TYPE_ID}"), "$schema": DRAFT7,
        "$ref": "#/definitions/node",
        "definitions": {"node": {"type": "object", "properties": {
            "child": {"$ref": "#/definitions/node"},
            "base": {"$ref": format!("gts://{other}")}
        }}}
    });
    assert_matches_store(
        &[(other, referenced), (TYPE_ID, document)],
        TYPE_ID,
        &[
            json!({"base": "text"}),
            json!({"base": 1}),
            json!({"child": {"base": "text"}}),
            json!({"child": {"child": {"base": 1}}}),
        ],
    );
}
