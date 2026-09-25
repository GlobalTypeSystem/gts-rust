use jsonschema::Draft;
use serde_json::{Map, Value};

use crate::schema_traits::{X_GTS_TRAITS, X_GTS_TRAITS_SCHEMA};

/// GTS reference keyword.
pub(crate) const X_GTS_REF: &str = "x-gts-ref";

pub const X_GTS_FINAL: &str = "x-gts-final";
pub const X_GTS_ABSTRACT: &str = "x-gts-abstract";

/// Validate `x-gts-final` and `x-gts-abstract` on a schema:
/// - both must be booleans,
/// - they are mutually exclusive when both true,
/// - they must appear only at the schema top level — anywhere nested
///   (inside `allOf`, `properties`, `$defs`, `items`, combinators, etc.) is rejected.
///
/// # Errors
/// Returns an error describing the first failed check.
pub fn validate_schema_modifiers(content: &Value) -> Result<(), String> {
    let is_final = match content.get(X_GTS_FINAL) {
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(format!("{X_GTS_FINAL} must be a boolean, got {other}")),
        None => false,
    };

    let is_abstract = match content.get(X_GTS_ABSTRACT) {
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(format!("{X_GTS_ABSTRACT} must be a boolean, got {other}")),
        None => false,
    };

    if is_final && is_abstract {
        return Err(format!(
            "schema cannot declare both {X_GTS_FINAL} and {X_GTS_ABSTRACT} as true"
        ));
    }

    visit_schema_nodes(content, "", EnterTraitSchema::Yes, &mut |map, path, _| {
        if path.is_empty() {
            return Ok(());
        }
        if map.contains_key(X_GTS_FINAL) {
            return Err(format!("{X_GTS_FINAL} must be at the schema top level"));
        }
        if map.contains_key(X_GTS_ABSTRACT) {
            return Err(format!("{X_GTS_ABSTRACT} must be at the schema top level"));
        }
        Ok(())
    })
}

/// Validate that `x-gts-traits` and `x-gts-traits-schema` appear only at the
/// schema document top level (GTS spec § 9.7.1/§9.11).
///
/// Top-level trait values and their schema contents are exempt.
///
/// # Errors
/// Returns an error describing the first misplaced keyword found.
pub fn validate_trait_placement(content: &Value) -> Result<(), String> {
    visit_schema_nodes(content, "", EnterTraitSchema::No, &mut |map, path, _| {
        if path.is_empty() {
            return Ok(());
        }
        if map.contains_key(X_GTS_TRAITS_SCHEMA) {
            return Err(format!(
                "{X_GTS_TRAITS_SCHEMA} must be at the schema top level"
            ));
        }
        if map.contains_key(X_GTS_TRAITS) {
            return Err(format!("{X_GTS_TRAITS} must be at the schema top level"));
        }
        Ok(())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnterTraitSchema {
    Yes,
    No,
}

/// Dialect subschemas, optionally including the GTS trait schema.
fn direct_subresources(
    node: &Value,
    draft: Draft,
    enter_trait_schema: EnterTraitSchema,
) -> Vec<&Value> {
    let mut subresources = draft.subresources_of(node).collect::<Vec<_>>();

    if enter_trait_schema == EnterTraitSchema::Yes
        && let Some(trait_schema) = node.get(X_GTS_TRAITS_SCHEMA)
    {
        subresources.push(trait_schema);
    }

    subresources
}

fn is_direct_subresource(value: &Value, subresources: &[&Value]) -> bool {
    // Equality is insufficient: identical JSON may also occur in literal data.
    subresources
        .iter()
        .any(|subresource| std::ptr::eq(*subresource, value))
}

/// Supported GTS extension keywords.
const KNOWN_GTS_KEYWORDS: &[&str] = &[
    X_GTS_FINAL,
    X_GTS_ABSTRACT,
    X_GTS_TRAITS,
    X_GTS_TRAITS_SCHEMA,
    X_GTS_REF,
];

/// Where a schema node sits: the dialect it is read under, and the schema
/// resource a same-document reference (`#...`) in it resolves from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SchemaScope<'a> {
    /// The dialect in effect at the node.
    pub(crate) dialect: Draft,
    /// The root of the innermost schema resource holding the node: the
    /// document, or a subschema whose `$id` the dialect honours.
    pub(crate) resource: &'a Value,
    /// The dialect in effect at `resource`.
    pub(crate) resource_dialect: Draft,
}

impl<'a> SchemaScope<'a> {
    fn document(root: &'a Value) -> Self {
        let dialect = Draft::default().detect(root);
        Self {
            dialect,
            resource: root,
            resource_dialect: dialect,
        }
    }

    /// The scope of `subschema`, reached from a node in this scope.
    ///
    /// Draft-07 ignores an `$id` beside `$ref` and treats `#name` as an
    /// anchor; neither starts a resource there.
    fn enter(self, subschema: &'a Value) -> Self {
        let dialect = self.dialect.detect(subschema);
        if dialect.create_resource_ref(subschema).id().is_some() {
            Self {
                dialect,
                resource: subschema,
                resource_dialect: dialect,
            }
        } else {
            Self { dialect, ..self }
        }
    }
}

type SchemaNodeVisitor<'a> =
    dyn FnMut(&serde_json::Map<String, Value>, &str, SchemaScope<'_>) -> Result<(), String> + 'a;
type SchemaNodePredicate<'a> = dyn FnMut(&Map<String, Value>) -> bool + 'a;
type SchemaNodeWalker<'a> = dyn FnMut(&Map<String, Value>, &str) + 'a;
type ScopedSchemaNodeWalker<'a> = dyn FnMut(&Map<String, Value>, &str, SchemaScope<'_>) + 'a;

/// Visits the document and dialect-defined subschemas, excluding annotation data.
fn visit_schema_nodes(
    node: &Value,
    path: &str,
    enter_trait_schema: EnterTraitSchema,
    visit: &mut SchemaNodeVisitor<'_>,
) -> Result<(), String> {
    visit_schema_nodes_in_scope(
        node,
        path,
        SchemaScope::document(node),
        enter_trait_schema,
        visit,
    )
}

fn visit_schema_nodes_in_scope<'a>(
    node: &'a Value,
    path: &str,
    scope: SchemaScope<'a>,
    enter_trait_schema: EnterTraitSchema,
    visit: &mut SchemaNodeVisitor<'_>,
) -> Result<(), String> {
    let Value::Object(map) = node else {
        return Ok(());
    };

    visit(map, path, scope)?;

    let subresources = direct_subresources(node, scope.dialect, enter_trait_schema);
    visit_direct_subresources(node, path, &subresources, scope, enter_trait_schema, visit)
}

/// Visits schema nodes with their path (`""` for the root).
pub(crate) fn for_each_schema_node(node: &Value, visit: &mut SchemaNodeWalker<'_>) {
    for_each_schema_node_in_scope(node, &mut |map, path, _| visit(map, path));
}

/// Visits schema nodes with their path and their [`SchemaScope`].
pub(crate) fn for_each_schema_node_in_scope(node: &Value, visit: &mut ScopedSchemaNodeWalker<'_>) {
    let result = visit_schema_nodes(node, "", EnterTraitSchema::Yes, &mut |map, path, scope| {
        visit(map, path, scope);
        Ok(())
    });
    debug_assert!(result.is_ok());
}

/// The subschemas of `document` that start a schema resource of their own,
/// by identity: those whose `$id` the dialect honours (see [`SchemaScope`]).
///
/// A same-document reference (`#...`) inside one resolves from it rather than
/// from `document`.
pub(crate) fn embedded_resources(
    document: &Value,
) -> std::collections::HashSet<*const Map<String, Value>> {
    let mut found = std::collections::HashSet::new();
    if !has_nested_id(document) {
        return found;
    }
    for_each_schema_node_in_scope(document, &mut |node, _, scope| {
        let starts_here = scope
            .resource
            .as_object()
            .is_some_and(|resource| std::ptr::eq(resource, node));
        if starts_here && !std::ptr::eq(scope.resource, document) {
            found.insert(std::ptr::from_ref(node));
        }
    });
    found
}

/// Whether any object below `document`'s root declares an `$id`: only then
/// can a resource be embedded in it.
fn has_nested_id(document: &Value) -> bool {
    let children: Box<dyn Iterator<Item = &Value>> = match document {
        Value::Object(map) => Box::new(map.values()),
        Value::Array(items) => Box::new(items.iter()),
        _ => return false,
    };
    children
        .into_iter()
        .any(|child| child.get("$id").is_some() || has_nested_id(child))
}

/// Whether `node` contains a local JSON Pointer reference.
fn is_local_ref(node: &Map<String, Value>) -> bool {
    node.get("$ref")
        .and_then(Value::as_str)
        .is_some_and(|target| target == "#" || target.starts_with("#/"))
}

/// Whether a schema position contains a local JSON Pointer reference.
pub(crate) fn contains_local_ref(schema: &Value) -> bool {
    any_schema_node(schema, &mut is_local_ref)
}

/// Whether any schema node satisfies `predicate`.
pub(crate) fn any_schema_node(node: &Value, predicate: &mut SchemaNodePredicate<'_>) -> bool {
    let mut found = false;
    let result = visit_schema_nodes(node, "", EnterTraitSchema::Yes, &mut |map, _, _| {
        if !found {
            found = predicate(map);
        }
        Ok(())
    });
    debug_assert!(result.is_ok());
    found
}

fn visit_direct_subresources<'a>(
    value: &'a Value,
    path: &str,
    subresources: &[&Value],
    scope: SchemaScope<'a>,
    enter_trait_schema: EnterTraitSchema,
    visit: &mut SchemaNodeVisitor<'_>,
) -> Result<(), String> {
    if is_direct_subresource(value, subresources) {
        return visit_schema_nodes_in_scope(
            value,
            path,
            scope.enter(value),
            enter_trait_schema,
            visit,
        );
    }

    match value {
        Value::Object(object) => {
            for (key, child) in object {
                visit_direct_subresources(
                    child,
                    &extend_path(path, key),
                    subresources,
                    scope,
                    enter_trait_schema,
                    visit,
                )?;
            }
        }
        Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                visit_direct_subresources(
                    child,
                    &format!("{path}[{index}]"),
                    subresources,
                    scope,
                    enter_trait_schema,
                    visit,
                )?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn extend_path(path: &str, segment: &str) -> String {
    if path.is_empty() {
        segment.to_owned()
    } else {
        format!("{path}/{segment}")
    }
}

/// Rejects unknown `x-gts-*` keywords in schema positions.
///
/// # Errors
/// Returns an error naming the first unrecognised keyword and its location.
fn validate_known_gts_keywords(content: &Value) -> Result<(), String> {
    visit_schema_nodes(content, "", EnterTraitSchema::Yes, &mut |map, path, _| {
        for key in map.keys() {
            if key.starts_with("x-gts-") && !KNOWN_GTS_KEYWORDS.contains(&key.as_str()) {
                let location = if path.is_empty() {
                    "the schema top level".to_owned()
                } else {
                    format!("'{path}'")
                };
                return Err(format!(
                    "unknown GTS extension keyword '{key}' at {location}; \
                     the x-gts- namespace is reserved"
                ));
            }
        }
        Ok(())
    })
}

/// Structurally validates GTS extension keywords without resolving references.
///
/// # Errors
/// Returns the human-readable reason the first malformed or misplaced keyword
/// fails.
pub fn validate_gts_keywords(content: &Value) -> Result<(), String> {
    validate_known_gts_keywords(content)?;
    validate_schema_modifiers(content)?;
    validate_trait_placement(content)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_known_keywords_accepted() {
        assert!(
            validate_known_gts_keywords(&json!({
                "x-gts-final": true,
                "type": "object",
                "properties": {"ref": {"type": "string", "x-gts-ref": "gts.x.a.b.c.v1~"}},
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_unknown_keyword_at_top_level_rejected() {
        let err = validate_known_gts_keywords(&json!({
            "type": "object",
            "x-gts-bogus": true,
        }))
        .unwrap_err();
        assert!(err.contains("x-gts-bogus"), "{err}");
    }

    #[test]
    fn test_unknown_keyword_inside_property_subschema_rejected() {
        let err = validate_known_gts_keywords(&json!({
            "type": "object",
            "properties": {"widget": {"type": "string", "x-gts-widget": "dropdown"}},
        }))
        .unwrap_err();
        assert!(err.contains("x-gts-widget"), "{err}");
        assert!(err.contains("properties/widget"), "{err}");
    }

    #[test]
    fn test_unknown_keyword_inside_definitions_rejected() {
        let err = validate_known_gts_keywords(&json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "definitions": {"Sub": {"type": "object", "x-gts-experimental": true}},
        }))
        .unwrap_err();
        assert!(err.contains("x-gts-experimental"), "{err}");
    }

    #[test]
    fn test_unknown_keyword_inside_all_of_rejected() {
        let err = validate_known_gts_keywords(&json!({
            "allOf": [
                {"$ref": "gts://gts.x.a.b.c.v1~"},
                {"type": "object", "x-gts-policy": "strict"},
            ],
        }))
        .unwrap_err();
        assert!(err.contains("x-gts-policy"), "{err}");
        assert!(err.contains("allOf[1]"), "{err}");
    }

    #[test]
    fn test_near_miss_typos_rejected() {
        for typo in ["x-gts-trait", "x-gts-refs", "x-gts-traits-schemas"] {
            let content = json!({"type": "object", typo: {}});
            assert!(
                validate_known_gts_keywords(&content).is_err(),
                "expected {typo} to be rejected"
            );
        }
    }

    #[test]
    fn test_dependency_property_names_are_not_keywords() {
        assert!(
            validate_known_gts_keywords(&json!({
                "type": "object",
                "dependencies": {"x-gts-widget": ["other"]},
            }))
            .is_ok()
        );
        assert!(
            validate_known_gts_keywords(&json!({
                "type": "object",
                "dependencies": {"x-gts-widget": {"required": ["other"]}},
            }))
            .is_ok()
        );
        assert!(
            validate_known_gts_keywords(&json!({
                "type": "object",
                "dependentRequired": {"x-gts-widget": ["other"]},
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_unknown_keyword_inside_a_dependency_subschema_is_rejected() {
        let err = validate_known_gts_keywords(&json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "dependencies": {"trigger": {"type": "object", "x-gts-bogus": 1}},
        }))
        .unwrap_err();
        assert!(err.contains("x-gts-bogus"), "{err}");
    }

    #[test]
    fn test_vocabulary_uris_are_not_keywords() {
        assert!(
            validate_known_gts_keywords(&json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "$vocabulary": {"x-gts-custom:example": false},
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_property_named_like_a_keyword_is_not_a_keyword() {
        assert!(
            validate_known_gts_keywords(&json!({
                "type": "object",
                "properties": {"x-gts-widget": {"type": "string"}},
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_trait_values_are_data_not_keywords() {
        assert!(
            validate_known_gts_keywords(&json!({
                "x-gts-traits": {"x-gts-anything": "is just data"},
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_unknown_keyword_inside_trait_schema_rejected() {
        let err = validate_known_gts_keywords(&json!({
            "x-gts-traits-schema": {
                "type": "object",
                "properties": {"topic": {"type": "string", "x-gts-bogus": 1}},
            },
        }))
        .unwrap_err();
        assert!(err.contains("x-gts-bogus"), "{err}");
    }

    #[test]
    fn test_enum_and_const_values_are_data() {
        assert!(
            validate_known_gts_keywords(&json!({
                "const": {"x-gts-final": "data"},
                "enum": [{"x-gts-abstract": "also data"}],
            }))
            .is_ok()
        );
    }

    // =========================================================================
    // validate_schema_modifiers unit tests
    // =========================================================================

    #[test]
    fn test_default() {
        assert!(validate_schema_modifiers(&json!({"type": "object"})).is_ok());
    }

    #[test]
    fn test_final_true() {
        assert!(validate_schema_modifiers(&json!({"x-gts-final": true})).is_ok());
    }

    #[test]
    fn test_abstract_true() {
        assert!(validate_schema_modifiers(&json!({"x-gts-abstract": true})).is_ok());
    }

    #[test]
    fn test_both_true_error() {
        let result = validate_schema_modifiers(&json!({
            "x-gts-final": true,
            "x-gts-abstract": true,
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_non_boolean_final() {
        let result = validate_schema_modifiers(&json!({"x-gts-final": "yes"}));
        assert!(result.is_err());
    }

    #[test]
    fn test_non_boolean_abstract() {
        let result = validate_schema_modifiers(&json!({"x-gts-abstract": 1}));
        assert!(result.is_err());
    }

    #[test]
    fn test_false_is_noop() {
        assert!(
            validate_schema_modifiers(&json!({
                "x-gts-final": false,
                "x-gts-abstract": false,
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_final_inside_allof_rejected() {
        let result = validate_schema_modifiers(&json!({
            "type": "object",
            "allOf": [
                {"$ref": "gts.x.foo.base.v1~"},
                {"x-gts-final": true},
            ],
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-final"));
    }

    #[test]
    fn test_abstract_inside_allof_rejected() {
        let result = validate_schema_modifiers(&json!({
            "type": "object",
            "allOf": [
                {"$ref": "gts.x.foo.base.v1~"},
                {"x-gts-abstract": true},
            ],
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-abstract"));
    }

    #[test]
    fn test_top_level_with_allof_ok() {
        assert!(
            validate_schema_modifiers(&json!({
                "type": "object",
                "x-gts-final": true,
                "allOf": [
                    {"$ref": "gts.x.foo.base.v1~"},
                    {"type": "object"},
                ],
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_final_inside_properties_rejected() {
        let result = validate_schema_modifiers(&json!({
            "type": "object",
            "properties": {
                "foo": {"type": "string", "x-gts-final": true},
            },
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-final"));
    }

    #[test]
    fn test_abstract_inside_defs_rejected() {
        let result = validate_schema_modifiers(&json!({
            "type": "object",
            "$defs": {
                "Inner": {"type": "object", "x-gts-abstract": true},
            },
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-abstract"));
    }

    #[test]
    fn test_final_inside_oneof_rejected() {
        let result = validate_schema_modifiers(&json!({
            "type": "object",
            "oneOf": [
                {"type": "object"},
                {"type": "object", "x-gts-final": true},
            ],
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-final"));
    }

    #[test]
    fn test_abstract_inside_items_rejected() {
        let result = validate_schema_modifiers(&json!({
            "type": "array",
            "items": {"type": "object", "x-gts-abstract": true},
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-abstract"));
    }

    // =========================================================================
    // validate_trait_placement unit tests (GTS spec §9.7.1/§9.11)
    // =========================================================================

    #[test]
    fn test_traits_top_level_ok() {
        assert!(
            validate_trait_placement(&json!({
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "object",
                    "properties": {"topicRef": {"type": "string"}}
                },
                "x-gts-traits": {"topicRef": "events.orders"},
                "allOf": [{"$ref": "gts.x.foo.base.v1~"}]
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_traits_inside_allof_rejected() {
        let result = validate_trait_placement(&json!({
            "type": "object",
            "allOf": [
                {"$ref": "gts.x.foo.base.v1~"},
                {"type": "object", "x-gts-traits": {"topicRef": "events.orders"}},
            ],
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-traits"));
    }

    #[test]
    fn test_traits_schema_inside_allof_rejected() {
        let result = validate_trait_placement(&json!({
            "type": "object",
            "allOf": [
                {"$ref": "gts.x.foo.base.v1~"},
                {
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {"auditRetention": {"type": "string"}}
                    }
                },
            ],
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-traits-schema"));
    }

    #[test]
    fn test_traits_inside_properties_rejected() {
        let result = validate_trait_placement(&json!({
            "type": "object",
            "properties": {
                "nested": {"type": "object", "x-gts-traits": {"topicRef": "x"}},
            },
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-traits"));
    }

    #[test]
    fn test_traits_schema_inside_defs_rejected() {
        let result = validate_trait_placement(&json!({
            "type": "object",
            "$defs": {
                "Sub": {"type": "object", "x-gts-traits-schema": {"type": "object"}},
            },
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-traits-schema"));
    }

    #[test]
    fn test_x_gts_keys_inside_trait_values_tolerated() {
        // The top-level x-gts-traits holds trait *values* matched against the
        // effective trait-schema, not a subschema. A member keyed
        // `x-gts-traits` nested inside those values is ordinary data, not a
        // misplaced keyword, and must NOT be flagged (§9.7.1 scope clause).
        assert!(
            validate_trait_placement(&json!({
                "type": "object",
                "x-gts-traits": {
                    "nested": {"x-gts-traits": {}}
                }
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_x_gts_keys_inside_trait_schema_value_tolerated() {
        // The contents of the top-level x-gts-traits-schema are an ordinary
        // JSON Schema subschema and may carry x-gts-* members (e.g. a $ref-
        // reused GTS type). The placement rule constrains only the keyword's
        // position, so nested x-gts-traits / x-gts-traits-schema *inside* the
        // top-level trait-schema value must NOT be flagged (§9.7.1 scope clause).
        assert!(
            validate_trait_placement(&json!({
                "type": "object",
                "x-gts-traits-schema": {
                    "type": "object",
                    "x-gts-traits-schema": {"type": "object"},
                    "x-gts-traits": {"foo": "bar"},
                    "properties": {"retention": {"type": "string"}}
                },
                "x-gts-traits": {"retention": "P30D"}
            }))
            .is_ok()
        );
    }

    #[test]
    fn test_vendor_annotation_contents_are_not_keywords() {
        assert_eq!(
            validate_gts_keywords(&json!({
                "type": "object",
                "x-ui": {"x-gts-widget": "text"},
                "properties": {"a": {"type": "string"}}
            })),
            Ok(())
        );

        assert_eq!(
            validate_gts_keywords(&json!({
                "type": "object",
                "x-vendor": [{"x-gts-bogus": 1}]
            })),
            Ok(())
        );
    }

    #[test]
    fn test_dialect_specific_keywords_only_contain_subschemas_in_their_dialect() {
        let draft7 = "http://json-schema.org/draft-07/schema#";
        let draft2020 = "https://json-schema.org/draft/2020-12/schema";

        assert!(
            validate_gts_keywords(&json!({
                "$schema": draft7,
                "prefixItems": [{"x-gts-bogus": "annotation data"}]
            }))
            .is_ok()
        );
        assert!(
            validate_gts_keywords(&json!({
                "$schema": draft7,
                "dependencies": {"name": {"x-gts-bogus": true}}
            }))
            .is_err()
        );

        assert!(
            validate_gts_keywords(&json!({
                "$schema": draft2020,
                "dependencies": {"name": {"x-gts-bogus": "annotation data"}}
            }))
            .is_ok()
        );
        assert!(
            validate_gts_keywords(&json!({
                "$schema": draft2020,
                "prefixItems": [{"x-gts-bogus": true}]
            }))
            .is_err()
        );
    }

    #[test]
    fn test_draft4_and_draft6_subschema_locations_are_supported() {
        for dialect in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
        ] {
            assert!(
                validate_gts_keywords(&json!({
                    "$schema": dialect,
                    "definitions": {"name": {"x-gts-bogus": true}}
                }))
                .is_err(),
                "definitions contains subschemas in {dialect}"
            );
        }

        assert!(
            validate_gts_keywords(&json!({
                "$schema": "http://json-schema.org/draft-06/schema#",
                "prefixItems": [{"x-gts-bogus": "annotation data"}]
            }))
            .is_ok(),
            "prefixItems is not a subschema container in Draft 6"
        );
    }

    #[test]
    fn test_known_gts_keyword_names_inside_annotations_are_data() {
        for document in [
            json!({"type": "object", "x-ui": {"x-gts-final": true}}),
            json!({"type": "object", "x-ui": {"x-gts-abstract": true}}),
            json!({"type": "object", "x-ui": {"x-gts-traits": {"retention": "P30D"}}}),
            json!({"type": "object", "x-ui": {"x-gts-traits-schema": {"type": "object"}}}),
            json!({"type": "object", "default": {"x-gts-final": true}}),
            json!({"type": "object", "examples": [{"x-gts-traits": {}}]}),
        ] {
            assert_eq!(validate_gts_keywords(&document), Ok(()), "{document}");
        }
    }

    #[test]
    fn test_modifiers_in_real_subschema_positions_are_still_rejected() {
        for document in [
            json!({"if": {"x-gts-final": true}}),
            json!({"then": {"x-gts-abstract": true}}),
            json!({"else": {"x-gts-final": true}}),
            json!({"not": {"x-gts-abstract": true}}),
            json!({"contains": {"x-gts-final": true}}),
            json!({"propertyNames": {"x-gts-abstract": true}}),
            json!({"additionalProperties": {"x-gts-final": true}}),
            json!({"unevaluatedProperties": {"x-gts-abstract": true}}),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "items": [{"x-gts-final": true}]
            }),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "additionalItems": {"x-gts-abstract": true}
            }),
            json!({"prefixItems": [{"x-gts-final": true}]}),
            json!({"dependentSchemas": {"a": {"x-gts-abstract": true}}}),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "dependencies": {"a": {"x-gts-final": true}}
            }),
        ] {
            assert!(
                validate_gts_keywords(&document).is_err(),
                "a modifier in a subschema position must be rejected: {document}"
            );
        }
    }

    #[test]
    fn test_unknown_keywords_in_real_subschema_positions_are_still_rejected() {
        for document in [
            json!({"if": {"x-gts-bogus": 1}}),
            json!({"not": {"x-gts-bogus": 1}}),
            json!({"contains": {"x-gts-bogus": 1}}),
            json!({"propertyNames": {"x-gts-bogus": 1}}),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "items": [{"x-gts-bogus": 1}]
            }),
            json!({"items": {"x-gts-bogus": 1}}),
            json!({"prefixItems": [{"x-gts-bogus": 1}]}),
            json!({"additionalProperties": {"x-gts-bogus": 1}}),
            json!({"x-gts-traits-schema": {"x-gts-bogus": 1}}),
        ] {
            assert!(
                validate_gts_keywords(&document).is_err(),
                "an unknown keyword in a subschema position must be rejected: {document}"
            );
        }
    }

    #[test]
    fn test_trait_keyword_nested_in_a_subschema_is_still_rejected() {
        assert!(validate_trait_placement(&json!({"if": {"x-gts-traits": {}}})).is_err());
        assert!(
            validate_trait_placement(&json!({"contains": {"x-gts-traits-schema": {}}})).is_err()
        );
    }

    // =========================================================================
    // Integration tests via store
    // =========================================================================

    use crate::entities::{GtsConfig, GtsEntity};
    use crate::store::GtsStore;

    fn default_config() -> GtsConfig {
        GtsConfig::default()
    }

    fn reg_schema(store: &mut GtsStore, content: Value) {
        // Ensure $id has gts:// prefix for entity detection
        let content = if let Some(id) = content.get("$id").and_then(|v| v.as_str()) {
            if id.starts_with(crate::GTS_ID_URI_PREFIX) {
                content
            } else {
                let mut c = content.as_object().unwrap().clone();
                c.insert(
                    "$id".to_owned(),
                    json!(format!("{}{id}", crate::GTS_ID_URI_PREFIX)),
                );
                Value::Object(c)
            }
        } else {
            content
        };
        let cfg = default_config();
        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );
        store.register(entity).expect("register failed");
    }

    fn reg_instance(store: &mut GtsStore, content: &Value) {
        let cfg = default_config();
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );
        store.register(entity).expect("register instance failed");
    }

    // -- x-gts-final tests --

    #[test]
    fn test_final_reject_derived_schema() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.final.base.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-final": true,
                "properties": {"name": {"type": "string"}},
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.final.base.v1~x.testmod._.derived.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts.x.testmod.final.base.v1~"},
                    {"type": "object", "properties": {"extra": {"type": "string"}}},
                ],
            }),
        );
        let result =
            store.validate_schema_chain("gts.x.testmod.final.base.v1~x.testmod._.derived.v1~");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("final"));
    }

    #[test]
    fn test_final_allow_well_known_instance() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.final.inst.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-final": true,
                "required": ["id", "description"],
                "properties": {
                    "id": {"type": "string"},
                    "description": {"type": "string"},
                },
            }),
        );
        reg_instance(
            &mut store,
            &json!({
                "id": "gts.x.testmod.final.inst.v1~x.testmod._.running.v1",
                "description": "Running state",
            }),
        );
        let result = store.validate_instance("gts.x.testmod.final.inst.v1~x.testmod._.running.v1");
        assert!(
            result.is_ok(),
            "expected instance of final type to pass: {result:?}"
        );
    }

    #[test]
    fn test_final_mid_chain() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.finalmid.base.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {"name": {"type": "string"}},
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.finalmid.base.v1~x.testmod._.mid.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-final": true,
                "allOf": [
                    {"$ref": "gts.x.testmod.finalmid.base.v1~"},
                    {"type": "object"},
                ],
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.finalmid.base.v1~x.testmod._.mid.v1~x.testmod._.leaf.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts.x.testmod.finalmid.base.v1~x.testmod._.mid.v1~"},
                    {"type": "object"},
                ],
            }),
        );
        let result = store.validate_schema_chain(
            "gts.x.testmod.finalmid.base.v1~x.testmod._.mid.v1~x.testmod._.leaf.v1~",
        );
        assert!(
            result.is_err(),
            "expected mid-chain final to block derivation"
        );
    }

    #[test]
    fn test_final_sibling_unaffected() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.finalsib.base.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {"name": {"type": "string"}},
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.finalsib.base.v1~x.testmod._.final_b.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-final": true,
                "allOf": [
                    {"$ref": "gts.x.testmod.finalsib.base.v1~"},
                    {"type": "object"},
                ],
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.finalsib.base.v1~x.testmod._.sibling_c.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts.x.testmod.finalsib.base.v1~"},
                    {"type": "object", "properties": {"extra": {"type": "string"}}},
                ],
            }),
        );
        let result =
            store.validate_schema_chain("gts.x.testmod.finalsib.base.v1~x.testmod._.sibling_c.v1~");
        assert!(result.is_ok(), "sibling should pass: {result:?}");
    }

    #[test]
    fn test_final_false_is_noop() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.finalfalse.base.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-final": false,
                "properties": {"name": {"type": "string"}},
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.finalfalse.base.v1~x.testmod._.derived.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts.x.testmod.finalfalse.base.v1~"},
                    {"type": "object"},
                ],
            }),
        );
        let result =
            store.validate_schema_chain("gts.x.testmod.finalfalse.base.v1~x.testmod._.derived.v1~");
        assert!(
            result.is_ok(),
            "final=false should allow derivation: {result:?}"
        );
    }

    // -- x-gts-abstract tests --

    #[test]
    fn test_abstract_reject_direct_instance() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.abs.reject.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-abstract": true,
                "required": ["id", "name"],
                "properties": {
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                },
            }),
        );
        reg_instance(
            &mut store,
            &json!({
                "id": "gts.x.testmod.abs.reject.v1~x.testmod._.item.v1",
                "name": "Direct item",
            }),
        );
        let result = store.validate_instance("gts.x.testmod.abs.reject.v1~x.testmod._.item.v1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("abstract"));
    }

    #[test]
    fn test_abstract_allow_derived_schema() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.abs.derive.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-abstract": true,
                "properties": {"name": {"type": "string"}},
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.abs.derive.v1~x.testmod._.concrete.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts.x.testmod.abs.derive.v1~"},
                    {"type": "object", "properties": {"extra": {"type": "string"}}},
                ],
            }),
        );
        let result =
            store.validate_schema_chain("gts.x.testmod.abs.derive.v1~x.testmod._.concrete.v1~");
        assert!(
            result.is_ok(),
            "derived from abstract should pass: {result:?}"
        );
    }

    #[test]
    fn test_abstract_allow_instance_of_concrete_derived() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.abs.concinst.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-abstract": true,
                "required": ["id", "name"],
                "properties": {
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                },
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.abs.concinst.v1~x.testmod._.concrete.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts://gts.x.testmod.abs.concinst.v1~"},
                    {"type": "object"},
                ],
            }),
        );
        reg_instance(
            &mut store,
            &json!({
                "id": "gts.x.testmod.abs.concinst.v1~x.testmod._.concrete.v1~x.testmod._.item.v1",
                "name": "My Item",
            }),
        );
        let result = store.validate_instance(
            "gts.x.testmod.abs.concinst.v1~x.testmod._.concrete.v1~x.testmod._.item.v1",
        );
        assert!(
            result.is_ok(),
            "instance of concrete derived should pass: {result:?}"
        );
    }

    #[test]
    fn test_abstract_chain_of_abstracts() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.abs.chain.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-abstract": true,
                "required": ["id"],
                "properties": {"id": {"type": "string"}},
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.abs.chain.v1~x.testmod._.mid.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-abstract": true,
                "allOf": [
                    {"$ref": "gts://gts.x.testmod.abs.chain.v1~"},
                    {"type": "object"},
                ],
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.abs.chain.v1~x.testmod._.mid.v1~x.testmod._.leaf.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts://gts.x.testmod.abs.chain.v1~x.testmod._.mid.v1~"},
                    {"type": "object"},
                ],
            }),
        );
        // Instance of concrete leaf — pass
        reg_instance(
            &mut store,
            &json!({
                "id": "gts.x.testmod.abs.chain.v1~x.testmod._.mid.v1~x.testmod._.leaf.v1~x.testmod._.item.v1",
            }),
        );
        assert!(store.validate_instance(
            "gts.x.testmod.abs.chain.v1~x.testmod._.mid.v1~x.testmod._.leaf.v1~x.testmod._.item.v1"
        ).is_ok());
        // Instance of abstract mid — fail
        reg_instance(
            &mut store,
            &json!({
                "id": "gts.x.testmod.abs.chain.v1~x.testmod._.mid.v1~x.testmod._.direct.v1",
            }),
        );
        assert!(
            store
                .validate_instance(
                    "gts.x.testmod.abs.chain.v1~x.testmod._.mid.v1~x.testmod._.direct.v1"
                )
                .is_err()
        );
    }

    #[test]
    fn test_abstract_false_is_noop() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.absfalse.base.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-abstract": false,
                "required": ["id"],
                "properties": {"id": {"type": "string"}},
            }),
        );
        reg_instance(
            &mut store,
            &json!({
                "id": "gts.x.testmod.absfalse.base.v1~x.testmod._.item.v1",
            }),
        );
        assert!(
            store
                .validate_instance("gts.x.testmod.absfalse.base.v1~x.testmod._.item.v1")
                .is_ok()
        );
    }

    // -- Interaction tests --

    #[test]
    fn test_abstract_base_final_derived() {
        let mut store = GtsStore::new();
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.absfinal.base.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-abstract": true,
                "required": ["id", "name"],
                "properties": {
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                },
            }),
        );
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.absfinal.base.v1~x.testmod._.concrete.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "x-gts-final": true,
                "allOf": [
                    {"$ref": "gts://gts.x.testmod.absfinal.base.v1~"},
                    {"type": "object", "properties": {"extra": {"type": "string"}}},
                ],
            }),
        );
        // B chain valid
        assert!(
            store
                .validate_schema_chain("gts.x.testmod.absfinal.base.v1~x.testmod._.concrete.v1~")
                .is_ok()
        );
        // Instance of B — pass
        reg_instance(
            &mut store,
            &json!({
                "id": "gts.x.testmod.absfinal.base.v1~x.testmod._.concrete.v1~x.testmod._.item.v1",
                "name": "My Item", "extra": "value",
            }),
        );
        assert!(
            store
                .validate_instance(
                    "gts.x.testmod.absfinal.base.v1~x.testmod._.concrete.v1~x.testmod._.item.v1"
                )
                .is_ok()
        );
        // Derived from B — fail (B is final)
        reg_schema(
            &mut store,
            json!({
                "$id": "gts.x.testmod.absfinal.base.v1~x.testmod._.concrete.v1~x.testmod._.sub.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts://gts.x.testmod.absfinal.base.v1~x.testmod._.concrete.v1~"},
                    {"type": "object"},
                ],
            }),
        );
        assert!(
            store
                .validate_schema_chain(
                    "gts.x.testmod.absfinal.base.v1~x.testmod._.concrete.v1~x.testmod._.sub.v1~"
                )
                .is_err()
        );
        // Direct instance of A — fail (A is abstract)
        reg_instance(
            &mut store,
            &json!({
                "id": "gts.x.testmod.absfinal.base.v1~x.testmod._.direct.v1",
                "name": "Direct from abstract",
            }),
        );
        assert!(
            store
                .validate_instance("gts.x.testmod.absfinal.base.v1~x.testmod._.direct.v1")
                .is_err()
        );
    }
}
