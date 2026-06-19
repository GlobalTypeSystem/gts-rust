//! Unit tests for [`SchemaResolver`]. They drive the resolver directly against
//! a tiny in-memory [`SchemaProvider`] mock (`MapProvider`) — no `GtsStore`
//! involved — so they exercise `SchemaResolver::resolve` / `try_resolve` in
//! isolation. End-to-end coverage of the `GtsStore` wrappers and provider
//! lookup semantics lives in `store_test.rs`.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{SchemaProvider, SchemaResolver};
use crate::store::StoreError;

/// Minimal [`SchemaProvider`]: a map from canonical type id to schema document.
#[derive(Default)]
struct MapProvider {
    schemas: HashMap<String, Value>,
}

impl MapProvider {
    fn new() -> Self {
        Self::default()
    }

    fn with(mut self, type_id: &str, content: Value) -> Self {
        self.schemas.insert(type_id.to_owned(), content);
        self
    }
}

impl SchemaProvider for MapProvider {
    fn schema_content(&self, type_id: &str) -> Option<&Value> {
        self.schemas.get(type_id)
    }
}

// By-value `json!(...)` literals read cleaner at the call sites.
#[allow(clippy::needless_pass_by_value)]
fn resolve(provider: &MapProvider, schema: Value) -> Value {
    SchemaResolver::new(provider).resolve(&schema)
}

#[allow(clippy::needless_pass_by_value)]
fn try_resolve(provider: &MapProvider, schema: Value) -> Result<Value, StoreError> {
    SchemaResolver::new(provider).try_resolve(&schema)
}

#[test]
fn test_resolve_passthrough_values_without_refs() {
    let p = MapProvider::new();
    for v in [
        json!({}),
        Value::Null,
        json!([1, 2, 3]),
        json!("test"),
        json!({"outer": {"inner": {"deep": "value"}}}),
    ] {
        assert_eq!(resolve(&p, v.clone()), v);
    }
}

#[test]
fn test_resolve_inlines_exact_ref() {
    let p = MapProvider::new().with(
        "gts.x.core.events.type.v1~",
        json!({
            "$id": "gts://gts.x.core.events.type.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"id": {"type": "string"}, "event": {"type": "string"}},
            "required": ["id"]
        }),
    );

    let resolved = resolve(&p, json!({"$ref": "gts://gts.x.core.events.type.v1~"}));

    assert_eq!(
        resolved,
        json!({
            "type": "object",
            "properties": {"id": {"type": "string"}, "event": {"type": "string"}},
            "required": ["id"]
        })
    );
}

#[test]
fn test_resolve_inlines_nested_ref() {
    let p = MapProvider::new().with(
        "gts.x.core.events.detail.v1~",
        json!({
            "$id": "gts://gts.x.core.events.detail.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"code": {"type": "string"}}
        }),
    );

    let resolved = resolve(
        &p,
        json!({
            "type": "object",
            "properties": {"detail": {"$ref": "gts://gts.x.core.events.detail.v1~"}}
        }),
    );

    assert_eq!(
        resolved,
        json!({
            "type": "object",
            "properties": {
                "detail": {
                    "type": "object",
                    "properties": {"code": {"type": "string"}}
                }
            }
        })
    );
}

#[test]
fn test_resolve_allof_inlines_refs_and_preserves_structure() {
    // `allOf` is preserved verbatim: every branch is resolved in place ($ref
    // inlined, $id/$schema stripped), branches are NOT flattened/merged into a
    // single object. Regression guard for faithful inlining.
    let p = MapProvider::new().with(
        "gts.x.core.events.base.v1~",
        json!({
            "$id": "gts://gts.x.core.events.base.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"id": {"type": "string"}},
            "required": ["id"],
            "additionalProperties": false
        }),
    );

    let resolved = resolve(
        &p,
        json!({
            "type": "object",
            "allOf": [
                {"$ref": "gts://gts.x.core.events.base.v1~"},
                {"type": "object", "properties": {"extra": {"type": "string"}}, "minProperties": 1}
            ]
        }),
    );

    assert_eq!(
        resolved,
        json!({
            "type": "object",
            "allOf": [
                {
                    "type": "object",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"],
                    "additionalProperties": false
                },
                {"type": "object", "properties": {"extra": {"type": "string"}}, "minProperties": 1}
            ]
        })
    );
}

#[test]
fn test_resolve_anyof_inlines_refs() {
    let p = MapProvider::new().with(
        "gts.x.core.events.a.v1~",
        json!({
            "$id": "gts://gts.x.core.events.a.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"a": {"type": "string"}}
        }),
    );

    let resolved = resolve(
        &p,
        json!({"anyOf": [{"$ref": "gts://gts.x.core.events.a.v1~"}, {"type": "null"}]}),
    );

    assert_eq!(
        resolved,
        json!({
            "anyOf": [
                {"type": "object", "properties": {"a": {"type": "string"}}},
                {"type": "null"}
            ]
        })
    );
}

#[test]
fn test_resolve_oneof_inlines_refs() {
    let p = MapProvider::new()
        .with(
            "gts.x.core.events.x.v1~",
            json!({
                "$id": "gts://gts.x.core.events.x.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {"x": {"type": "integer"}}
            }),
        )
        .with(
            "gts.x.core.events.y.v1~",
            json!({
                "$id": "gts://gts.x.core.events.y.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {"y": {"type": "boolean"}}
            }),
        );

    let resolved = resolve(
        &p,
        json!({
            "oneOf": [
                {"$ref": "gts://gts.x.core.events.x.v1~"},
                {"$ref": "gts://gts.x.core.events.y.v1~"}
            ]
        }),
    );

    assert_eq!(
        resolved,
        json!({
            "oneOf": [
                {"type": "object", "properties": {"x": {"type": "integer"}}},
                {"type": "object", "properties": {"y": {"type": "boolean"}}}
            ]
        })
    );
}

#[test]
fn test_resolve_strips_type_level_modifiers_from_inlined_ref() {
    // When a base is inlined into a non-root position (here an `allOf` branch),
    // the root-only keys are dropped: `$id`, `$schema`, and the type-level
    // modifiers `x-gts-final`/`x-gts-abstract`/`x-gts-traits`/
    // `x-gts-traits-schema`. Everything else (incl. `title`) is preserved.
    let p = MapProvider::new().with(
        "gts.x.core.events.modbase.v1~",
        json!({
            "$id": "gts://gts.x.core.events.modbase.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "title": "Mod Base",
            "x-gts-final": true,
            "x-gts-abstract": true,
            "x-gts-traits": {"tier": "gold"},
            "x-gts-traits-schema": {"type": "object", "properties": {"tier": {"type": "string"}}},
            "properties": {"id": {"type": "string"}},
            "required": ["id"]
        }),
    );

    let resolved = resolve(
        &p,
        json!({"allOf": [{"$ref": "gts://gts.x.core.events.modbase.v1~"}]}),
    );

    assert_eq!(
        resolved,
        json!({
            "allOf": [
                {
                    "type": "object",
                    "title": "Mod Base",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"]
                }
            ]
        })
    );
}

#[test]
fn test_resolve_traits_schema_allof_inlines_refs() {
    let p = MapProvider::new()
        .with(
            "gts.x.core.traits.retention.v1~",
            json!({
                "$id": "gts://gts.x.core.traits.retention.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {"retention": {"type": "string"}},
                "required": ["retention"],
                "additionalProperties": false
            }),
        )
        .with(
            "gts.x.core.traits.region.v1~",
            json!({
                "$id": "gts://gts.x.core.traits.region.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {"region": {"enum": ["eu", "us"]}},
                "required": ["region"]
            }),
        );

    let resolved = resolve(
        &p,
        json!({
            "$id": "gts://gts.x.core.events.event.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "x-gts-traits-schema": {
                "allOf": [
                    {"$ref": "gts://gts.x.core.traits.retention.v1~"},
                    {"$ref": "gts://gts.x.core.traits.region.v1~"}
                ]
            },
            "properties": {"id": {"type": "string"}}
        }),
    );

    assert_eq!(
        resolved,
        json!({
            "$id": "gts://gts.x.core.events.event.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "x-gts-traits-schema": {
                "allOf": [
                    {
                        "type": "object",
                        "properties": {"retention": {"type": "string"}},
                        "required": ["retention"],
                        "additionalProperties": false
                    },
                    {
                        "type": "object",
                        "properties": {"region": {"enum": ["eu", "us"]}},
                        "required": ["region"]
                    }
                ]
            },
            "properties": {"id": {"type": "string"}}
        })
    );
}

#[test]
fn test_resolve_three_type_derivation_chain_inlines_nested_allof_refs() {
    let p = MapProvider::new()
        .with(
            "gts.x.core.events.base.v1~",
            json!({
                "$id": "gts://gts.x.core.events.base.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"],
                "additionalProperties": false
            }),
        )
        .with(
            "gts.x.core.events.base.v1~x.core.events.enriched.v1~",
            json!({
                "$id": "gts://gts.x.core.events.base.v1~x.core.events.enriched.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {"$ref": "gts://gts.x.core.events.base.v1~"},
                    {
                        "type": "object",
                        "properties": {"tenant": {"type": "string"}},
                        "required": ["tenant"]
                    }
                ]
            }),
        );

    let resolved = resolve(
        &p,
        json!({
            "$id": "gts://gts.x.core.events.base.v1~x.core.events.enriched.v1~x.core.events.audit.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "allOf": [
                {"$ref": "gts://gts.x.core.events.base.v1~x.core.events.enriched.v1~"},
                {
                    "type": "object",
                    "properties": {"audit_id": {"type": "string"}},
                    "required": ["audit_id"]
                }
            ]
        }),
    );

    assert_eq!(
        resolved,
        json!({
            "$id": "gts://gts.x.core.events.base.v1~x.core.events.enriched.v1~x.core.events.audit.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "allOf": [
                {
                    "type": "object",
                    "allOf": [
                        {
                            "type": "object",
                            "properties": {"id": {"type": "string"}},
                            "required": ["id"],
                            "additionalProperties": false
                        },
                        {
                            "type": "object",
                            "properties": {"tenant": {"type": "string"}},
                            "required": ["tenant"]
                        }
                    ]
                },
                {
                    "type": "object",
                    "properties": {"audit_id": {"type": "string"}},
                    "required": ["audit_id"]
                }
            ]
        })
    );
}

#[test]
fn test_resolve_ref_with_object_siblings_composes_into_allof() {
    // A `$ref` carrying object-shaped sibling keywords composes the resolved
    // target with the siblings via `allOf` (rather than a lossy last-wins merge).
    let p = MapProvider::new().with(
        "gts.x.core.events.base.v1~",
        json!({
            "$id": "gts://gts.x.core.events.base.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"id": {"type": "string"}}
        }),
    );

    let resolved = resolve(
        &p,
        json!({
            "$ref": "gts://gts.x.core.events.base.v1~",
            "properties": {"name": {"type": "string"}}
        }),
    );

    assert_eq!(
        resolved,
        json!({
            "allOf": [
                {"type": "object", "properties": {"id": {"type": "string"}}},
                {"properties": {"name": {"type": "string"}}}
            ]
        })
    );
}

#[test]
fn test_resolve_keeps_unresolved_bare_ref() {
    let p = MapProvider::new();
    let schema = json!({"$ref": "gts://gts.x.core.events.missing.v1~"});
    assert_eq!(resolve(&p, schema.clone()), schema);
}

#[test]
fn test_resolve_keeps_unresolved_ref_with_siblings() {
    let p = MapProvider::new();
    let schema = json!({
        "type": "object",
        "properties": {
            "event": {
                "$ref": "gts://gts.x.core.events.missing.v1~",
                "description": "missing dependency must not be dropped"
            }
        }
    });
    let resolved = resolve(&p, schema.clone());
    assert_eq!(resolved, schema);
}

#[test]
fn test_try_resolve_errors_on_unresolved_ref() {
    let p = MapProvider::new();
    let err = try_resolve(
        &p,
        json!({
            "type": "object",
            "properties": {
                "event": {
                    "$ref": "gts://gts.x.core.events.missing.v1~",
                    "description": "strict mode should reject this"
                }
            }
        }),
    )
    .expect_err("missing external ref should fail checked resolution");

    assert!(matches!(
        &err,
        StoreError::UnresolvedRefs(refs)
            if refs == &["gts://gts.x.core.events.missing.v1~".to_owned()]
    ));
}

#[test]
fn test_try_resolve_allows_duplicate_ref_in_allof() {
    // Redundant manual aggregation (the same $ref appearing more than once in an
    // allOf composition) uses DFS-path cycle detection, so independent duplicate
    // $refs are not flagged as cycles.
    let p = MapProvider::new().with(
        "gts.x.test.dup.trait.v1~",
        json!({
            "$id": "gts://gts.x.test.dup.trait.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"retention": {"type": "string"}}
        }),
    );

    let schema = json!({
        "type": "object",
        "allOf": [
            {"$ref": "gts://gts.x.test.dup.trait.v1~"},
            {"$ref": "gts://gts.x.test.dup.trait.v1~"}
        ]
    });

    assert!(try_resolve(&p, schema.clone()).is_ok());
    assert!(resolve(&p, schema).is_object());
}

#[test]
fn test_resolve_pointer_to_boolean_with_siblings() {
    // A gts:// $ref with a pointer fragment that resolves to a non-object
    // (boolean) subschema, plus a sibling keyword. The ref resolves, so it must
    // NOT be reported unresolved, and the resolved boolean wins per $ref precedence.
    let p = MapProvider::new().with(
        "gts.x.core.events.flag.v1~",
        json!({
            "$id": "gts://gts.x.core.events.flag.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "$defs": {"closed": false}
        }),
    );

    let resolved = try_resolve(
        &p,
        json!({
            "$ref": "gts://gts.x.core.events.flag.v1~#/$defs/closed",
            "description": "extra"
        }),
    )
    .expect("resolved non-object ref with siblings must not be reported unresolved");

    assert_eq!(resolved, json!(false));
}

#[test]
fn test_resolve_circular_ref_does_not_hang() {
    // A refs B, B refs A. Lenient resolve must terminate; checked resolution
    // reports the cycle.
    let p = MapProvider::new()
        .with(
            "gts.x.test.circ.a.v1~",
            json!({
                "$id": "gts://gts.x.test.circ.a.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [{"$ref": "gts://gts.x.test.circ.b.v1~"}],
                "properties": {"id": {"type": "string"}}
            }),
        )
        .with(
            "gts.x.test.circ.b.v1~",
            json!({
                "$id": "gts://gts.x.test.circ.b.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [{"$ref": "gts://gts.x.test.circ.a.v1~"}],
                "properties": {"name": {"type": "string"}}
            }),
        );

    let schema = json!({
        "$id": "gts://gts.x.test.circ.a.v1~",
        "type": "object",
        "allOf": [{"$ref": "gts://gts.x.test.circ.b.v1~"}],
        "properties": {"id": {"type": "string"}}
    });

    assert!(resolve(&p, schema.clone()).is_object());
    assert!(matches!(
        try_resolve(&p, schema).expect_err("circular ref must fail checked resolution"),
        StoreError::CircularRef
    ));
}
