#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::*;
    use serde_json::json;

    #[test]
    fn the_keyword_applies_only_in_schema_positions() {
        // Keyword-looking names in data stay ordinary JSON.
        let schema = json!({
            "type": "object",
            "properties": {
                "x-gts-ref": {
                    "type": "string",
                    "x-gts-ref": "gts.x.test.refs.target.v1~"
                },
                "payload": {
                    "const": {"x-gts-ref": "literal const data"},
                    "default": {"x-gts-ref": "literal default data"},
                    "examples": [{"x-gts-ref": "literal example data"}]
                }
            }
        });
        let validator = XGtsRefValidator::new();

        assert!(
            validator
                .validate_instance(
                    &json!({"payload": {"x-gts-ref": "literal const data"}}),
                    &schema,
                    ""
                )
                .is_empty(),
            "literal data must not be read as a reference"
        );

        let errors = validator.validate_instance(
            &json!({"x-gts-ref": "gts.x.other._.target.v1~x.v._.bad.v1"}),
            &schema,
            "",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].field_path, "/x-gts-ref");
    }

    #[test]
    fn applicability_honours_the_declared_dialect() {
        // Applicator semantics follow the selected draft.
        let target = "gts.x.test.refs.target.v1~";
        let bad = json!("gts.x.other._.target.v1~x.v._.bad.v1");
        let validator = XGtsRefValidator::new();

        let draft7 = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "dependencies": {
                "name": {"properties": {"ref": {"type": "string", "x-gts-ref": target}}}
            }
        });
        let errors = validator.validate_instance(&json!({"name": "n", "ref": bad}), &draft7, "");
        assert_eq!(
            errors.len(),
            1,
            "draft-07 applies `dependencies`: {errors:?}"
        );

        let draft2020 = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "array",
            "prefixItems": [{"type": "string", "x-gts-ref": target}]
        });
        let errors = validator.validate_instance(&json!([bad]), &draft2020, "");
        assert_eq!(
            errors.len(),
            1,
            "draft 2020-12 applies `prefixItems`: {errors:?}"
        );
        assert_eq!(errors[0].field_path, "/0");
    }

    fn refs_of(value: &str, pattern: &str) -> Vec<XGtsRefValidationError> {
        XGtsRefValidator::new().validate_instance(
            &json!(value),
            &json!({"type": "string", "x-gts-ref": pattern}),
            "",
        )
    }

    #[test]
    fn a_value_is_matched_against_its_declared_pattern() {
        for pattern in ["gts.x.core.events.topic.v1~", "gts.*", "gts.x.core.*"] {
            assert!(
                refs_of("gts.x.core.events.topic.v1~", pattern).is_empty(),
                "expected '{pattern}' to match"
            );
        }

        let errors = refs_of("gts.x.core.events.topic.v1~", "gts.y.core.*");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].reason.contains("does not match pattern"));
    }

    #[test]
    fn test_validate_schema_with_x_gts_ref() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "topic_id": {
                    "type": "string",
                    "x-gts-ref": "gts.x.core.events.topic.*"
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_instance_with_x_gts_ref() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "topic_id": {
                    "type": "string",
                    "x-gts-ref": "gts.x.core.events.topic.*"
                }
            }
        });

        let instance = json!({
            "topic_id": "gts.x.core.events.topic.v1~"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_schema_relative_ref_resolving_to_wildcard() {
        // Relative declarations accept the same wildcards as literals.
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "anchor": {
                    "type": "string",
                    "x-gts-ref": "gts.x.core.events.topic.*"
                },
                "relative": {
                    "type": "string",
                    "x-gts-ref": "/properties/anchor"
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(
            errors.is_empty(),
            "relative ref resolving to a wildcard must be accepted: {errors:?}"
        );
    }

    #[test]
    fn test_validate_schema_relative_ref_resolving_to_invalid_still_rejected() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "anchor": {"type": "string", "const": "not a gts id"},
                "relative": {
                    "type": "string",
                    "x-gts-ref": "/properties/anchor/const"
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(
            !errors.is_empty(),
            "relative ref resolving to an invalid identifier must be rejected"
        );
    }

    #[test]
    fn test_validate_instance_with_x_gts_ref_mismatch() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "topic_id": {
                    "type": "string",
                    "x-gts-ref": "gts.x.core.events.topic.*"
                }
            }
        });

        let instance = json!({
            "topic_id": "gts.y.core.events.topic.v1~"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_instance_with_dollar_id_ref_strips_gts_prefix() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "x-gts-ref": "/$id"
                }
            }
        });

        let instance = json!({
            "id": "gts.x.test._.entity.v1~"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty(), "Expected no errors but got: {errors:?}");
    }

    #[test]
    fn test_validate_instance_with_dollar_id_ref_rejects_full_uri() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "x-gts-ref": "/$id"
                }
            }
        });

        let instance = json!({
            "id": "gts://gts.x.test._.entity.v1~"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(
            !errors.is_empty(),
            "Expected validation error for value with gts:// prefix"
        );
    }

    #[test]
    fn test_strip_gts_uri_prefix() {
        assert_eq!(
            XGtsRefValidator::strip_gts_uri_prefix("gts://gts.x.test._.entity.v1~"),
            "gts.x.test._.entity.v1~"
        );
        assert_eq!(
            XGtsRefValidator::strip_gts_uri_prefix("gts.x.test._.entity.v1~"),
            "gts.x.test._.entity.v1~"
        );
        assert_eq!(XGtsRefValidator::strip_gts_uri_prefix(""), "");
        assert_eq!(
            XGtsRefValidator::strip_gts_uri_prefix("gts:/incomplete"),
            "gts:/incomplete"
        );
    }

    #[test]
    fn test_validation_error_creation_and_display() {
        let error = XGtsRefValidationError::new(
            "test_field".to_owned(),
            "invalid_value".to_owned(),
            "gts.x.*".to_owned(),
            "Test reason".to_owned(),
        );

        assert_eq!(error.field_path, "test_field");
        assert_eq!(error.value, "invalid_value");
        assert_eq!(error.ref_pattern, "gts.x.*");
        assert_eq!(error.reason, "Test reason");

        let display = format!("{error}");
        assert!(display.contains("test_field"));
        assert!(display.contains("Test reason"));
    }

    #[test]
    fn a_value_that_is_not_an_identifier_is_reported_as_such() {
        let errors = refs_of("not-a-valid-gts-id", "gts.*");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].reason.contains("not a valid GTS identifier"));

        for pattern in ["gts.x.y.*", "gts.x.y.z.w.v1~"] {
            let errors = refs_of("gts.a.b.c.d.v1~", pattern);
            assert_eq!(errors.len(), 1, "{pattern}: {errors:?}");
        }
    }

    #[test]
    fn a_literal_declaration_is_judged_by_the_pattern_parser() {
        let no_root = json!({});

        for ok in ["gts.x.core.events.topic.v1~", "gts.*", "gts.x.core.*"] {
            assert!(
                resolve_declaration(&json!(ok), &no_root).is_ok(),
                "expected '{ok}' to validate"
            );
        }

        // Reject malformed wildcard placement.
        for bad in ["gts.x.*.events.*", "gts.*.*.*.*"] {
            let reason = resolve_declaration(&json!(bad), &no_root)
                .expect_err("expected '{bad}' to be rejected");
            assert!(!reason.is_empty());
        }
    }

    #[test]
    fn a_major_version_pattern_matches_a_pinned_minor() {
        // A major-only pattern matches a concrete minor version.
        assert!(
            refs_of(
                "gts.x.core.events.event.v1.0~",
                "gts.x.core.events.event.v1~"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_declaration_is_resolved_the_same_way_everywhere() {
        // Compilation and schema validation share declaration rules.
        let with_id = json!({"$id": "gts://gts.x.test._.entity.v1~"});
        let no_root = json!({});

        assert!(resolve_declaration(&json!("gts.x.y.z.w.v1~"), &no_root).is_ok());
        assert!(resolve_declaration(&json!("/$id"), &with_id).is_ok());

        for (declared, root, expected) in [
            ("gts.INVALID", &no_root, "Invalid GTS identifier"),
            ("/nonexistent", &no_root, "Cannot resolve reference path"),
            ("invalid-format", &no_root, "must start with 'gts.' or '/'"),
        ] {
            let reason = resolve_declaration(&json!(declared), root)
                .expect_err("declaration must be rejected");
            assert!(reason.contains(expected), "{declared}: {reason}");
        }

        let lands_on_junk = json!({"notAnId": "not-a-valid-gts-id"});
        let reason = resolve_declaration(&json!("/notAnId"), &lands_on_junk)
            .expect_err("a pointer onto a non-identifier must be rejected");
        assert!(reason.contains("not a valid GTS identifier"), "{reason}");

        let reason = resolve_declaration(&json!(123), &no_root)
            .expect_err("a non-string declaration must be rejected");
        assert!(reason.contains("must be a string"), "{reason}");
    }

    #[test]
    fn a_relative_reference_is_resolved_before_matching() {
        let validator = XGtsRefValidator::new();
        let with_id = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "properties": {"self": {"type": "string", "x-gts-ref": "/$id"}}
        });
        assert!(
            validator
                .validate_instance(&json!({"self": "gts.x.test._.entity.v1~"}), &with_id, "")
                .is_empty()
        );

        // Unusable pointer targets make the schema invalid.
        for broken in [
            json!({
                "someField": "not-a-gts-pattern",
                "properties": {"r": {"type": "string", "x-gts-ref": "/someField"}}
            }),
            json!({"properties": {"r": {"type": "string", "x-gts-ref": "/missing"}}}),
        ] {
            let errors = validator.validate_instance(&json!({"r": "some-value"}), &broken, "");
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert!(
                errors[0].reason.contains("compilable schema"),
                "{:?}",
                errors[0]
            );
        }

        let missing = json!({"properties": {"r": {"type": "string", "x-gts-ref": "/missing"}}});
        let reported = validator.validate_schema(&missing, "", None);
        assert_eq!(reported.len(), 1, "{reported:?}");
        assert_eq!(reported[0].field_path, "properties/r/x-gts-ref");
        assert!(
            reported[0].reason.contains("Cannot resolve reference path"),
            "{:?}",
            reported[0]
        );
    }

    #[test]
    fn test_resolve_pointer() {
        let schema = json!({"$id": "gts://gts.x.test._.entity.v1~"});
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/$id"),
            Some("gts.x.test._.entity.v1~".to_owned())
        );

        let schema = json!({
            "properties": {
                "name": {
                    "x-gts-ref": "gts.x.test.*"
                }
            }
        });
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/properties/name/x-gts-ref"),
            Some("gts.x.test.*".to_owned())
        );

        let schema = json!({"properties": {}});
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/nonexistent"),
            None
        );

        let schema = json!({"$id": "test"});
        assert_eq!(XGtsRefValidator::resolve_pointer(&schema, "/"), None);

        let schema = json!({"value": "string"});
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/value/nested"),
            None
        );

        let schema = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "properties": {
                "type": {
                    "x-gts-ref": "/$id"
                }
            }
        });
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/properties/type"),
            Some("gts.x.test._.entity.v1~".to_owned())
        );

        let schema = json!({
            "$id": "gts://gts.x.test._.entity.v1~",
            "type": "gts://gts.x.another._.type.v1~"
        });
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/$id"),
            Some("gts.x.test._.entity.v1~".to_owned())
        );
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/type"),
            Some("gts.x.another._.type.v1~".to_owned())
        );
    }

    #[test]
    fn test_resolve_pointer_self_cycle_terminates() {
        let schema = json!({
            "properties": {
                "a": { "x-gts-ref": "/properties/a" }
            }
        });
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/properties/a"),
            None
        );

        let schema = json!({
            "properties": {
                "a": { "x-gts-ref": "/properties/b" },
                "b": { "x-gts-ref": "/properties/a" }
            }
        });
        assert_eq!(
            XGtsRefValidator::resolve_pointer(&schema, "/properties/a"),
            None
        );
    }

    #[test]
    fn test_visit_schema_non_string_x_gts_ref() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "properties": {
                "field": {
                    "x-gts-ref": 123
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(!errors.is_empty());
        assert!(errors[0].reason.contains("must be a string"));
    }

    #[test]
    fn test_visit_schema_nested_in_properties() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "field1": {
                    "type": "string",
                    "x-gts-ref": "gts.x.test.*"
                },
                "field2": {
                    "type": "string",
                    "x-gts-ref": "gts.y.test.*"
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(errors.is_empty());
    }

    #[test]
    fn the_build_rejects_a_declaration_that_says_nothing_usable() {
        // Invalid declarations in schema positions fail compilation.
        for broken in [
            json!({"type": "string", "x-gts-ref": "not-a-pattern"}),
            json!({"type": "string", "x-gts-ref": "gts.x.*.events.*"}),
            json!({"type": "string", "x-gts-ref": "/nowhere"}),
            json!({"type": "string", "x-gts-ref": 123}),
            json!({"type": "string", "x-gts-ref": ["gts.x.a.b.target.v1~"]}),
        ] {
            assert!(
                crate::json_schema::gts_validator_for(&broken, None).is_err(),
                "must not compile: {broken}"
            );
        }
    }

    #[test]
    fn the_build_accepts_a_declaration_spelled_inside_literal_data() {
        let schema = json!({
            "type": "object",
            "properties": {
                "payload": {
                    "const": {"x-gts-ref": "not-a-pattern"},
                    "default": {"x-gts-ref": 123},
                    "enum": [{"x-gts-ref": "not-a-pattern"}],
                    "examples": [{"x-gts-ref": "/nowhere"}]
                }
            }
        });

        let validator = crate::json_schema::gts_validator_for(&schema, None)
            .expect("literal data must not be read as a declaration");
        assert!(validator.is_valid(&json!({"payload": {"x-gts-ref": "not-a-pattern"}})));
    }

    #[test]
    fn a_declaration_is_only_read_from_a_subschema_position() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "payload": {
                    "const": {"x-gts-ref": "not-a-pattern"},
                    "default": {"x-gts-ref": "not-a-pattern"},
                    "enum": [{"x-gts-ref": "not-a-pattern"}],
                    "examples": [{"x-gts-ref": "not-a-pattern"}]
                },
                "x-gts-ref": {"type": "string"},
                "vendor": {"x-vendor-extension": {"x-gts-ref": "not-a-pattern"}}
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(
            errors.is_empty(),
            "literal data must not be read as a declaration: {errors:?}"
        );
    }

    #[test]
    fn a_declaration_nested_in_properties_is_still_validated() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "outer": {
                    "type": "object",
                    "properties": {
                        "inner": {"type": "string", "x-gts-ref": "not-a-pattern"}
                    }
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].field_path,
            "properties/outer/properties/inner/x-gts-ref"
        );
        assert!(errors[0].reason.contains("must start with"), "{errors:?}");
    }

    #[test]
    fn a_declaration_position_follows_the_declared_dialect() {
        let validator = XGtsRefValidator::new();
        let bad = json!({"type": "string", "x-gts-ref": "not-a-pattern"});

        let draft2020 = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "prefixItems": [bad]
        });
        assert_eq!(
            validator.validate_schema(&draft2020, "", None).len(),
            1,
            "draft 2020-12 reads `prefixItems` as subschemas"
        );

        let draft7 = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "prefixItems": [bad]
        });
        assert!(
            validator.validate_schema(&draft7, "", None).is_empty(),
            "draft-07 has no `prefixItems`, so this is annotation data"
        );
    }

    #[test]
    fn test_visit_schema_nested_in_array() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "items": [
                {
                    "x-gts-ref": "gts.x.test.*"
                },
                {
                    "x-gts-ref": "gts.y.test.*"
                }
            ]
        });

        let errors = validator.validate_schema(&schema, "", None);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_visit_instance_nested_objects() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "outer": {
                    "type": "object",
                    "properties": {
                        "inner": {
                            "type": "string",
                            "x-gts-ref": "gts.x.test.*"
                        }
                    }
                }
            }
        });

        let instance_valid = json!({
            "outer": {
                "inner": "gts.x.test._.entity.v1~"
            }
        });
        let errors = validator.validate_instance(&instance_valid, &schema, "");
        assert!(errors.is_empty(), "Valid nested value should pass");

        let instance_invalid = json!({
            "outer": {
                "inner": "gts.y.different._.entity.v1~"
            }
        });
        let errors = validator.validate_instance(&instance_invalid, &schema, "");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].field_path, "/outer/inner");
        assert_eq!(errors[0].ref_pattern, "gts.x.test.*");
        assert_eq!(errors[0].value, "gts.y.different._.entity.v1~");
    }

    #[test]
    fn test_visit_instance_array() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "array",
            "items": {
                "type": "string",
                "x-gts-ref": "gts.x.test.*"
            }
        });

        let instance = json!(["gts.x.test._.entity1.v1~", "gts.x.test._.entity2.v1~"]);

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_visit_instance_array_with_error() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "array",
            "items": {
                "type": "string",
                "x-gts-ref": "gts.x.test.*"
            }
        });

        let instance = json!(["gts.x.test._.entity1.v1~", "gts.y.other._.entity2.v1~"]);

        let errors = validator.validate_instance(&instance, &schema, "");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field_path, "/1");
    }

    #[test]
    fn an_uncheckable_schema_is_reported_rather_than_passed() {
        let errors = XGtsRefValidator::new().validate_instance(
            &json!({"field": "value"}),
            &json!("not an object"),
            "",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].reason.contains("compilable schema"),
            "{:?}",
            errors[0]
        );
    }

    #[test]
    fn test_visit_instance_no_x_gts_ref() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "field": {
                    "type": "string"
                }
            }
        });

        let instance = json!({
            "field": "any value"
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_visit_instance_value_not_string() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "field": {
                    "type": "number",
                    "x-gts-ref": "gts.x.test.*"
                }
            }
        });

        let instance = json!({
            "field": 123
        });

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_instance_empty_path() {
        let validator = XGtsRefValidator::new();
        let schema = json!({
            "type": "string",
            "x-gts-ref": "gts.x.test.*"
        });

        let instance = json!("gts.x.test._.entity.v1~");

        let errors = validator.validate_instance(&instance, &schema, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_schema_with_root_schema() {
        let validator = XGtsRefValidator::new();
        let root = json!({
            "$id": "gts://gts.x.test._.root.v1~"
        });

        let schema = json!({
            "properties": {
                "field": {
                    "x-gts-ref": "/$id"
                }
            }
        });

        let errors = validator.validate_schema(&schema, "", Some(&root));
        assert!(errors.is_empty());
    }

    #[test]
    fn test_strip_gts_uri_prefix_empty_string() {
        let result = XGtsRefValidator::strip_gts_uri_prefix("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_gts_uri_prefix_partial_prefix() {
        let result = XGtsRefValidator::strip_gts_uri_prefix("gts:/incomplete");
        assert_eq!(result, "gts:/incomplete");
    }
}

#[cfg(test)]
mod applicator_tests {
    use super::super::*;
    use serde_json::json;

    fn is_valid(instance: &Value, schema: &Value) -> bool {
        crate::json_schema::gts_validator_for(schema, None)
            .expect("schema compiles")
            .is_valid(instance)
    }

    fn nested_instance(depth: usize) -> Value {
        let mut inst = json!({"r": "bad"});
        for _ in 0..depth {
            inst = json!({"child": inst});
        }
        inst
    }

    #[test]
    fn recursive_any_of_stays_bounded() {
        // Recursive `anyOf` diagnostics grow exponentially; the verdict must not.
        let schema = json!({
            "type": "object",
            "properties": {
                "child": {"anyOf": [{"$ref": "#"}, {"$ref": "#"}]},
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });
        let instance = nested_instance(40);

        let started = std::time::Instant::now();
        let rejected = !is_valid(&instance, &schema);
        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        let elapsed = started.elapsed();

        assert!(
            rejected,
            "the nested reference violation must still be caught"
        );
        assert!(
            !errors.is_empty(),
            "the rejection must survive the budget, not be reported as clean"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "took {elapsed:?}; explaining the rejection is multiplying out"
        );
    }

    #[test]
    fn a_deep_rejection_reports_why_the_detail_is_missing() {
        let schema = json!({
            "type": "object",
            "properties": {
                "child": {"anyOf": [{"$ref": "#"}, {"$ref": "#"}]},
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });
        let validator =
            crate::json_schema::gts_validator_for(&schema, None).expect("schema compiles");

        let diagnosis = crate::json_schema::diagnose(&validator, &schema, &nested_instance(40));
        assert!(diagnosis.standard.is_empty(), "{:?}", diagnosis.standard);
        assert!(
            diagnosis.references.is_empty(),
            "{:?}",
            diagnosis.references
        );
        let reason = diagnosis
            .unexplained
            .expect("a rejection too costly to explain must say so");
        assert!(reason.contains("re-enter itself"), "{reason}");
    }

    #[test]
    fn a_wide_recursive_combinator_is_bounded_at_a_shallower_depth() {
        // Cost depends on both branch count and depth.
        let branch = json!({"$ref": "#"});
        let schema = json!({
            "type": "object",
            "properties": {
                "child": {"anyOf": vec![branch; 10]},
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });
        let instance = nested_instance(10);

        let started = std::time::Instant::now();
        let rejected = !is_valid(&instance, &schema);
        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        let elapsed = started.elapsed();

        assert!(
            rejected,
            "the nested reference violation must still be caught"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.reason.contains("were not verified")),
            "the guard must fire and still report the rejection: {errors:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "took {elapsed:?}; the branch count is not being accounted for"
        );
    }

    #[test]
    fn sibling_recursive_combinators_are_bounded_together() {
        // Sibling combinators multiply the fan-out.
        let branches = json!([{"$ref": "#"}, {"$ref": "#"}]);
        let schema = json!({
            "type": "object",
            "properties": {
                "child": {
                    "allOf": [
                        {"anyOf": branches},
                        {"anyOf": branches},
                    ]
                },
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });
        let instance = nested_instance(16);

        let started = std::time::Instant::now();
        let rejected = !is_valid(&instance, &schema);
        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        let elapsed = started.elapsed();

        assert!(
            rejected,
            "the nested reference violation must still be caught"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.reason.contains("were not verified")),
            "the guard must fire and still report the rejection: {errors:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "took {elapsed:?}; sibling combinators are not being added up"
        );
    }

    #[test]
    fn a_recursive_combinator_is_refused_even_when_the_value_is_shallow() {
        // Fan-out is unsafe even at depth one.
        let schema = json!({
            "type": "object",
            "properties": {
                "child": {"anyOf": [{"$ref": "#"}, {"$ref": "#"}]},
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });
        let instance = nested_instance(1);

        assert!(!is_valid(&instance, &schema), "the value is still rejected");
        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        assert!(
            errors
                .iter()
                .any(|e| e.reason.contains("were not verified")),
            "the rejection must be reported without detail: {errors:?}"
        );
    }

    #[test]
    fn the_conformance_shape_keeps_its_detail_before_and_after_resolution() {
        // Resolution inlines this self-reference once.
        let leaf = json!({
            "$id": "gts://gts.x.testref_root._.holder.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "link": {"type": "string", "x-gts-ref": "gts.x.testref_root._.target.v1~"},
                "child": {"$ref": "#"}
            },
            "additionalProperties": false
        });
        let mut resolved = leaf.clone();
        resolved["properties"]["child"] = leaf.clone();

        let instance = json!({
            "id": "gts.x.testref_root._.holder.v1~x.vendor._.bad.v1",
            "child": {"link": "gts.x.other._.target.v1~x.vendor._.bad.v1"}
        });

        for (label, schema) in [("authored", &leaf), ("resolved", &resolved)] {
            let errors = XGtsRefValidator::new().validate_instance(&instance, schema, "");
            assert!(
                errors
                    .iter()
                    .any(|e| e.reason.contains("does not match pattern")),
                "{label}: the suite asserts on this wording: {errors:?}"
            );
        }
    }

    #[test]
    fn an_unrelated_combinator_does_not_forfeit_the_detail() {
        // A non-recursive combinator adds no recursive fan-out.
        let schema = json!({
            "type": "object",
            "properties": {
                "child": {"$ref": "#"},
                "kind": {"anyOf": [{"type": "string"}, {"type": "integer"}]},
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });

        let errors = XGtsRefValidator::new().validate_instance(&nested_instance(6), &schema, "");
        let expected = format!("{}/r", "/child".repeat(6));
        assert!(
            errors.iter().any(|e| e.field_path == expected),
            "an unrelated combinator must not cost the detail: {errors:?}"
        );
    }

    #[test]
    fn a_recursive_all_of_is_refused() {
        let schema = json!({
            "type": "object",
            "properties": {
                "child": {"allOf": [{"$ref": "#"}, {"$ref": "#"}]},
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });
        let instance = nested_instance(12);

        let started = std::time::Instant::now();
        assert!(!is_valid(&instance, &schema), "the value is still rejected");
        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        let elapsed = started.elapsed();

        assert!(
            errors
                .iter()
                .any(|e| e.reason.contains("were not verified")),
            "{errors:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "took {elapsed:?}"
        );
    }

    #[test]
    fn two_property_recursions_are_refused() {
        let schema = json!({
            "type": "object",
            "properties": {
                "left": {"$ref": "#"},
                "right": {"$ref": "#"},
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });
        let instance = json!({"left": {"r": "bad"}});

        assert!(!is_valid(&instance, &schema), "the value is still rejected");
        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        assert!(
            errors
                .iter()
                .any(|e| e.reason.contains("were not verified")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_deep_document_under_a_flat_schema_keeps_its_detail() {
        let mut schema = json!({"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"});
        let mut instance = json!("gts.x.a.b.other.v1~x.c._.i.v1");
        for _ in 0..20 {
            schema = json!({"type": "object", "properties": {"child": schema}});
            instance = json!({"child": instance});
        }

        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].reason.contains("does not match pattern"));
    }

    #[test]
    fn a_reference_under_an_inapplicable_branch_is_not_the_finding() {
        // A mixed branch failure belongs to the standard vocabulary.
        let schema = json!({
            "anyOf": [
                {"type": "object"},
                {"type": "string", "x-gts-ref": "gts.*"},
            ]
        });

        let errors = XGtsRefValidator::new().validate_instance(&json!("bad"), &schema, "");
        assert!(
            errors.is_empty(),
            "no reference verdict is invented against the instance: {errors:?}"
        );
        assert!(
            !is_valid(&json!("bad"), &schema),
            "the document is still rejected, by the combinator"
        );
    }

    #[test]
    fn a_shadowing_definition_cannot_flip_a_verdict() {
        let schema = json!({
            "$defs": {
                "n": {"type": "string", "properties": {"next": {"$ref": "#/$defs/n"}}}
            },
            "anyOf": [
                {"$ref": "#/$defs/n", "$defs": {"n": {"type": "object"}}},
                {"type": "integer"}
            ]
        });

        let errors = XGtsRefValidator::new().validate_instance(&json!("ok"), &schema, "");
        assert!(errors.is_empty(), "a valid document must pass: {errors:?}");
    }

    #[test]
    fn a_property_named_default_does_not_affect_the_walk() {
        let schema = json!({
            "type": "object",
            "properties": {
                "x": {
                    "anyOf": [{
                        "type": "object",
                        "required": ["default"],
                        "properties": {"default": {"$ref": "#"}}
                    }]
                }
            }
        });

        let errors =
            XGtsRefValidator::new().validate_instance(&json!({"x": {"default": {}}}), &schema, "");
        assert!(errors.is_empty(), "a valid document must pass: {errors:?}");
    }

    #[test]
    fn a_reference_behind_recursive_refs_is_still_checked() {
        // A single property self-reference has no fan-out.
        let schema = json!({
            "type": "object",
            "properties": {
                "child": {"$ref": "#"},
                "r": {"type": "string", "x-gts-ref": "gts.x.a.b.target.v1~"},
            },
        });
        let validator = XGtsRefValidator::new();

        let shallow = validator.validate_instance(&nested_instance(1), &schema, "");
        assert!(
            shallow.iter().any(|e| e.field_path == "/child/r"),
            "the reference behind a $ref hop must be named: {shallow:?}"
        );

        let started = std::time::Instant::now();
        let deep = validator.validate_instance(&nested_instance(80), &schema, "");
        let elapsed = started.elapsed();
        let expected = format!("{}/r", "/child".repeat(80));
        assert!(
            deep.iter().any(|e| e.field_path == expected),
            "the reference behind eighty $ref hops must still be named: {deep:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "took {elapsed:?}; this shape must stay proportional to the value"
        );
    }

    #[test]
    fn nested_gts_combinators_accept_a_value_matching_one_inner_branch() {
        let schema = json!({
            "anyOf": [{
                "oneOf": [
                    {"x-gts-ref": "gts.x.a.b.target_a.v1~"},
                    {"x-gts-ref": "gts.x.a.b.target_b.v1~"},
                ]
            }]
        });
        let instance = json!("gts.x.a.b.target_a.v1~x.c._.a1.v1");

        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        assert!(
            errors.is_empty(),
            "a value matching exactly one inner branch must pass: {errors:?}"
        );
    }

    #[test]
    fn a_large_array_under_a_combinator_stays_cheap() {
        let ids: Vec<Value> = (0..2000)
            .map(|n| json!(format!("gts.x.a.b.target.v1~x.c._.item{n}.v1")))
            .collect();
        let schema = json!({
            "type": "array",
            "items": {
                "anyOf": [{
                    "type": "string",
                    "enum": Value::Array(ids.clone()),
                    "x-gts-ref": "gts.*",
                }]
            }
        });
        let instance = Value::Array(ids);

        let started = std::time::Instant::now();
        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        let elapsed = started.elapsed();

        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "walk took {elapsed:?}; the branch is being recompiled per element"
        );
    }

    #[test]
    fn an_unreadable_branch_never_invents_a_reference_verdict() {
        let schema = json!({
            "oneOf": [{}, {"$ref": "#/$defs/node"}],
            "$defs": {"node": {"$ref": "#/$defs/node"}}
        });

        let errors = XGtsRefValidator::new().validate_instance(&Value::Null, &schema, "");
        assert!(
            errors
                .iter()
                .all(|e| e.reason.contains("were not verified")),
            "no reference verdict may be invented: {errors:?}"
        );
    }

    #[test]
    fn a_branch_definition_cannot_shadow_a_root_definition() {
        let schema = json!({
            "$defs": {
                "a": {"$ref": "#/$defs/b"},
                "b": {"type": "object", "properties": {"next": {"$ref": "#/$defs/b"}}}
            },
            "anyOf": [
                {"$ref": "#/$defs/a", "$defs": {"b": {"type": "string"}}},
                {"type": "string", "x-gts-ref": "gts.*"}
            ]
        });

        let errors = XGtsRefValidator::new().validate_instance(&json!("bad"), &schema, "");
        assert!(
            errors
                .iter()
                .all(|e| !e.reason.contains("could not be evaluated")),
            "no branch should be reported unevaluatable: {errors:?}"
        );
    }

    #[test]
    fn deep_and_escaped_pointers_do_not_make_a_document_unverifiable() {
        for schema in [
            json!({
                "$defs": {
                    "a": {"properties": {"b": {"type": "object"}}}
                },
                "anyOf": [{"$ref": "#/$defs/a/properties/b"}, {"type": "string"}]
            }),
            json!({
                "$defs": {"a/b": {"type": "object"}},
                "anyOf": [{"$ref": "#/$defs/a~1b"}, {"type": "string"}]
            }),
            json!({
                "$defs": {"a~b": {"type": "object"}},
                "anyOf": [{"$ref": "#/$defs/a~0b"}, {"type": "string"}]
            }),
        ] {
            let errors = XGtsRefValidator::new().validate_instance(&json!({}), &schema, "");
            assert!(
                errors.is_empty(),
                "a valid document must not be rejected: {errors:?}"
            );
        }
    }

    #[test]
    fn an_unused_definition_still_does_not_affect_other_branches() {
        let schema = json!({
            "type": "object",
            "$defs": {"unused": {"$ref": "#"}},
            "properties": {
                "x": {"anyOf": [{"type": "string"}, {"type": "integer"}]}
            }
        });
        let errors = XGtsRefValidator::new().validate_instance(&json!({"x": "ok"}), &schema, "");
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn one_of_with_real_branches_never_invents_a_mismatch() {
        let schema = json!({
            "$defs": {
                "n": {"type": "object", "properties": {"next": {"$ref": "#/$defs/n"}}}
            },
            "oneOf": [
                {"type": "string", "x-gts-ref": "gts.*"},
                {"$ref": "#/$defs/n"}
            ]
        });

        let errors = XGtsRefValidator::new().validate_instance(
            &json!("gts.x.a.b.target.v1~x.c._.i.v1"),
            &schema,
            "",
        );
        assert!(
            errors.is_empty(),
            "a valid value must not be rejected: {errors:?}"
        );
    }

    #[test]
    fn one_of_branch_counting_still_applies_to_gts_discriminators() {
        let schema = json!({
            "oneOf": [
                {"x-gts-ref": "gts.x.a.b.target_a.v1~"},
                {"x-gts-ref": "gts.x.a.b.target_b.v1~"},
            ]
        });
        let validator = XGtsRefValidator::new();

        assert!(
            validator
                .validate_instance(&json!("gts.x.a.b.target_a.v1~x.c._.a1.v1"), &schema, "")
                .is_empty()
        );
        assert!(
            !validator
                .validate_instance(&json!("gts.x.a.b.other.v1~x.c._.o1.v1"), &schema, "")
                .is_empty()
        );

        // Multiple matches are a combinator error, not a reference error.
        let overlapping = json!({
            "oneOf": [
                {"x-gts-ref": "gts.x.a.b.*"},
                {"x-gts-ref": "gts.x.a.b.target_a.v1~"},
            ]
        });
        let both = json!("gts.x.a.b.target_a.v1~x.c._.a1.v1");
        assert!(
            validator
                .validate_instance(&both, &overlapping, "")
                .is_empty(),
            "an over-matched combinator is not a reference violation"
        );
        assert!(
            !is_valid(&both, &overlapping),
            "but the document is still rejected"
        );
    }

    #[test]
    fn a_property_named_default_is_a_property_not_an_annotation() {
        let schema = json!({
            "type": "object",
            "properties": {
                "x": {
                    "anyOf": [{
                        "type": "object",
                        "required": ["default"],
                        "properties": {"default": {"$ref": "#"}}
                    }]
                }
            }
        });

        let errors =
            XGtsRefValidator::new().validate_instance(&json!({"x": {"default": {}}}), &schema, "");
        assert!(errors.is_empty(), "a valid document must pass: {errors:?}");
    }

    #[test]
    fn an_unused_definition_does_not_disable_a_branchs_own_reference_check() {
        let schema = json!({
            "anyOf": [{
                "type": "string",
                "x-gts-ref": "gts.*",
                "$defs": {"unused": {"$ref": "#"}}
            }]
        });

        let errors = XGtsRefValidator::new().validate_instance(&json!("bad"), &schema, "");
        assert!(
            !errors.is_empty(),
            "the branch's own reference must still be checked: {errors:?}"
        );
    }

    #[test]
    fn bare_x_gts_ref_branches_still_discriminate_one_of() {
        let schema = json!({
            "oneOf": [
                {"x-gts-ref": "gts.x.a.b.target_a.v1~"},
                {"x-gts-ref": "gts.x.a.b.target_b.v1~"},
            ]
        });
        let validator = XGtsRefValidator::new();

        let matches_a = json!("gts.x.a.b.target_a.v1~x.c._.a1.v1");
        assert!(
            validator
                .validate_instance(&matches_a, &schema, "")
                .is_empty(),
            "a value matching exactly one branch must pass"
        );
        assert!(is_valid(&matches_a, &schema));

        let matches_neither = json!("gts.x.a.b.other.v1~x.c._.o1.v1");
        let errors = validator.validate_instance(&matches_neither, &schema, "");
        assert_eq!(
            errors.len(),
            2,
            "every branch failed on its reference, so both are the finding: {errors:?}"
        );
        assert!(!is_valid(&matches_neither, &schema));
    }

    fn nothing_exists() -> ReferenceExists {
        Arc::new(|_| false)
    }

    fn everything_exists() -> ReferenceExists {
        Arc::new(|_| true)
    }

    fn two_branch_schema(constrained_first: bool) -> Value {
        let constrained = json!({
            "required": ["other"],
            "properties": {"topic": {"type": "string", "x-gts-ref": "gts.x.a.b.topic.v1~"}}
        });
        let open = json!({"type": "object"});
        let branches = if constrained_first {
            vec![constrained, open]
        } else {
            vec![open, constrained]
        };
        json!({"type": "object", "anyOf": branches})
    }

    #[test]
    fn a_dangling_reference_lets_another_branch_apply() {
        let instance = json!({"topic": "gts.x.a.b.topic.v1~x.c._.missing.v1"});
        for constrained_first in [true, false] {
            let errors = validate_instance_refs(
                &instance,
                &two_branch_schema(constrained_first),
                "",
                Some(nothing_exists()),
            );
            assert!(
                errors.is_empty(),
                "constrained_first={constrained_first}: {errors:?}"
            );
        }
    }

    #[test]
    fn a_dangling_reference_is_reported_when_no_branch_tolerates_it() {
        let schema = json!({
            "type": "object",
            "anyOf": [
                {"properties": {"topic": {"type": "string", "x-gts-ref": "gts.x.a.b.topic.v1~"}}},
                {"properties": {"topic": {"type": "string", "x-gts-ref": "gts.x.a.b.topic.v1~"}}},
            ]
        });
        let instance = json!({"topic": "gts.x.a.b.topic.v1~x.c._.missing.v1"});

        let errors = validate_instance_refs(&instance, &schema, "", Some(nothing_exists()));
        assert!(
            errors.iter().any(|e| e.reason.contains("not registered")),
            "every branch failed on the same reference: {errors:?}"
        );
    }

    #[test]
    fn existence_applies_under_all_of() {
        let schema = json!({
            "type": "object",
            "allOf": [
                {"properties": {"topic": {"type": "string", "x-gts-ref": "gts.x.a.b.topic.v1~"}}}
            ]
        });
        let instance = json!({"topic": "gts.x.a.b.topic.v1~x.c._.missing.v1"});

        let errors = validate_instance_refs(&instance, &schema, "", Some(nothing_exists()));
        assert!(
            errors
                .iter()
                .any(|error| error.reason.contains("not registered")),
            "{errors:?}"
        );

        let satisfied = validate_instance_refs(&instance, &schema, "", Some(everything_exists()));
        assert!(satisfied.is_empty(), "{satisfied:?}");
    }

    #[test]
    fn a_pattern_violation_is_not_repeated_as_an_existence_failure() {
        let schema = json!({
            "type": "object",
            "properties": {"topic": {"type": "string", "x-gts-ref": "gts.x.a.b.topic.v1~"}}
        });
        let instance = json!({"topic": "gts.x.a.b.other.v1~x.c._.o1.v1"});

        let errors = validate_instance_refs(&instance, &schema, "", Some(nothing_exists()));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            !errors[0].reason.contains("not registered"),
            "the pattern violation is the finding: {errors:?}"
        );
    }

    #[test]
    fn without_a_registry_only_patterns_are_checked() {
        let schema = json!({
            "type": "object",
            "properties": {"topic": {"type": "string", "x-gts-ref": "gts.x.a.b.topic.v1~"}}
        });
        let instance = json!({"topic": "gts.x.a.b.topic.v1~x.c._.missing.v1"});

        let errors = XGtsRefValidator::new().validate_instance(&instance, &schema, "");
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn candidate_reference_values_covers_every_string_in_the_instance() {
        let instance = json!({
            "a": "one",
            "nested": {"b": "two", "deep": [{"c": "three"}, "four"]},
            "number": 5,
            "repeat": "one"
        });
        let mut found: Vec<String> = candidate_reference_values(&instance);
        found.sort();
        assert_eq!(found, ["four", "one", "three", "two"]);
    }

    // Applicators

    const TARGET: &str = "gts.x.a.b.target.v1~";
    const OTHER: &str = "gts.x.a.b.other.v1~";

    fn target_id() -> Value {
        json!("gts.x.a.b.target.v1~x.c._.i.v1")
    }

    fn other_id() -> Value {
        json!("gts.x.a.b.other.v1~x.c._.i.v1")
    }

    fn violation_paths(instance: &Value, schema: &Value) -> Vec<String> {
        XGtsRefValidator::new()
            .validate_instance(instance, schema, "")
            .into_iter()
            .map(|error| error.field_path)
            .collect()
    }

    #[test]
    fn not_inverts_a_reference_constraint() {
        let schema = json!({"not": {"type": "string", "x-gts-ref": TARGET}});

        assert!(
            !is_valid(&target_id(), &schema),
            "a matching reference must fail a negated subschema"
        );
        assert!(
            is_valid(&other_id(), &schema),
            "a non-matching reference satisfies it"
        );
    }

    #[test]
    fn if_then_else_applies_the_selected_branch_only() {
        let schema = json!({
            "type": "object",
            "if": {"type": "object", "required": ["kind"],
                   "properties": {"kind": {"const": "target"}}},
            "then": {"properties": {"ref": {"type": "string", "x-gts-ref": TARGET}}},
            "else": {"properties": {"ref": {"type": "string", "x-gts-ref": OTHER}}}
        });

        assert!(is_valid(
            &json!({"kind": "target", "ref": target_id()}),
            &schema
        ));
        assert!(is_valid(&json!({"ref": other_id()}), &schema));

        assert_eq!(
            violation_paths(&json!({"kind": "target", "ref": other_id()}), &schema),
            ["/ref"],
            "the `then` branch owns the constraint here"
        );
        assert_eq!(
            violation_paths(&json!({"ref": target_id()}), &schema),
            ["/ref"],
            "the `else` branch owns it here"
        );
    }

    #[test]
    fn contains_needs_one_member_whose_reference_matches() {
        let schema = json!({
            "type": "array",
            "contains": {"type": "string", "x-gts-ref": TARGET}
        });

        assert!(is_valid(&json!([other_id(), target_id()]), &schema));
        assert!(
            !is_valid(&json!([other_id()]), &schema),
            "no member satisfies the reference"
        );
    }

    #[test]
    fn pattern_and_additional_properties_carry_their_own_references() {
        let schema = json!({
            "type": "object",
            "patternProperties": {"^ref_": {"type": "string", "x-gts-ref": TARGET}},
            "additionalProperties": {"type": "string", "x-gts-ref": OTHER}
        });

        assert!(is_valid(
            &json!({"ref_a": target_id(), "spare": other_id()}),
            &schema
        ));

        let mut paths =
            violation_paths(&json!({"ref_a": other_id(), "spare": target_id()}), &schema);
        paths.sort();
        assert_eq!(paths, ["/ref_a", "/spare"]);
    }

    #[test]
    fn dependent_schemas_apply_their_references_when_triggered() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "dependentSchemas": {
                "kind": {"properties": {"ref": {"type": "string", "x-gts-ref": TARGET}}}
            }
        });

        assert!(
            is_valid(&json!({"ref": other_id()}), &schema),
            "the trigger property is absent, so the subschema does not apply"
        );
        assert_eq!(
            violation_paths(&json!({"kind": "k", "ref": other_id()}), &schema),
            ["/ref"]
        );
    }

    #[test]
    fn property_names_carry_references_too() {
        let schema = json!({
            "type": "object",
            "propertyNames": {"x-gts-ref": TARGET}
        });

        assert!(is_valid(
            &json!({"gts.x.a.b.target.v1~x.c._.i.v1": 1}),
            &schema
        ));
        assert!(!is_valid(
            &json!({"gts.x.a.b.other.v1~x.c._.i.v1": 1}),
            &schema
        ));
    }

    #[test]
    fn a_reference_behind_a_defs_pointer_is_decided_but_not_detailed() {
        // `$defs` recursion is outside the single-path exception.
        let schema = json!({
            "type": "object",
            "properties": {"ref": {"$ref": "#/$defs/TargetRef"}},
            "$defs": {"TargetRef": {"type": "string", "x-gts-ref": TARGET}}
        });

        assert!(is_valid(&json!({"ref": target_id()}), &schema));
        assert!(!is_valid(&json!({"ref": other_id()}), &schema));

        let errors =
            XGtsRefValidator::new().validate_instance(&json!({"ref": other_id()}), &schema, "");
        assert!(
            errors
                .iter()
                .any(|e| e.reason.contains("were not verified")),
            "{errors:?}"
        );
    }

    // Relative references (RFC 6901)

    #[test]
    fn a_relative_reference_resolves_escaped_pointer_tokens() {
        // RFC 6901 escapes `/` as `~1` and `~` as `~0`.
        let schema = json!({
            "$defs": {
                "a/b": {"const": "gts.x.a.b.slash.v1~"},
                "gts.x.a.b.tilde.v1~": {"const": "gts.x.a.b.tilde.v1~"}
            },
            "type": "object",
            "properties": {
                "slash": {"type": "string", "x-gts-ref": "/$defs/a~1b/const"},
                "tilde": {"type": "string", "x-gts-ref": "/$defs/gts.x.a.b.tilde.v1~0/const"}
            }
        });

        assert!(
            is_valid(
                &json!({
                    "slash": "gts.x.a.b.slash.v1~x.c._.i.v1",
                    "tilde": "gts.x.a.b.tilde.v1~x.c._.i.v1"
                }),
                &schema
            ),
            "both escaped pointers must resolve to their patterns"
        );

        let mut paths = violation_paths(
            &json!({
                "slash": "gts.x.a.b.tilde.v1~x.c._.i.v1",
                "tilde": "gts.x.a.b.slash.v1~x.c._.i.v1"
            }),
            &schema,
        );
        paths.sort();
        assert_eq!(paths, ["/slash", "/tilde"]);
    }

    #[test]
    fn an_unescaped_pointer_token_does_not_resolve() {
        let schema = json!({
            "$defs": {"a/b": {"const": "gts.x.a.b.slash.v1~"}},
            "type": "object",
            "properties": {"r": {"type": "string", "x-gts-ref": "/$defs/a/b/const"}}
        });

        let errors = XGtsRefValidator::new().validate_instance(
            &json!({"r": "gts.x.a.b.slash.v1~x.c._.i.v1"}),
            &schema,
            "",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].reason.contains("Cannot resolve reference path"),
            "{:?}",
            errors[0]
        );
    }

    #[test]
    fn a_relative_reference_can_point_through_an_array() {
        let schema = json!({
            "type": "object",
            "examples": ["gts.x.a.b.target.v1~"],
            "properties": {"r": {"type": "string", "x-gts-ref": "/examples/0"}}
        });

        assert!(is_valid(&json!({"r": target_id()}), &schema));
        assert_eq!(violation_paths(&json!({"r": other_id()}), &schema), ["/r"]);
    }

    #[test]
    fn a_relative_reference_follows_a_pointer_chain_to_the_document_id() {
        // Follow a pointer chain and strip `gts://` from the document ID.
        let schema = json!({
            "$id": "gts://gts.x.testref._.pointer.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string", "x-gts-ref": "/$id"},
                "type": {"type": "string", "x-gts-ref": "/properties/id"}
            }
        });

        assert!(
            is_valid(
                &json!({
                    "id": "gts.x.testref._.pointer.v1~x.vendor._.ptr.v1",
                    "type": "gts.x.testref._.pointer.v1~"
                }),
                &schema
            ),
            "both pointers resolve to the document id"
        );

        let mut paths = violation_paths(
            &json!({
                "id": "gts://gts.x.testref._.pointer.v1~",
                "type": "gts.x.testref._.wrong.v1~"
            }),
            &schema,
        );
        paths.sort();
        assert_eq!(
            paths,
            ["/id", "/type"],
            "the `gts://` form is not an identifier, and a foreign id misses the pattern"
        );
    }

    #[test]
    fn a_bare_slash_pointer_names_the_empty_key_not_the_root() {
        // In RFC 6901, `/` names the empty key rather than the root.
        let schema = json!({
            "$id": "gts://gts.x.a.b.target.v1~",
            "type": "object",
            "properties": {"r": {"type": "string", "x-gts-ref": "/"}}
        });

        let errors =
            XGtsRefValidator::new().validate_instance(&json!({"r": target_id()}), &schema, "");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].reason.contains("Cannot resolve reference path"),
            "{:?}",
            errors[0]
        );
    }
}
