//! Shared JSON Schema validation with GTS formats and diagnostics.
//!
//! GTS asserts `uuid` on every dialect. Regular expressions follow the GTS
//! profile (README §11.0.1): `format: "regex"` asserts membership, every
//! schema pattern is checked before a validator is built, and matching uses
//! the linear-time `regex` engine.

use serde_json::{Map, Value};
use uuid::Uuid;

/// Configures GTS formats without asserting optional formats on newer drafts.
#[must_use]
fn options_for(schema: &Value) -> jsonschema::ValidationOptions<'_> {
    let mut options = jsonschema::options()
        .with_format("uuid", is_valid_uuid)
        .with_format("regex", crate::regex_profile::is_supported)
        .with_pattern_options(jsonschema::PatternOptions::regex());

    if declared_dialect_asserts_formats(schema) {
        return options;
    }

    // jsonschema 0.58 still applies `dependencies` in 2019-09+; disable it
    // to match the dialect-aware regex profile check.
    options = options.with_keyword("dependencies", |_, _, _| Ok(Box::new(NotAKeyword)));

    options = options.should_validate_formats(true);
    for name in ASSERTABLE_FORMATS
        .iter()
        .filter(|name| !GTS_ASSERTED_FORMATS.contains(*name))
    {
        options = options.with_format(*name, |_| true);
    }
    options
}

/// A name the dialect does not define as a keyword: it never fails.
struct NotAKeyword;

impl<'i> jsonschema::Keyword<'i> for NotAKeyword {
    fn validate(&self, _instance: &'i Value) -> Result<(), jsonschema::ValidationError<'i>> {
        Ok(())
    }

    fn is_valid(&self, _instance: &Value) -> bool {
        true
    }
}

/// Formats GTS requires every dialect to assert (README sec 9.2).
const GTS_ASSERTED_FORMATS: &[&str] = &[
    "uuid",
    "regex",
    "email",
    "date-time",
    "date",
    "time",
    "uri",
    "hostname",
    "ipv4",
    "ipv6",
];

/// Every format the validator knows how to assert.
const ASSERTABLE_FORMATS: &[&str] = &[
    "date",
    "date-time",
    "duration",
    "email",
    "hostname",
    "idn-email",
    "idn-hostname",
    "ipv4",
    "ipv6",
    "iri",
    "iri-reference",
    "json-pointer",
    "regex",
    "relative-json-pointer",
    "time",
    "uri",
    "uri-reference",
    "uri-template",
    "uuid",
];

/// Whether the root dialect asserts `format` by default.
///
/// The validator-wide switch means embedded resources follow the root dialect.
fn declared_dialect_asserts_formats(schema: &Value) -> bool {
    matches!(
        jsonschema::Draft::default().detect(schema),
        jsonschema::Draft::Draft4 | jsonschema::Draft::Draft6 | jsonschema::Draft::Draft7
    )
}

/// Compiles `schema` with GTS format assertions.
///
/// # Errors
///
/// Returns the underlying compilation error when `schema` is not a valid
/// JSON Schema.
pub fn validator_for(
    schema: &Value,
) -> Result<jsonschema::Validator, jsonschema::ValidationError<'static>> {
    check_regex_profile(schema, &[])?;
    options_for(schema).build(schema)
}

/// Rejects `schema` if it or `resources` hold an expression outside the GTS
/// regex profile in a schema position.
fn check_regex_profile(
    schema: &Value,
    resources: &[(String, &Value)],
) -> Result<(), jsonschema::ValidationError<'static>> {
    let root = schema
        .get("$id")
        .and_then(Value::as_str)
        .unwrap_or(crate::schema_regex::DEFAULT_BASE_URI);
    let documents: Vec<(&str, &Value)> = std::iter::once((root, schema))
        .chain(
            resources
                .iter()
                .map(|(uri, document)| (uri.as_str(), *document)),
        )
        .collect();
    let registry = jsonschema::Registry::new()
        .extend(documents.iter().copied())
        .and_then(jsonschema::RegistryBuilder::prepare)
        .map_err(jsonschema::ValidationError::from)?;
    crate::schema_regex::check(&registry, &documents).map_err(jsonschema::ValidationError::schema)
}

/// Compiles `schema` with GTS formats and `x-gts-ref` enforcement.
///
/// `/$id` names the document's own top-level `$id`. `exists` enables
/// store-aware reference checks; `None` checks patterns only.
///
/// # Errors
///
/// Returns the underlying compilation error when `schema` is not a valid
/// JSON Schema.
pub fn gts_validator_for(
    schema: &Value,
    exists: Option<crate::x_gts_ref::ReferenceExists>,
) -> Result<jsonschema::Validator, jsonschema::ValidationError<'static>> {
    let selected = crate::x_gts_ref::XGtsRefValidator::self_id(schema);
    gts_validator_for_type(schema, selected.as_deref(), &[], exists)
}

/// [`gts_validator_for`] on behalf of `selected_type`, the GTS type being
/// validated, which `/$id` names wherever it is declared (spec v0.14 §9.6).
///
/// `resources` are the other documents `schema` references, by URI; the
/// validator follows every `$ref` itself, under the rules of its dialect.
///
/// # Errors
///
/// See [`gts_validator_for`].
pub fn gts_validator_for_type(
    schema: &Value,
    selected_type: Option<&str>,
    resources: &[(String, &Value)],
    exists: Option<crate::x_gts_ref::ReferenceExists>,
) -> Result<jsonschema::Validator, jsonschema::ValidationError<'static>> {
    check_regex_profile(schema, resources)?;
    let builder = jsonschema::Registry::new()
        .extend(resources.iter().map(|(uri, document)| (uri, *document)))
        .map_err(jsonschema::ValidationError::from)?;
    let builder = if let Some(uri) = schema.get("$id").and_then(Value::as_str) {
        builder
            .add(uri, schema)
            .map_err(jsonschema::ValidationError::from)?
    } else {
        builder
    };
    let registry = builder
        .prepare()
        .map_err(jsonschema::ValidationError::from)?;
    crate::x_gts_ref::with_x_gts_ref(
        options_for(schema),
        selected_type.map(str::to_owned),
        exists,
    )
    .with_registry(&registry)
    .build(schema)
}

/// A validation result split by diagnostic source.
///
/// `unexplained` still represents a rejection and must be treated as failure.
#[derive(Debug, Default)]
pub struct Diagnosis {
    /// Standard-vocabulary diagnostics.
    pub standard: Vec<String>,
    /// `x-gts-ref` violations.
    pub references: Vec<crate::x_gts_ref::XGtsRefValidationError>,
    /// Why a rejection carries no detail, when that happened.
    pub unexplained: Option<String>,
}

/// Validates exactly, omitting details when explaining recursive combinators
/// would grow exponentially.
pub fn diagnose(validator: &jsonschema::Validator, schema: &Value, instance: &Value) -> Diagnosis {
    diagnose_resolved(validator, schema, Some(schema), instance)
}

/// [`diagnose`] for a `schema` whose references `validator` follows itself.
///
/// `resolved` is `schema` with its references inlined, the shape whose
/// recursion bounds what explaining a rejection costs. Without it (a `$ref`
/// cycle defeats inlining), a rejection is left unexplained.
pub fn diagnose_resolved(
    validator: &jsonschema::Validator,
    schema: &Value,
    resolved: Option<&Value>,
    instance: &Value,
) -> Diagnosis {
    if validator.is_valid(instance) {
        return Diagnosis::default();
    }

    if !resolved.is_some_and(has_linear_recursion) {
        return Diagnosis {
            unexplained: Some(
                "the schema can re-enter itself in a shape where explaining a rejection costs \
                 exponentially more than deciding it"
                    .to_owned(),
            ),
            ..Diagnosis::default()
        };
    }

    let (standard, references) =
        crate::x_gts_ref::split_errors(schema, validator.iter_errors(instance));
    Diagnosis {
        standard,
        references,
        unexplained: None,
    }
}

/// Whether explaining a rejection has no recursive fan-out.
///
/// True without re-entry, or with a single `$ref` reached from the root only
/// through keywords that descend into the instance: every re-entry then lands
/// deeper in the instance, so each location is explained once (a tree through
/// `items`, a map through `additionalProperties`). A combinator on the path
/// re-evaluates its branches while explaining, and two re-entry points can meet
/// at one location; either compounds per level. Every `$ref` counts, since
/// anchors and same-document URIs re-enter as well as JSON Pointers.
fn has_linear_recursion(schema: &Value) -> bool {
    let mut reentries: Vec<*const Map<String, Value>> = Vec::new();
    let mut dynamic = false;
    crate::schema_modifiers::for_each_schema_node(schema, &mut |node, _| {
        dynamic |= node.contains_key("$dynamicRef") || node.contains_key("$recursiveRef");
        if node.contains_key("$ref") {
            reentries.push(std::ptr::from_ref(node));
        }
    });
    match (dynamic, reentries.as_slice()) {
        (false, []) => true,
        (false, [only]) => descends_to(schema, *only),
        _ => false,
    }
}

/// Whether `target` lies below `node` through instance-descending keywords.
fn descends_to(node: &Value, target: *const Map<String, Value>) -> bool {
    let Value::Object(map) = node else {
        return false;
    };
    let mut children = ["properties", "patternProperties"]
        .into_iter()
        .filter_map(|keyword| map.get(keyword).and_then(Value::as_object))
        .flat_map(Map::values)
        .chain(
            ["items", "prefixItems"]
                .into_iter()
                .filter_map(|keyword| map.get(keyword).and_then(Value::as_array))
                .flatten(),
        )
        .chain(
            ["additionalProperties", "items", "additionalItems"]
                .into_iter()
                .filter_map(|keyword| map.get(keyword).filter(|child| child.is_object())),
        );
    children.any(|child| {
        child.as_object().is_some_and(|m| std::ptr::eq(m, target)) || descends_to(child, target)
    })
}

/// Checks for a hyphenated RFC 4122 UUID.
#[must_use]
fn is_valid_uuid(value: &str) -> bool {
    Uuid::try_parse(value).is_ok() && value.len() == 36
}

/// Renders validator errors in the reference implementation's format.
///
/// Only type errors need rewriting; other messages may contain schema text.
#[must_use]
pub fn render_error(error: &jsonschema::ValidationError<'_>) -> String {
    match error.kind() {
        jsonschema::error::ValidationErrorKind::Type { kind } => {
            let types = match kind {
                jsonschema::error::TypeKind::Single(single) => format!("'{single}'"),
                jsonschema::error::TypeKind::Multiple(set) => set
                    .iter()
                    .map(|each| format!("'{each}'"))
                    .collect::<Vec<_>>()
                    .join(", "),
            };
            format!("{} is not of type {types}", repr_instance(error.instance()))
        }
        _ => error.to_string(),
    }
}

/// Uses single quotes for strings and JSON syntax for other values.
fn repr_instance(instance: &Value) -> String {
    match instance {
        Value::String(text) => format!("'{}'", text.replace('\'', "\\'")),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnose_json(schema: &serde_json::Value, instance: &serde_json::Value) -> Diagnosis {
        let validator = gts_validator_for(schema, None).expect("schema compiles");
        diagnose(&validator, schema, instance)
    }

    #[test]
    fn a_tree_through_items_keeps_its_detail() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "kids": {"type": "array", "items": {"$ref": "#"}}
            }
        });
        let diagnosis = diagnose_json(
            &schema,
            &serde_json::json!({"kids": [{"name": "a"}, {"kids": [{"name": 1}]}]}),
        );
        assert!(diagnosis.unexplained.is_none(), "{diagnosis:?}");
        assert_eq!(
            diagnosis.standard,
            vec!["1 is not of type 'string'".to_owned()]
        );
    }

    #[test]
    fn a_map_through_additional_properties_keeps_its_detail() {
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": {"type": "object", "properties": {"sub": {"$ref": "#"}}}
        });
        let diagnosis = diagnose_json(&schema, &serde_json::json!({"a": {"sub": {"b": 1}}}));
        assert!(diagnosis.unexplained.is_none(), "{diagnosis:?}");
        assert_eq!(diagnosis.standard.len(), 1, "{diagnosis:?}");
    }

    #[test]
    fn a_recursion_through_a_named_anchor_stays_unexplained() {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$anchor": "node",
            "type": "object",
            "properties": {
                "n": {"type": "integer"},
                "child": {"anyOf": [{"$ref": "#node"}, {"$ref": "#node"}]}
            }
        });
        let diagnosis = diagnose_json(&schema, &serde_json::json!({"child": {"n": "x"}}));
        assert!(diagnosis.unexplained.is_some(), "{diagnosis:?}");
    }

    #[test]
    fn a_single_recursion_under_a_combinator_stays_unexplained() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "child": {"anyOf": [{"$ref": "#"}, {"type": "null"}]},
                "n": {"type": "integer"}
            }
        });
        let diagnosis = diagnose_json(&schema, &serde_json::json!({"child": {"n": "x"}}));
        assert!(diagnosis.unexplained.is_some(), "{diagnosis:?}");
    }

    #[test]
    fn format_mode_uses_exact_dialect_detection() {
        assert!(declared_dialect_asserts_formats(&serde_json::json!({
            "$schema": "http://json-schema.org/draft-04/schema#"
        })));
        assert!(declared_dialect_asserts_formats(&serde_json::json!({
            "$schema": "http://json-schema.org/draft-06/schema#"
        })));
        assert!(!declared_dialect_asserts_formats(&serde_json::json!({
            "$schema": "https://vendor.example/draft-07-compatible/schema"
        })));
    }

    #[test]
    fn formats_are_asserted_on_draft_2020_12() {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "string",
            "format": "uuid"
        });
        let validator = validator_for(&schema).expect("schema compiles");
        assert!(validator.is_valid(&serde_json::json!("550e8400-e29b-41d4-a716-446655440000")));
        assert!(!validator.is_valid(&serde_json::json!("bad")));
    }

    #[test]
    fn unknown_formats_still_compile() {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "string",
            "format": "some-vendor-format"
        });
        let validator = validator_for(&schema).expect("unknown format compiles");
        assert!(validator.is_valid(&serde_json::json!("anything")));
    }

    #[test]
    fn render_error_preserves_a_quote_inside_the_reported_value() {
        let schema = serde_json::json!({"type": "integer"});
        let validator = validator_for(&schema).expect("schema compiles");
        let instance = serde_json::json!("a\"b");
        let rendered: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| render_error(&e))
            .collect();
        assert_eq!(rendered, vec!["'a\"b' is not of type 'integer'".to_owned()]);
    }

    #[test]
    fn render_error_escapes_a_single_quote_in_the_value() {
        let schema = serde_json::json!({"type": "integer"});
        let validator = validator_for(&schema).expect("schema compiles");
        let instance = serde_json::json!("a'b");
        let rendered: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| render_error(&e))
            .collect();
        assert_eq!(
            rendered,
            vec![r"'a\'b' is not of type 'integer'".to_owned()]
        );
    }

    #[test]
    fn render_error_leaves_other_messages_verbatim() {
        let schema = serde_json::json!({"type": "string", "pattern": "a\"b"});
        let validator = validator_for(&schema).expect("schema compiles");
        let instance = serde_json::json!("x");
        let rendered: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| render_error(&e))
            .collect();
        assert_eq!(rendered.len(), 1);
        assert!(
            rendered[0].contains(r#"a"b"#),
            "the pattern must survive intact: {}",
            rendered[0]
        );
    }

    #[test]
    fn optional_formats_keep_annotation_semantics_on_draft_2020_12() {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "string",
            "format": "duration"
        });
        let validator = validator_for(&schema).expect("schema compiles");
        assert!(validator.is_valid(&serde_json::json!("not-a-duration")));
    }

    #[test]
    fn optional_formats_still_assert_on_draft_07() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "string",
            "format": "json-pointer"
        });
        let validator = validator_for(&schema).expect("schema compiles");
        assert!(validator.is_valid(&serde_json::json!("/valid/pointer")));
        assert!(!validator.is_valid(&serde_json::json!("not-a-pointer")));
    }

    #[test]
    fn annotation_data_does_not_change_how_the_document_is_validated() {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "default": {"$schema": "http://json-schema.org/draft-07/schema#"},
            "properties": {"duration": {"type": "string", "format": "duration"}}
        });
        let validator = validator_for(&schema).expect("schema compiles");
        assert!(
            validator.is_valid(&serde_json::json!({"duration": "bad"})),
            "`duration` is annotation-only here; the `default` must not change that"
        );
    }

    #[test]
    fn an_embedded_legacy_resource_follows_the_documents_decision() {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "x": {
                    "$id": "https://example.com/old",
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "string",
                    "format": "json-pointer"
                }
            }
        });
        let validator = validator_for(&schema).expect("schema compiles");
        assert!(validator.is_valid(&serde_json::json!({"x": "/ok"})));
        assert!(validator.is_valid(&serde_json::json!({"x": "bad"})));

        let required = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "x": {
                    "$id": "https://example.com/old2",
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "string",
                    "format": "uuid"
                }
            }
        });
        let validator = validator_for(&required).expect("schema compiles");
        assert!(!validator.is_valid(&serde_json::json!({"x": "bad"})));
    }

    #[test]
    fn no_gts_required_format_is_neutralised() {
        for required in GTS_ASSERTED_FORMATS {
            assert!(
                ASSERTABLE_FORMATS.contains(required),
                "{required} is required by GTS but is not in the assertable set, \
                 so the neutralising filter would never see it"
            );
        }
    }

    #[test]
    fn required_formats_assert_on_every_dialect() {
        for dialect in [
            "http://json-schema.org/draft-07/schema#",
            "https://json-schema.org/draft/2019-09/schema",
            "https://json-schema.org/draft/2020-12/schema",
        ] {
            let schema = serde_json::json!({
                "$schema": dialect, "type": "string", "format": "ipv4"
            });
            let validator = validator_for(&schema).expect("schema compiles");
            assert!(
                !validator.is_valid(&serde_json::json!("999.999.999.999")),
                "ipv4 must assert under {dialect}"
            );
        }
    }

    #[test]
    fn render_error_single_quotes_the_type_name() {
        let schema = serde_json::json!({"type": "string"});
        let validator = validator_for(&schema).expect("schema compiles");
        let instance = serde_json::json!(1);
        let rendered: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| render_error(&e))
            .collect();
        assert_eq!(rendered, vec!["1 is not of type 'string'".to_owned()]);
    }

    #[test]
    fn uuid_format_accepts_hyphenated_uuid() {
        assert!(is_valid_uuid("550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn uuid_format_rejects_non_uuid_values() {
        assert!(!is_valid_uuid("not-a-uuid"));
        assert!(!is_valid_uuid(""));
        assert!(!is_valid_uuid("550e8400e29b41d4a716446655440000"));
    }

    #[test]
    fn uuid_format_rejects_gts_identifiers() {
        assert!(!is_valid_uuid("gts.x.a.b.c.v1~x.d._.e.v1"));
        assert!(!is_valid_uuid("gts.x.a.b.c.v1~"));
    }

    #[test]
    fn draft7_schema_asserts_uuid_format() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "string",
            "format": "uuid"
        });
        let validator = validator_for(&schema).expect("schema compiles");
        assert!(validator.is_valid(&serde_json::json!("550e8400-e29b-41d4-a716-446655440000")));
        assert!(!validator.is_valid(&serde_json::json!("not-a-uuid")));
    }

    #[test]
    fn regex_format_asserts_the_gts_profile() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "string",
            "format": "regex"
        });
        let validator = validator_for(&schema).expect("schema compiles");
        assert!(validator.is_valid(&serde_json::json!("^[A-Za-z0-9]+$")));
        // Malformed, ECMA-262 only, RE2 only, and beyond the bounds.
        for outside in ["a**", "a(?=b)", r"abc\z", "a{1001}"] {
            assert!(
                !validator.is_valid(&serde_json::json!(outside)),
                "{outside}"
            );
        }
        // An ordinary format violation, which `not` inverts.
        let negated = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "not": {"format": "regex"}
        });
        let validator = validator_for(&negated).expect("schema compiles");
        assert!(validator.is_valid(&serde_json::json!("a(?=b)")));
        assert!(!validator.is_valid(&serde_json::json!("abc")));
    }

    #[test]
    fn an_unsupported_schema_pattern_is_a_schema_error() {
        for schema in [
            serde_json::json!({"pattern": "a(?=b)"}),
            serde_json::json!({"anyOf": [true, {"not": {"pattern": r"abc\z"}}]}),
            serde_json::json!({"patternProperties": {"(?i)a": true}}),
            serde_json::json!({
                "anyOf": [true, {"$ref": "#/custom/x"}],
                "custom": {"x": {"pattern": "a{1001}"}}
            }),
            serde_json::json!({"x-gts-traits-schema": {"properties": {"v": {"pattern": "["}}}}),
        ] {
            let error = validator_for(&schema)
                .err()
                .unwrap_or_else(|| panic!("{schema}"));
            assert!(
                error.to_string().contains("unsupported regular expression"),
                "{schema}: {error}"
            );
            assert!(gts_validator_for(&schema, None).is_err(), "{schema}");
        }
    }

    /// jsonschema 0.58.5 treats classification panics as non-matches (#1715,
    /// fixed by #1721). Reproducing rust-lang/regex#1344 requires sizes beyond
    /// the profile bounds. When fixed, remove the README's "Engine panics" gap.
    #[test]
    fn known_gap_an_engine_panic_in_classification_is_dropped() {
        let pattern = "^.{0,404600}$";
        let schema = serde_json::json!({"patternProperties": {pattern: {"type": "integer"}}});
        let validator = jsonschema::options()
            .with_pattern_options(jsonschema::PatternOptions::regex().size_limit(1_000_000_000))
            .build(&schema)
            .expect("compiles under the inflated limit");
        assert!(
            validator.is_valid(&serde_json::json!({"": "not-an-integer"})),
            "jsonschema now reports the panic"
        );
        assert!(
            crate::regex_profile::check(pattern).is_err(),
            "the expression is outside the profile bounds"
        );
    }

    /// Known gap: jsonschema 0.58.5 uses a custom `\s` set and ECMA-262
    /// whitespace for `^\S*$`, violating RE2 semantics. When fixed, remove
    /// the gap from README and `.gts-spec-known-failures`.
    #[test]
    fn known_gap_jsonschema_spells_its_own_space_set() {
        let space = validator_for(&serde_json::json!({"pattern": r"^\s$"})).expect("compiles");
        let non_space = validator_for(&serde_json::json!({"pattern": r"^\S*$"})).expect("compiles");
        // Outside the reference set, yet matched.
        for probe in ["\u{b}", "\u{a0}", "\u{feff}"] {
            assert!(space.is_valid(&serde_json::json!(probe)), "{probe:?}");
            assert!(!non_space.is_valid(&serde_json::json!(probe)), "{probe:?}");
        }
        // Off the fast path too.
        let single = validator_for(&serde_json::json!({"pattern": r"^\S$"})).expect("compiles");
        assert!(!single.is_valid(&serde_json::json!("\u{a0}")));
        // The reference set itself still matches.
        for probe in ["\t", "\n", "\u{c}", "\r", " "] {
            assert!(space.is_valid(&serde_json::json!(probe)), "{probe:?}");
        }
    }

    #[test]
    fn matching_follows_the_reference_semantics() {
        let cases = [
            // `.` matches CR, U+2028 and U+2029, but not LF.
            (r"^.$", "\r", true),
            (r"^.$", "\u{2028}", true),
            (r"^.$", "\n", false),
            // `$` does not match before a final LF.
            (r"^abc$", "abc\n", false),
            // ASCII `\d` and `\w`.
            (r"^\d$", "\u{0661}", false),
            (r"^\w$", "\u{e9}", false),
            (r"^[^\W]$", "\u{e9}", false),
        ];
        for (pattern, input, expected) in cases {
            let schema = serde_json::json!({"pattern": pattern});
            let validator = validator_for(&schema).expect("schema compiles");
            assert_eq!(
                validator.is_valid(&serde_json::json!(input)),
                expected,
                "{pattern} on {input:?}"
            );
        }
    }
}
