//! Shared JSON Schema validation with GTS formats and diagnostics.
//!
//! GTS asserts `uuid` on every dialect and uses ECMA 262 syntax for `regex`.

use serde_json::{Map, Value};
use std::sync::{OnceLock, mpsc};
use uuid::Uuid;

/// Configures GTS formats without asserting optional formats on newer drafts.
#[must_use]
fn options_for(schema: &Value) -> jsonschema::ValidationOptions {
    let mut options = jsonschema::options()
        .with_format("uuid", is_valid_uuid)
        .with_format("regex", is_valid_ecma262_regex);

    if declared_dialect_asserts_formats(schema) {
        return options;
    }

    options = options.should_validate_formats(true);
    for name in ASSERTABLE_FORMATS
        .iter()
        .filter(|name| !GTS_ASSERTED_FORMATS.contains(*name))
    {
        options = options.with_format(*name, |_| true);
    }
    options
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
    options_for(schema).build(schema)
}

/// Compiles `schema` with GTS formats and `x-gts-ref` enforcement.
///
/// `exists` enables store-aware reference checks; `None` checks patterns only.
///
/// # Errors
///
/// Returns the underlying compilation error when `schema` is not a valid
/// JSON Schema.
pub fn gts_validator_for(
    schema: &Value,
    exists: Option<crate::x_gts_ref::ReferenceExists>,
) -> Result<jsonschema::Validator, jsonschema::ValidationError<'static>> {
    crate::x_gts_ref::with_x_gts_ref(options_for(schema), schema, exists).build(schema)
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
    if validator.is_valid(instance) {
        return Diagnosis::default();
    }

    if !has_linear_recursion(schema) {
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

/// Checks ECMA 262 regex syntax (README sec 9.2, ADR-0005).
#[must_use]
fn is_valid_ecma262_regex(pattern: &str) -> bool {
    // Bound the recursive parser's input and stack usage.
    if pattern.len() > MAX_REGEX_LEN {
        return false;
    }
    if pattern.len() <= REGEX_INLINE_LEN {
        return regress::Regex::new(pattern).is_ok();
    }
    parse_on_owned_stack(pattern)
}

/// Longest accepted `format: regex` value.
const MAX_REGEX_LEN: usize = 32 * 1024;

/// Patterns safe to parse on the smallest supported worker stack.
const REGEX_INLINE_LEN: usize = 512;

/// Stack reserved for parsing larger patterns.
const REGEX_PARSE_STACK: usize = 64 * 1024 * 1024;

struct RegexParseRequest {
    pattern: String,
    response: mpsc::SyncSender<bool>,
}

static REGEX_WORKER: OnceLock<Option<mpsc::SyncSender<RegexParseRequest>>> = OnceLock::new();

/// Parses long patterns on a reusable large-stack worker; failures reject.
fn parse_on_owned_stack(pattern: &str) -> bool {
    parse_with_regex_worker(pattern, regex_worker())
}

fn regex_worker() -> Option<&'static mpsc::SyncSender<RegexParseRequest>> {
    REGEX_WORKER.get_or_init(start_regex_worker).as_ref()
}

fn start_regex_worker() -> Option<mpsc::SyncSender<RegexParseRequest>> {
    let (sender, receiver) = mpsc::sync_channel::<RegexParseRequest>(0);
    std::thread::Builder::new()
        .name("gts-regex-parser".to_owned())
        .stack_size(REGEX_PARSE_STACK)
        .spawn(move || {
            while let Ok(request) = receiver.recv() {
                let valid =
                    std::panic::catch_unwind(|| regress::Regex::new(&request.pattern).is_ok())
                        .unwrap_or(false);
                let _ = request.response.send(valid);
            }
        })
        .ok()
        .map(|_| sender)
}

fn parse_with_regex_worker(
    pattern: &str,
    worker: Option<&mpsc::SyncSender<RegexParseRequest>>,
) -> bool {
    let Some(worker) = worker else {
        return false;
    };
    let (response, result) = mpsc::sync_channel(1);
    if worker
        .send(RegexParseRequest {
            pattern: pattern.to_owned(),
            response,
        })
        .is_err()
    {
        return false;
    }
    result.recv().unwrap_or(false)
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
    fn regex_format_accepts_ecma_only_constructs() {
        for pattern in [r"(?=a)a", r"(?!a)b", r"(a)\1", r"(?<n>a)\k<n>", r"(?<=a)b"] {
            assert!(is_valid_ecma262_regex(pattern), "expected valid: {pattern}");
        }
    }

    #[test]
    fn regex_format_rejects_rust_only_constructs() {
        for pattern in [r"(?P<n>a)", r"(?i)a", r"^*", r"(?x) a"] {
            assert!(
                !is_valid_ecma262_regex(pattern),
                "expected invalid: {pattern}"
            );
        }
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
    fn regex_format_rejects_a_pattern_nested_past_the_guard() {
        let bomb = format!("{}a{}", "(?:".repeat(10_000), ")".repeat(10_000));
        assert!(!is_valid_ecma262_regex(&bomb));

        let modest = format!("{}a{}", "(?:".repeat(20), ")".repeat(20));
        assert!(is_valid_ecma262_regex(&modest));
    }

    #[test]
    fn regex_format_accepts_duplicate_named_groups_across_alternatives() {
        assert!(is_valid_ecma262_regex(r"(?<a>x)|(?<a>y)"));
        assert!(!is_valid_ecma262_regex(r"(?<a>x)(?<a>y)"));
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
    fn regex_format_survives_a_flat_alternation_bomb() {
        let bomb = vec!["a"; 10_000].join("|");
        assert!(
            bomb.len() > REGEX_INLINE_LEN,
            "must take the owned-stack path"
        );
        assert!(
            is_valid_ecma262_regex(&bomb),
            "an alternation of literals is a valid pattern"
        );
    }

    #[test]
    fn regex_format_rejects_a_pattern_past_the_size_cap() {
        let oversized = "a".repeat(MAX_REGEX_LEN + 1);
        assert!(!is_valid_ecma262_regex(&oversized));
    }

    #[test]
    fn regex_worker_unavailable_fails_closed() {
        assert!(!parse_with_regex_worker("valid", None));
    }

    #[test]
    fn regex_worker_disconnect_fails_closed() {
        let (worker, receiver) = std::sync::mpsc::sync_channel(0);
        drop(receiver);

        assert!(!parse_with_regex_worker("valid", Some(&worker)));
    }

    #[test]
    fn regex_format_parses_a_large_but_permitted_pattern() {
        let wide = vec!["ab"; 400].join("|");
        assert!(wide.len() > REGEX_INLINE_LEN && wide.len() <= MAX_REGEX_LEN);
        assert!(is_valid_ecma262_regex(&wide));

        let wide_invalid = format!("{wide}|(");
        assert!(!is_valid_ecma262_regex(&wide_invalid));
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
    fn regex_format_accepts_ecma262_patterns() {
        for pattern in [
            r"^[A-Za-z0-9]+$",
            r"\d{3}-\d{4}",
            r"(foo|bar)+",
            r"[a-z]{1,3}",
            r"^(https?):\/\/",
            r"a.*?b",
            r"(a(b)?c)*",
            r"[\s\S]*",
            r"\(\d+\)",
            r"(a*)*",
            r"a+?",
        ] {
            assert!(is_valid_ecma262_regex(pattern), "expected valid: {pattern}");
        }
    }

    #[test]
    fn regex_format_rejects_invalid_ecma262_patterns() {
        for pattern in [
            "[unclosed",
            "(unclosed",
            "a{3,2}",
            r"\",
            "*abc",
            "a)",
            "a**",
            "a+*",
            "a??*",
        ] {
            assert!(
                !is_valid_ecma262_regex(pattern),
                "expected invalid: {pattern}"
            );
        }
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
    fn draft7_schema_asserts_ecma262_regex_format() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "string",
            "format": "regex"
        });
        let validator = validator_for(&schema).expect("schema compiles");
        assert!(validator.is_valid(&serde_json::json!("^[A-Za-z0-9]+$")));
        assert!(!validator.is_valid(&serde_json::json!("a**")));
    }
}
