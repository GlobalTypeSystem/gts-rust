use serde_json::Value;

use crate::schema_traits::{X_GTS_TRAITS, X_GTS_TRAITS_SCHEMA};

pub const X_GTS_FINAL: &str = "x-gts-final";
pub const X_GTS_ABSTRACT: &str = "x-gts-abstract";

/// All `x-gts-*` keywords that are valid only on JSON Schema documents and
/// MUST be rejected when found inside instance documents (GTS spec § 9.7.1,
/// § 9.11.1).
const SCHEMA_ONLY_KEYWORDS: &[&str] = &[
    X_GTS_FINAL,
    X_GTS_ABSTRACT,
    X_GTS_TRAITS_SCHEMA,
    X_GTS_TRAITS,
];

fn contains_key_recursive(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            if map.contains_key(key) {
                return true;
            }
            map.values().any(|v| contains_key_recursive(v, key))
        }
        Value::Array(arr) => arr.iter().any(|v| contains_key_recursive(v, key)),
        _ => false,
    }
}

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

    if let Value::Object(map) = content {
        for (k, v) in map {
            if k == X_GTS_FINAL || k == X_GTS_ABSTRACT {
                continue;
            }
            if contains_key_recursive(v, X_GTS_FINAL) {
                return Err(format!("{X_GTS_FINAL} must be at the schema top level"));
            }
            if contains_key_recursive(v, X_GTS_ABSTRACT) {
                return Err(format!("{X_GTS_ABSTRACT} must be at the schema top level"));
            }
        }
    }

    Ok(())
}

/// Validate that `x-gts-traits` and `x-gts-traits-schema` appear only at the
/// schema document top level (GTS spec § 9.7.1/§9.11).
///
/// Like the modifier placement rule, these are type-level keywords describing
/// the GTS Type as a whole; nesting either inside a subschema (`allOf`,
/// `properties`, `$defs`, combinators, `items`, …) is a misplacement and is
/// rejected (fail fast) rather than silently ignored.
///
/// The rule constrains only the *position* of the keyword, not the *contents*
/// of the two top-level trait keywords, so neither value is re-scanned — for
/// distinct reasons:
///
/// - `x-gts-traits-schema` is an ordinary JSON Schema subschema whose body may
///   legitimately carry `x-gts-*` members (e.g. when an existing GTS type is
///   reused as a trait-schema source via `$ref`); §9.7.1 explicitly exempts its
///   contents.
/// - `x-gts-traits` is a plain JSON object of trait *values* matched against
///   the effective trait-schema. A member that happens to be keyed
///   `x-gts-traits` / `x-gts-traits-schema` inside it is ordinary data, not a
///   misplaced keyword — just as instance data may contain any key. There is no
///   subschema there for a keyword to be "misplaced" in.
///
/// # Errors
/// Returns an error describing the first misplaced keyword found.
pub fn validate_trait_placement(content: &Value) -> Result<(), String> {
    if let Value::Object(map) = content {
        for (k, v) in map {
            // The four document-level keyword slots are allowed at the top
            // level; their own values are not re-scanned (see doc comment).
            if k == X_GTS_FINAL
                || k == X_GTS_ABSTRACT
                || k == X_GTS_TRAITS
                || k == X_GTS_TRAITS_SCHEMA
            {
                continue;
            }
            if contains_key_recursive(v, X_GTS_TRAITS_SCHEMA) {
                return Err(format!(
                    "{X_GTS_TRAITS_SCHEMA} must be at the schema top level"
                ));
            }
            if contains_key_recursive(v, X_GTS_TRAITS) {
                return Err(format!("{X_GTS_TRAITS} must be at the schema top level"));
            }
        }
    }

    Ok(())
}

/// Validate every GTS extension keyword carried by a *schema* document — both
/// format and placement:
/// - `x-gts-final` / `x-gts-abstract`: boolean, mutually exclusive, top-level
///   only ([`validate_schema_modifiers`]);
/// - `x-gts-traits` / `x-gts-traits-schema`: top-level only
///   ([`validate_trait_placement`]).
///
/// Pure structural check on raw content (no `$ref` resolution), so it is the
/// natural companion to ref validation: both gate a schema before any
/// resolution or cross-schema work. The single entry point used by both the
/// ingest path and [`crate::store::GtsStore::validate_schema`].
///
/// # Errors
/// Returns the human-readable reason the first malformed or misplaced keyword
/// fails.
pub fn validate_gts_keywords(content: &Value) -> Result<(), String> {
    validate_schema_modifiers(content)?;
    validate_trait_placement(content)?;
    Ok(())
}

/// Check that schema-only keywords (`x-gts-final`, `x-gts-abstract`,
/// `x-gts-traits-schema`, `x-gts-traits`) do not appear anywhere in instance
/// content. Per GTS spec § 9.7.1 and § 9.11.1 these annotations are only
/// valid on JSON Schema documents and implementations MUST reject instances
/// that contain them.
///
/// # Errors
/// Returns an error naming the first schema-only keyword found in the content
/// (top-level or nested).
pub fn validate_instance_modifiers(content: &Value) -> Result<(), String> {
    for keyword in SCHEMA_ONLY_KEYWORDS {
        if contains_key_recursive(content, keyword) {
            return Err(format!(
                "{keyword} is a schema-only keyword and must not appear in instances"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

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

    // =========================================================================
    // validate_instance_modifiers unit tests
    // =========================================================================

    #[test]
    fn test_instance_clean() {
        assert!(validate_instance_modifiers(&json!({"id": "test", "name": "foo"})).is_ok());
    }

    #[test]
    fn test_instance_has_final() {
        let result = validate_instance_modifiers(&json!({"id": "test", "x-gts-final": true}));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-final"));
    }

    #[test]
    fn test_instance_has_abstract() {
        let result = validate_instance_modifiers(&json!({"id": "test", "x-gts-abstract": true}));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-abstract"));
    }

    #[test]
    fn test_instance_nested_final_rejected() {
        let result = validate_instance_modifiers(&json!({
            "id": "test",
            "metadata": {"flags": {"x-gts-final": true}},
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-final"));
    }

    #[test]
    fn test_instance_nested_abstract_in_array_rejected() {
        let result = validate_instance_modifiers(&json!({
            "id": "test",
            "items": [
                {"name": "ok"},
                {"name": "bad", "x-gts-abstract": true},
            ],
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-abstract"));
    }

    #[test]
    fn test_instance_has_traits_rejected() {
        let result = validate_instance_modifiers(&json!({
            "id": "test",
            "x-gts-traits": {"retention": "P30D"},
        }));
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("x-gts-traits"), "msg: {msg}");
        assert!(msg.contains("schema-only"), "msg: {msg}");
    }

    #[test]
    fn test_instance_has_traits_schema_rejected() {
        let result = validate_instance_modifiers(&json!({
            "id": "test",
            "x-gts-traits-schema": {
                "type": "object",
                "properties": {"retention": {"type": "string"}},
            },
        }));
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("x-gts-traits-schema"), "msg: {msg}");
    }

    #[test]
    fn test_instance_nested_traits_rejected() {
        let result = validate_instance_modifiers(&json!({
            "id": "test",
            "metadata": {"x-gts-traits": {"foo": "bar"}},
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("x-gts-traits"));
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
            if id.starts_with("gts://") {
                content
            } else {
                let mut c = content.as_object().unwrap().clone();
                c.insert("$id".to_owned(), json!(format!("gts://{id}")));
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
                    {"$ref": "gts.x.testmod.abs.concinst.v1~"},
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
                    {"$ref": "gts.x.testmod.abs.chain.v1~"},
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
                    {"$ref": "gts.x.testmod.abs.chain.v1~x.testmod._.mid.v1~"},
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
                    {"$ref": "gts.x.testmod.absfinal.base.v1~"},
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
                    {"$ref": "gts.x.testmod.absfinal.base.v1~x.testmod._.concrete.v1~"},
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
