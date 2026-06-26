//! Unit tests for [`SchemaResolver`]. They drive the resolver directly against
//! a tiny in-memory [`SchemaProvider`] mock (`MapProvider`) — no `GtsStore`
//! involved — so they exercise `SchemaResolver::resolve` in isolation.
//! End-to-end coverage of the `GtsStore` wrapper and provider lookup semantics
//! lives in `store_test.rs`.

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
    SchemaResolver::new(provider)
        .resolve(&schema)
        .expect("schema should resolve")
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
fn test_resolve_errors_on_unresolved_ref() {
    let p = MapProvider::new();
    let err = SchemaResolver::new(&p)
        .resolve(&json!({
            "type": "object",
            "properties": {
                "event": {
                    "$ref": "gts://gts.x.core.events.missing.v1~",
                    "description": "strict mode should reject this"
                }
            }
        }))
        .expect_err("missing external ref should fail checked resolution");

    assert!(matches!(
        &err,
        StoreError::UnresolvedRefs(refs)
            if refs == &["gts://gts.x.core.events.missing.v1~".to_owned()]
    ));
}

#[test]
fn test_resolve_allows_duplicate_ref_in_allof() {
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

    assert!(SchemaResolver::new(&p).resolve(&schema).is_ok());
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

    let resolved = SchemaResolver::new(&p)
        .resolve(&json!({
            "$ref": "gts://gts.x.core.events.flag.v1~#/$defs/closed",
            "description": "extra"
        }))
        .expect("resolved non-object ref with siblings must not be reported unresolved");

    assert_eq!(resolved, json!(false));
}

#[test]
fn test_resolve_remote_pointer_fragment_resolves_local_refs_against_remote_root() {
    // A fragment selected from a remote GTS document can contain local refs that
    // are scoped to that remote document root. The inlined fragment must not
    // keep a dangling `#/$defs/...` pointer after the root `$defs` is dropped.
    let p = MapProvider::new().with(
        "gts.x.core.events.named.v1~",
        json!({
            "$id": "gts://gts.x.core.events.named.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": {"Name": {"type": "string"}},
            "properties": {
                "name": {"$ref": "#/$defs/Name"}
            }
        }),
    );

    let resolved = SchemaResolver::new(&p)
        .resolve(&json!({"$ref": "gts://gts.x.core.events.named.v1~#/properties/name"}))
        .expect("remote pointer fragment should resolve");

    assert_eq!(resolved, json!({"type": "string"}));
}

#[test]
fn test_resolve_remote_fragment_uses_remote_root_not_caller_root() {
    // Regression guard: the same `#/$defs/Name` path exists in both the caller
    // schema and the remote document, but with different types. The fragment's
    // local ref must resolve against the *remote* root, not the caller's root.
    let p = MapProvider::new().with(
        "gts.x.core.events.named.v1~",
        json!({
            "$id": "gts://gts.x.core.events.named.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": {"Name": {"type": "string"}},
            "properties": {
                "name": {"$ref": "#/$defs/Name"}
            }
        }),
    );

    let schema = json!({
        "$defs": {"Name": {"type": "integer"}},
        "properties": {
            "test": {"$ref": "gts://gts.x.core.events.named.v1~#/properties/name"}
        }
    });

    let resolved = SchemaResolver::new(&p)
        .resolve(&schema)
        .expect("remote pointer fragment should resolve");

    assert_eq!(
        resolved,
        json!({
            "$defs": {"Name": {"type": "integer"}},
            "properties": {
                "test": {"type": "string"}
            }
        })
    );
}

#[test]
fn test_resolve_remote_fragment_missing_local_ref_errors() {
    // A remote fragment contains a local ref whose target does not exist in the
    // remote document root. Strict resolution must report it as unresolved.
    let p = MapProvider::new().with(
        "gts.x.core.events.broken.v1~",
        json!({
            "$id": "gts://gts.x.core.events.broken.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": {"Other": {"type": "string"}},
            "properties": {
                "name": {"$ref": "#/$defs/Missing"}
            }
        }),
    );

    let err = SchemaResolver::new(&p)
        .resolve(&json!({"$ref": "gts://gts.x.core.events.broken.v1~#/properties/name"}))
        .expect_err("missing local ref in remote fragment should fail");

    assert!(matches!(
        &err,
        StoreError::UnresolvedRefs(refs) if refs == &["#/$defs/Missing".to_owned()]
    ));
}

#[test]
fn test_resolve_nested_remote_fragments_resolve_local_refs_against_correct_root() {
    // Remote doc A's fragment contains a gts:// ref to remote doc B, and B's
    // fragment contains a local #/$defs/... ref. The local ref must resolve
    // against B's root, not A's or the caller's.
    let p = MapProvider::new()
        .with(
            "gts.x.core.events.outer.v1~",
            json!({
                "$id": "gts://gts.x.core.events.outer.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {
                    "inner": {"$ref": "gts://gts.x.core.events.inner.v1~#/properties/value"}
                }
            }),
        )
        .with(
            "gts.x.core.events.inner.v1~",
            json!({
                "$id": "gts://gts.x.core.events.inner.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "$defs": {"Value": {"type": "boolean"}},
                "properties": {
                    "value": {"$ref": "#/$defs/Value"}
                }
            }),
        );

    let resolved = SchemaResolver::new(&p)
        .resolve(&json!({"$ref": "gts://gts.x.core.events.outer.v1~#/properties/inner"}))
        .expect("nested remote fragments should resolve");

    assert_eq!(resolved, json!({"type": "boolean"}));
}

#[test]
fn test_resolve_remote_fragment_siblings_resolve_against_caller_root() {
    // A $ref to a remote fragment has siblings that contain local refs. The
    // fragment's local refs resolve against the remote root, but the siblings'
    // local refs must resolve against the caller's root.
    let p = MapProvider::new().with(
        "gts.x.core.events.remote.v1~",
        json!({
            "$id": "gts://gts.x.core.events.remote.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": {"Shared": {"type": "string"}},
            "properties": {
                "name": {"$ref": "#/$defs/Shared"}
            }
        }),
    );

    let schema = json!({
        "$defs": {"Shared": {"type": "integer"}},
        "properties": {
            "combined": {
                "$ref": "gts://gts.x.core.events.remote.v1~#/properties/name",
                "properties": {
                    "local": {"$ref": "#/$defs/Shared"}
                }
            }
        }
    });

    let resolved = SchemaResolver::new(&p)
        .resolve(&schema)
        .expect("remote fragment with siblings should resolve");

    assert_eq!(
        resolved,
        json!({
            "$defs": {"Shared": {"type": "integer"}},
            "properties": {
                "combined": {
                    "allOf": [
                        {"type": "string"},
                        {"properties": {"local": {"type": "integer"}}}
                    ]
                }
            }
        })
    );
}

#[test]
fn test_resolve_root_self_ref_is_circular() {
    // `$ref: "#"` is a JSON Pointer to the document root. Since the root
    // contains the ref itself, inlining is inherently recursive and must be
    // detected as a cycle — NOT reported as an unresolved external ref (which
    // was the bug before `#` was handled as a local pointer).
    let p = MapProvider::new();

    let schema = json!({
        "$id": "gts://gts.x.test.self.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "self_ref": {"$ref": "#"}
        }
    });

    assert!(matches!(
        SchemaResolver::new(&p)
            .resolve(&schema)
            .expect_err("root self-ref must be detected as circular"),
        StoreError::CircularRef
    ));
}

#[test]
fn test_resolve_root_self_ref_with_siblings_is_circular() {
    // `$ref: "#"` with siblings is still circular — the root contains the
    // ref, so inlining recurses. Cycle detection must fire, not UnresolvedRefs.
    let p = MapProvider::new();

    let schema = json!({
        "$id": "gts://gts.x.test.self2.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "wrapped": {
                "$ref": "#",
                "properties": {"extra": {"type": "boolean"}}
            }
        }
    });

    assert!(matches!(
        SchemaResolver::new(&p)
            .resolve(&schema)
            .expect_err("root self-ref with siblings must be circular"),
        StoreError::CircularRef
    ));
}

#[test]
fn test_resolve_circular_ref_does_not_hang() {
    // A refs B, B refs A. Strict resolution must terminate and report the cycle.
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

    assert!(matches!(
        SchemaResolver::new(&p)
            .resolve(&schema)
            .expect_err("circular ref must fail checked resolution"),
        StoreError::CircularRef
    ));
}
