//! Unit tests for JSON Schema dialect selection and reference boundaries.

use super::*;
use serde_json::json;
use std::collections::HashMap;

struct Schemas(HashMap<&'static str, Value>);

impl SchemaProvider for Schemas {
    fn schema_content(&self, type_id: &str) -> Option<&Value> {
        self.0.get(type_id)
    }
}

fn no_schemas() -> Schemas {
    Schemas(HashMap::new())
}

#[test]
fn supported_dialects_accept_equivalent_spellings() {
    for (uri, expected) in [
        ("http://json-schema.org/draft-07/schema#", Draft::Draft7),
        ("https://json-schema.org/draft-07/schema", Draft::Draft7),
        (
            "https://json-schema.org/draft/2019-09/schema",
            Draft::Draft201909,
        ),
        (
            "http://json-schema.org/draft/2019-09/schema#",
            Draft::Draft201909,
        ),
        (
            "https://json-schema.org/draft/2020-12/schema",
            Draft::Draft202012,
        ),
        (
            "http://json-schema.org/draft/2020-12/schema#",
            Draft::Draft202012,
        ),
    ] {
        assert_eq!(
            document_dialect(&json!({"$schema": uri})),
            Ok(expected),
            "{uri}"
        );
    }
}

#[test]
fn an_undeclared_dialect_is_refused() {
    let error = document_dialect(&json!({"type": "object"})).unwrap_err();
    assert!(error.contains("'$schema'"), "{error}");
}

#[test]
fn pre_draft_07_dialects_are_refused_as_too_old() {
    for uri in [
        "http://json-schema.org/draft-06/schema#",
        "http://json-schema.org/draft-04/schema#",
        "https://json-schema.org/draft-03/schema",
    ] {
        let error = document_dialect(&json!({"$schema": uri})).unwrap_err();
        assert!(
            error.contains("minimum supported dialect"),
            "{uri}: {error}"
        );
    }
}

#[test]
fn unrecognized_dialects_are_refused() {
    for declared in [
        json!("https://example.invalid/not-a-json-schema-dialect"),
        json!("https://json-schema.org/draft/2020-21/schema"),
        json!("https://json-schema.org/draft/2020-12/schema##"),
        json!("ftp://json-schema.org/draft-07/schema#"),
        json!(""),
    ] {
        let error = document_dialect(&json!({"$schema": declared})).unwrap_err();
        assert!(
            error.contains("not a supported dialect"),
            "{declared}: {error}"
        );
    }
    let error = document_dialect(&json!({"$schema": 7})).unwrap_err();
    assert!(error.contains("must be a string"), "{error}");
}

#[test]
fn a_local_ref_into_an_embedded_resource_of_another_dialect_is_refused() {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "properties": {"legacy": {"$ref": "#/$defs/legacy"}},
        "$defs": {
            "legacy": {
                "$id": "legacy",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "string"
            }
        }
    });
    let error = check_references(&schema, &no_schemas()).unwrap_err();
    assert!(error.contains("'properties/legacy/$ref'"), "{error}");
    assert!(error.contains("Draft 2020-12"), "{error}");
    assert!(error.contains("Draft-07"), "{error}");
}

#[test]
fn a_local_ref_inside_an_embedded_resource_resolves_from_that_resource() {
    // `#` inside `legacy` names `legacy`, not the document, so none of these
    // references leave Draft-07 - even though the document root is 2020-12
    // and also has a `definitions/name` for the pointer to land on.
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "definitions": {"name": {"type": "string"}},
        "$defs": {
            "legacy": {
                "$id": "legacy",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {
                    "next": {"$ref": "#"},
                    "name": {"$ref": "#/definitions/name"},
                    "inner": {"$ref": "#/definitions/inner"}
                },
                "definitions": {
                    "name": {"type": "string"},
                    "inner": {
                        "$id": "inner",
                        "properties": {"again": {"$ref": "#"}}
                    }
                }
            }
        }
    });
    assert_eq!(check_references(&schema, &no_schemas()), Ok(()));
}

#[test]
fn a_local_ref_inside_an_embedded_resource_is_still_checked() {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": {
            "legacy": {
                "$id": "legacy",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {"modern": {"$ref": "#/definitions/modern"}},
                "definitions": {
                    "modern": {
                        "$id": "modern",
                        "$schema": "https://json-schema.org/draft/2020-12/schema"
                    }
                }
            }
        }
    });
    let error = check_references(&schema, &no_schemas()).unwrap_err();
    assert!(
        error.contains("'$defs/legacy/properties/modern/$ref'"),
        "{error}"
    );
    assert!(error.contains("read under Draft-07"), "{error}");
}

#[test]
fn a_draft_07_id_beside_a_ref_starts_no_resource() {
    // Draft-07 ignores every sibling of `$ref`, `$id` included, so this
    // `#` still names the 2020-12 document root.
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "properties": {
            "legacy": {
                "$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {"self": {"$id": "ignored", "$ref": "#"}}
            },
            "modern": {"$id": "modern", "$ref": "#"}
        }
    });
    let error = check_references(&schema, &no_schemas()).unwrap_err();
    assert!(
        error.contains("'properties/legacy/properties/self/$ref'"),
        "{error}"
    );
}

#[test]
fn a_local_ref_within_one_dialect_is_accepted() {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "properties": {"name": {"$ref": "#/$defs/name"}},
        "$defs": {"name": {"type": "string"}}
    });
    assert_eq!(check_references(&schema, &no_schemas()), Ok(()));
}

#[test]
fn an_unreferenced_embedded_resource_is_left_to_the_subschema_check() {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": {
            "legacy": {
                "$id": "legacy",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "string"
            }
        }
    });
    assert_eq!(check_references(&schema, &no_schemas()), Ok(()));
    assert!(check_subschemas(&schema, Draft::Draft202012).is_err());
}

#[test]
fn a_gts_ref_must_target_the_same_dialect() {
    let schemas = Schemas(HashMap::from([(
        "gts.x.dialect._.base.v1~",
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {
                "legacy": {
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "string"
                }
            }
        }),
    )]));
    let referrer = |dialect: &str, reference: &str| json!({"$schema": dialect, "allOf": [{"$ref": reference}]});
    let draft_07 = "http://json-schema.org/draft-07/schema#";
    let draft_2020 = "https://json-schema.org/draft/2020-12/schema";

    let error = check_references(
        &referrer(draft_07, "gts://gts.x.dialect._.base.v1~"),
        &schemas,
    )
    .unwrap_err();
    assert!(error.contains("'allOf[0]/$ref'"), "{error}");
    assert_eq!(
        check_references(
            &referrer(draft_2020, "gts://gts.x.dialect._.base.v1~"),
            &schemas
        ),
        Ok(())
    );
    assert!(
        check_references(
            &referrer(draft_2020, "gts://gts.x.dialect._.base.v1~#/$defs/legacy"),
            &schemas
        )
        .is_err(),
        "a fragment into an embedded resource is read in that resource's dialect"
    );
}

#[test]
fn an_unresolved_ref_is_left_to_resolution() {
    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "properties": {
            "missing": {"$ref": "#/definitions/missing"},
            "absent": {"$ref": "gts://gts.x.dialect._.absent.v1~"}
        }
    });
    assert_eq!(check_references(&schema, &no_schemas()), Ok(()));
}

#[test]
fn a_ref_inside_the_trait_schema_is_checked() {
    let schemas = Schemas(HashMap::from([(
        "gts.x.dialect._.traits.v1~",
        json!({"$schema": "https://json-schema.org/draft/2020-12/schema", "type": "object"}),
    )]));
    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "x-gts-traits-schema": {"$ref": "gts://gts.x.dialect._.traits.v1~"}
    });
    let error = check_references(&schema, &schemas).unwrap_err();
    assert!(error.contains("'x-gts-traits-schema/$ref'"), "{error}");
}

#[test]
fn a_subschema_of_another_dialect_is_refused() {
    let draft_07 = json!("http://json-schema.org/draft-07/schema#");
    for (location, schema) in [
        (
            "x-gts-traits-schema",
            json!({"x-gts-traits-schema": {
                "$id": "https://example.com/gts/legacy-traits",
                "$schema": draft_07,
                "type": "object"
            }}),
        ),
        (
            "x-gts-traits-schema/properties/limits",
            json!({"x-gts-traits-schema": {
                "properties": {"limits": {"$schema": draft_07}}
            }}),
        ),
        (
            "$defs/legacy",
            json!({"$defs": {"legacy": {"$id": "legacy", "$schema": draft_07}}}),
        ),
        (
            "allOf[0]/properties/legacy",
            json!({"allOf": [{"properties": {"legacy": {"$schema": draft_07}}}]}),
        ),
    ] {
        let mut schema = schema;
        schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
        let error = check_subschemas(&schema, Draft::Draft202012).unwrap_err();
        assert!(error.contains(&format!("'{location}'")), "{error}");
        assert!(error.contains("declares Draft-07"), "{error}");
        assert!(error.contains("read under Draft 2020-12"), "{error}");
    }

    let unrecognized = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "properties": {"custom": {"$schema": "https://example.invalid/meta"}}
    });
    let error = check_subschemas(&unrecognized, Draft::Draft7).unwrap_err();
    assert!(error.contains("'properties/custom'"), "{error}");
}

#[test]
fn a_subschema_restating_the_dialect_is_accepted() {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "properties": {
            "restated": {"$schema": "http://json-schema.org/draft/2020-12/schema#"},
            "resource": {"$id": "resource", "type": "string"},
            // Data, not a subschema: its `$schema` selects nothing.
            "literal": {"const": {"$schema": "http://json-schema.org/draft-07/schema#"}}
        },
        "x-gts-traits-schema": true,
        "x-gts-traits": {"$schema": "http://json-schema.org/draft-07/schema#"}
    });
    assert_eq!(check_subschemas(&schema, Draft::Draft202012), Ok(()));
}
