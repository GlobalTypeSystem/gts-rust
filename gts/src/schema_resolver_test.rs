//! Unit tests for [`SchemaResolver`]. They drive the resolver directly against
//! a tiny in-memory [`SchemaProvider`] mock (`MapProvider`) — no `GtsStore`
//! involved — so they exercise `SchemaResolver::resolve` in isolation.
//! End-to-end coverage of the `GtsStore` wrapper and provider lookup semantics
//! lives in `store_test.rs`.
//! Partial resolution is the default here; `resolve_fully` exercises full mode.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{ResolutionMode, SchemaProvider, SchemaResolver};
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
fn resolve_partial(provider: &MapProvider, schema: Value) -> Value {
    SchemaResolver::new(provider)
        .resolve(&schema)
        .expect("schema should resolve in partial mode")
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
        assert_eq!(resolve_partial(&p, v.clone()), v);
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

    let resolved = resolve_partial(&p, json!({"$ref": "gts://gts.x.core.events.type.v1~"}));

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

    let resolved = resolve_partial(
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

    let resolved = resolve_partial(
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

    let resolved = resolve_partial(
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

    let resolved = resolve_partial(
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

    let resolved = resolve_partial(
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

    let resolved = resolve_partial(
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

    let resolved = resolve_partial(
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
    // A `$ref` carrying object-shaped sibling keywords keeps the siblings at
    // the reference site and adds the resolved target to its `allOf` (rather
    // than a lossy last-wins merge).
    let p = MapProvider::new().with(
        "gts.x.core.events.base.v1~",
        json!({
            "$id": "gts://gts.x.core.events.base.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"id": {"type": "string"}}
        }),
    );

    let resolved = resolve_partial(
        &p,
        json!({
            "$ref": "gts://gts.x.core.events.base.v1~",
            "properties": {"name": {"type": "string"}}
        }),
    );

    assert_eq!(
        resolved,
        json!({
            "properties": {"name": {"type": "string"}},
            "allOf": [{"type": "object", "properties": {"id": {"type": "string"}}}]
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
    assert!(resolve_partial(&p, schema).is_object());
}

#[test]
fn test_resolve_pointer_to_boolean_with_siblings() {
    // A gts:// $ref with a pointer fragment that resolves to a non-object
    // (boolean) subschema, plus a sibling keyword. The ref resolves, so it must
    // NOT be reported unresolved, and the sibling still applies beside it.
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

    assert_eq!(resolved, json!({"description": "extra", "allOf": [false]}));
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
                    "properties": {"local": {"type": "integer"}},
                    "allOf": [{"type": "string"}]
                }
            }
        })
    );
}

#[test]
fn test_resolve_root_self_ref_is_recursive_not_circular() {
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

    let resolved = SchemaResolver::new(&p)
        .resolve(&schema)
        .expect("a recursive local $ref is legal and must resolve");

    assert_eq!(
        resolved.pointer("/properties/self_ref/properties/self_ref"),
        Some(&json!({"$ref": "#"})),
        "the recursive reference must survive rather than expand forever"
    );
}

#[test]
fn test_resolve_root_self_ref_with_siblings_is_recursive_not_circular() {
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

    let resolved = SchemaResolver::new(&p)
        .resolve(&schema)
        .expect("a recursive local $ref with siblings is legal and must resolve");

    assert_eq!(
        resolved.pointer("/properties/wrapped"),
        schema.pointer("/properties/wrapped"),
        "a recursive node must be left as authored: {resolved}"
    );
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

/// Builds `links` registered documents, each `$ref`ing the next, and resolves a
/// root pointing at the first.
///
/// Each link wraps the next one, so the resolved root nests `links` levels deep
/// even though no single document nests at all - which is the shape
/// [`super::MAX_REF_CHAIN_DEPTH`] exists to bound.
fn resolve_ref_chain(links: usize) -> Result<Value, StoreError> {
    let id = |index: usize| format!("gts.x.test.chain.n{index}.v1~");
    let mut provider = MapProvider::new();
    for index in 0..links {
        let content = if index + 1 == links {
            json!({"type": "object"})
        } else {
            json!({
                "type": "object",
                "properties": {"next": {"$ref": format!("gts://{}", id(index + 1))}}
            })
        };
        provider = provider.with(&id(index), content);
    }
    SchemaResolver::new(&provider).resolve(&json!({"$ref": format!("gts://{}", id(0))}))
}

/// A long `$ref` chain multiplies each link's own nesting into the resolved
/// body, so it is refused rather than inlined - every later walk descends the
/// tree this produces.
///
/// Both sides of the budget are asserted, so it can be neither silently
/// tightened (which would refuse documents that must resolve) nor loosened.
#[test]
fn test_resolve_refuses_a_ref_chain_past_the_budget() {
    let resolved = resolve_ref_chain(super::MAX_REF_CHAIN_DEPTH)
        .expect("a chain exactly at the budget must still resolve");
    // Fully inlined: nothing is left pointing outwards.
    assert!(
        !resolved.to_string().contains("$ref"),
        "chain at the budget must be inlined: {resolved}"
    );

    let error = resolve_ref_chain(super::MAX_REF_CHAIN_DEPTH + 1)
        .expect_err("one link past the budget must not resolve");
    let StoreError::UnresolvedRefs(refs) = error else {
        panic!("expected the over-long chain to be reported as unresolved: {error:?}");
    };
    assert!(
        refs.iter().any(|entry| entry.contains("chained $refs")),
        "{refs:?}"
    );
}

#[test]
fn test_recursive_pointer_target_stays_addressable_from_the_output_root() {
    let p = MapProvider::new();

    let schema = json!({
        "$id": "gts://gts.x.test.defs.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$ref": "#/$defs/node",
        "$defs": {
            "node": {
                "type": "object",
                "properties": {"next": {"$ref": "#/$defs/node"}}
            }
        }
    });

    let resolved = SchemaResolver::new(&p)
        .resolve(&schema)
        .expect("a recursive pointer is legal and must resolve");

    assert!(
        resolved.pointer("/$defs/node").is_some(),
        "the pointer target must remain at the document root: {resolved}"
    );

    crate::json_schema::validator_for(&resolved).expect("the resolved document must compile");
}

#[test]
fn test_recursive_pointer_into_properties_stays_addressable() {
    let p = MapProvider::new();

    let schema = json!({
        "$id": "gts://gts.x.test.propptr.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$ref": "#/properties/node",
        "properties": {
            "node": {
                "type": "object",
                "properties": {"next": {"$ref": "#/properties/node"}}
            }
        }
    });

    let resolved = SchemaResolver::new(&p)
        .resolve(&schema)
        .expect("a recursive pointer is legal and must resolve");

    assert!(
        resolved.pointer("/properties/node").is_some(),
        "the pointer target must remain reachable: {resolved}"
    );
    crate::json_schema::validator_for(&resolved).expect("the result must compile");
}

#[test]
fn test_referring_definitions_do_not_replace_the_target_s_own() {
    let p = MapProvider::new().with(
        "gts.x.test.defsmerge.base.v1~",
        json!({
            "$id": "gts://gts.x.test.defsmerge.base.v1~",
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {"from_base": {"$ref": "#/$defs/shared"}},
            "$defs": {"shared": {"type": "integer"}}
        }),
    );

    let schema = json!({
        "$id": "gts://gts.x.test.defsmerge.leaf.v1~",
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": "gts://gts.x.test.defsmerge.base.v1~",
        "$defs": {"own": {"type": "string"}}
    });

    let resolved = SchemaResolver::new(&p).resolve(&schema).expect("resolves");

    crate::json_schema::validator_for(&resolved).expect("the resolved document must still compile");
}

#[test]
fn test_resolve_reads_a_pointer_inside_an_embedded_resource_from_that_resource() {
    // `#/definitions/n` inside `legacy` names legacy's own definition, whether
    // the document is resolved itself or inlined through a `gts://` reference.
    let lib = json!({
        "$id": "gts://gts.x.test.embedded.lib.v1~",
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "definitions": {"n": {"type": "string"}},
        "properties": {"legacy": {
            "$id": "legacy",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {"n": {"$ref": "#/definitions/n"}},
            "definitions": {"n": {"type": "integer"}}
        }}
    });
    let p = MapProvider::new().with("gts.x.test.embedded.lib.v1~", lib.clone());

    let on_its_own = resolve_partial(&p, lib);
    assert_eq!(
        on_its_own.pointer("/properties/legacy/properties/n"),
        Some(&json!({"type": "integer"}))
    );

    let inlined = resolve_partial(
        &p,
        json!({
            "$id": "gts://gts.x.test.embedded.holder.v1~",
            "properties": {"item": {"$ref": "gts://gts.x.test.embedded.lib.v1~#/properties/legacy"}}
        }),
    );
    assert_eq!(
        inlined.pointer("/properties/item/properties/n"),
        Some(&json!({"type": "integer"})),
        "{inlined}"
    );
}

const DRAFT_07: &str = "http://json-schema.org/draft-07/schema#";

#[test]
fn test_resolve_draft07_drops_ref_siblings() {
    // Draft-07 ignores `$ref` siblings, custom keywords included, so the
    // target replaces the reference site alone.
    let resolved = resolve_partial(
        &MapProvider::new(),
        json!({
            "$schema": DRAFT_07,
            "definitions": {"s": {"type": "string"}},
            "properties": {
                "n": {"$ref": "#/definitions/s", "minLength": 5, "x-gts-type-ref": "gts.x.a.b.c.v1~"}
            }
        }),
    );

    assert_eq!(
        resolved.pointer("/properties/n"),
        Some(&json!({"type": "string"}))
    );
}

#[test]
fn test_resolve_draft07_root_ref_keeps_identity_and_dialect() {
    let resolved = resolve_partial(
        &MapProvider::new(),
        json!({
            "$id": "gts://gts.x.test.root.v1~",
            "$schema": DRAFT_07,
            "definitions": {"t": {"type": "object"}},
            "$ref": "#/definitions/t",
            "required": ["v"]
        }),
    );

    assert_eq!(
        resolved,
        json!({
            "$id": "gts://gts.x.test.root.v1~",
            "$schema": DRAFT_07,
            "allOf": [{"type": "object"}]
        })
    );
}

#[test]
fn test_resolve_draft07_resolves_siblings_of_a_recursive_root_ref() {
    // Siblings of a kept recursive `$ref` are inert in Draft-07, but the kept
    // pointer reaches into one of them, so its references must be inlined.
    let p = MapProvider::new().with(
        "gts.x.test.other.v1~",
        json!({"$id": "gts://gts.x.test.other.v1~", "$schema": DRAFT_07, "type": "string"}),
    );
    let resolved = resolve_partial(
        &p,
        json!({
            "$id": "gts://gts.x.test.root.v1~",
            "$schema": DRAFT_07,
            "$ref": "#/definitions/node",
            "definitions": {"node": {"properties": {
                "child": {"$ref": "#/definitions/node"},
                "base": {"$ref": "gts://gts.x.test.other.v1~"}
            }}}
        }),
    );

    assert_eq!(resolved["$ref"], json!("#/definitions/node"));
    assert_eq!(
        resolved.pointer("/definitions/node/properties/base/type"),
        Some(&json!("string")),
        "{resolved}"
    );
}

#[test]
fn test_resolve_draft07_reports_a_missing_ref_beside_a_recursive_ref() {
    // A kept pointer may reach into a sibling, so a sibling's reference is
    // never presumed dead.
    let err = SchemaResolver::new(&MapProvider::new())
        .resolve(&json!({
            "$schema": DRAFT_07,
            "properties": {
                "self": {"$ref": "#", "items": {"$ref": "gts://gts.x.test.missing.v1~"}}
            }
        }))
        .expect_err("a missing sibling reference must be reported");

    assert!(
        matches!(&err, StoreError::UnresolvedRefs(refs) if refs == &["gts://gts.x.test.missing.v1~"]),
        "{err:?}"
    );
}

#[test]
fn test_resolve_ref_target_joins_an_authored_allof() {
    let resolved = resolve_partial(
        &MapProvider::new(),
        json!({
            "$defs": {"t": {"properties": {"a": true}}},
            "properties": {
                "n": {
                    "$ref": "#/$defs/t",
                    "allOf": [{"required": ["a"]}],
                    "unevaluatedProperties": false
                }
            }
        }),
    );

    assert_eq!(
        resolved.pointer("/properties/n"),
        Some(&json!({
            "unevaluatedProperties": false,
            "allOf": [{"properties": {"a": true}}, {"required": ["a"]}]
        }))
    );
}

const DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";

fn resolve_fully(provider: &MapProvider, schema: &Value) -> Result<Value, StoreError> {
    SchemaResolver::new(provider)
        .with_resolution_mode(ResolutionMode::Full)
        .resolve(schema)
}

fn resolve_in_both_modes(provider: &MapProvider, schema: &Value) -> [Result<Value, StoreError>; 2] {
    [
        SchemaResolver::new(provider).resolve(schema),
        resolve_fully(provider, schema),
    ]
}

#[test]
fn test_ref_shaped_literals_are_data() {
    let p = MapProvider::new();
    for literal in [json!({"$ref": "#"}), json!({"$ref": "#/nope"})] {
        for dialect in [
            DRAFT_07,
            "https://json-schema.org/draft/2019-09/schema",
            DRAFT_2020_12,
        ] {
            let schema = json!({
                "$schema": dialect,
                "type": "object",
                "default": literal,
                "examples": [literal],
                "const": literal,
                "enum": [literal],
                "x-annotation": {"nested": [literal]},
                "x-gts-traits": {"value": literal},
                "properties": {"a": {"default": literal, "const": literal}}
            });
            for result in resolve_in_both_modes(&p, &schema) {
                assert_eq!(result.expect("literals are not references"), schema);
            }
        }
    }
}

#[test]
fn test_resolution_is_idempotent() {
    let p = MapProvider::new().with(
        "gts.x.test.idem.target.v1~",
        json!({"type": "object", "default": {"$ref": "#"}}),
    );
    let schema = json!({
        "$schema": DRAFT_2020_12,
        "$defs": {"A": {"type": "string", "default": {"$ref": "#/$defs/A"}}},
        "default": {"$ref": "#"},
        "properties": {
            "a": {"$ref": "#/$defs/A"},
            "b": {"$ref": "gts://gts.x.test.idem.target.v1~"}
        }
    });
    let once = resolve_partial(&p, schema);
    assert_eq!(once["default"], json!({"$ref": "#"}));
    assert_eq!(resolve_partial(&p, once.clone()), once);
}

#[test]
fn test_pointer_into_literal_data_resolves_the_copy_as_a_schema() {
    let schema = json!({
        "$schema": DRAFT_2020_12,
        "$defs": {"T": {"type": "string"}},
        "default": {"properties": {"v": {"$ref": "#/$defs/T"}}},
        "properties": {"node": {"$ref": "#/default"}}
    });
    let resolved = resolve_partial(&MapProvider::new(), schema.clone());
    assert_eq!(resolved["default"], schema["default"]);
    assert_eq!(
        resolved.pointer("/properties/node/properties/v"),
        Some(&json!({"type": "string"}))
    );
}

#[test]
fn test_keyword_named_property_maps_are_containers() {
    let resolved = resolve_partial(
        &MapProvider::new(),
        json!({
            "$schema": DRAFT_07,
            "definitions": {"A": {"type": "string"}},
            "required": ["$ref"],
            "properties": {"$ref": {"$ref": "#/definitions/A"}},
            "patternProperties": {"$ref": {"$ref": "#/definitions/A"}},
            "dependencies": {"$ref": {"$ref": "#/definitions/A"}}
        }),
    );
    for keyword in ["properties", "patternProperties", "dependencies"] {
        assert_eq!(
            resolved[keyword]["$ref"],
            json!({"type": "string"}),
            "{keyword}"
        );
    }
}

#[test]
fn test_subschema_positions_follow_the_dialect() {
    let p = MapProvider::new();
    let draft7 = resolve_partial(
        &p,
        json!({
            "$schema": DRAFT_07,
            "definitions": {"A": {"type": "string"}},
            "items": [{"$ref": "#/definitions/A"}],
            "prefixItems": [{"$ref": "#/nope"}]
        }),
    );
    assert_eq!(draft7["items"], json!([{"type": "string"}]));
    assert_eq!(draft7["prefixItems"], json!([{"$ref": "#/nope"}]));

    let draft2020 = resolve_partial(
        &p,
        json!({
            "$schema": DRAFT_2020_12,
            "$defs": {"A": {"type": "string"}},
            "prefixItems": [{"$ref": "#/$defs/A"}],
            "dependencies": {"x": {"$ref": "#/nope"}}
        }),
    );
    assert_eq!(draft2020["prefixItems"], json!([{"type": "string"}]));
    assert_eq!(draft2020["dependencies"], json!({"x": {"$ref": "#/nope"}}));
}

#[test]
fn test_resolve_fully_rejects_local_recursion() {
    let p = MapProvider::new();
    for schema in [
        json!({"$schema": DRAFT_07, "properties": {"self": {"$ref": "#"}}}),
        json!({
            "$schema": DRAFT_07,
            "$ref": "#/definitions/node",
            "definitions": {"node": {"properties": {"next": {"$ref": "#/definitions/node"}}}}
        }),
        json!({
            "$schema": DRAFT_2020_12,
            "$defs": {
                "a": {"properties": {"b": {"$ref": "#/$defs/b"}}},
                "b": {"properties": {"a": {"$ref": "#/$defs/a"}}}
            },
            "properties": {"root": {"$ref": "#/$defs/a"}}
        }),
        json!({
            "$schema": DRAFT_2020_12,
            "x-gts-traits-schema": {"properties": {"again": {"$ref": "#/x-gts-traits-schema"}}}
        }),
    ] {
        SchemaResolver::new(&p)
            .resolve(&schema)
            .expect("recursion is legal");
        assert!(
            matches!(resolve_fully(&p, &schema), Err(StoreError::CircularRef)),
            "{schema}"
        );
    }
}

#[test]
fn test_resolve_fully_rejects_recursion_through_a_gts_reference() {
    let id = "gts.x.test.mat.tree.v1~";
    let p = MapProvider::new().with(
        id,
        json!({
            "$id": format!("gts://{id}"),
            "$schema": DRAFT_07,
            "properties": {"child": {"$ref": format!("gts://{id}")}}
        }),
    );
    let holder =
        json!({"$schema": DRAFT_07, "properties": {"tree": {"$ref": format!("gts://{id}")}}});
    assert!(matches!(
        resolve_fully(&p, &holder),
        Err(StoreError::CircularRef)
    ));
    // A local recursion inside another type is no less recursive.
    let local = MapProvider::new().with(
        id,
        json!({"$schema": DRAFT_07, "properties": {"child": {"$ref": "#"}}}),
    );
    assert!(matches!(
        resolve_fully(&local, &holder),
        Err(StoreError::CircularRef)
    ));
}

#[test]
fn test_resolve_fully_inlines_finite_and_repeated_references() {
    let p = MapProvider::new().with(
        "gts.x.test.mat.leaf.v1~",
        json!({
            "$schema": DRAFT_2020_12,
            "$defs": {"S": {"type": "string"}},
            "properties": {"s": {"$ref": "#/$defs/S"}}
        }),
    );
    let schema = json!({
        "$schema": DRAFT_2020_12,
        "$defs": {"A": {"$ref": "gts://gts.x.test.mat.leaf.v1~"}},
        "default": {"$ref": "#"},
        "properties": {
            "a": {"$ref": "#/$defs/A"},
            "b": {"$ref": "#/$defs/A"},
            "c": {"allOf": [{"$ref": "gts://gts.x.test.mat.leaf.v1~"}, {"$ref": "#/$defs/A"}]}
        }
    });
    let fully_resolved = resolve_fully(&p, &schema).expect("finite references resolve fully");
    assert_eq!(fully_resolved, resolve_partial(&p, schema));
    assert_eq!(fully_resolved["default"], json!({"$ref": "#"}));
    assert_eq!(
        fully_resolved.pointer("/properties/b/properties/s"),
        Some(&json!({"type": "string"}))
    );
}

#[test]
fn test_resolve_fully_reports_references_it_cannot_inline() {
    let p = MapProvider::new();
    let anchor = json!({
        "$schema": DRAFT_2020_12,
        "$defs": {"n": {"$anchor": "node", "type": "string"}},
        "properties": {"a": {"$ref": "#node"}}
    });
    SchemaResolver::new(&p)
        .resolve(&anchor)
        .expect("partial resolution keeps an anchor reference");
    assert!(matches!(
        resolve_fully(&p, &anchor),
        Err(StoreError::UnresolvedRefs(refs)) if refs == ["#node"]
    ));

    let dynamic = json!({"$schema": DRAFT_2020_12, "items": {"$dynamicRef": "#meta"}});
    assert!(matches!(
        resolve_fully(&p, &dynamic),
        Err(StoreError::UnresolvedRefs(refs)) if refs == ["#meta"]
    ));
    // Draft-07 has no `$dynamicRef`: there it is an unknown keyword.
    let draft7 = json!({"$schema": DRAFT_07, "items": {"$dynamicRef": "#meta"}});
    assert_eq!(resolve_fully(&p, &draft7).expect("not a reference"), draft7);

    assert!(matches!(
        resolve_fully(&p, &json!({"properties": {"a": {"$ref": "#/nope"}}})),
        Err(StoreError::UnresolvedRefs(refs)) if refs == ["#/nope"]
    ));

    let unknown = json!({
        "$schema": "https://example.test/custom-meta",
        "items": {"$dynamicRef": "#meta"},
        "default": {"$dynamicRef": "#literal"}
    });
    assert!(matches!(
        resolve_fully(&p, &unknown),
        Err(StoreError::UnresolvedRefs(refs)) if refs == ["#meta"]
    ));
}

#[test]
fn test_resolve_in_reads_pointers_and_dialect_from_the_host() {
    let p = MapProvider::new().with("gts.x.test.frag.v1~", json!({"type": "integer"}));
    let host = json!({
        "$schema": DRAFT_07,
        "definitions": {"R": {"type": "string", "enum": ["P30D"]}},
        "x-gts-traits-schema": {
            "properties": {
                "retention": {"$ref": "#/definitions/R"},
                "pair": {"items": [{"$ref": "#/definitions/R"}]},
                "count": {"$ref": "gts://gts.x.test.frag.v1~"},
                "sample": {"default": {"$ref": "#/nope"}}
            }
        }
    });
    let resolved = SchemaResolver::new(&p)
        .resolve_in(&host["x-gts-traits-schema"], &host)
        .expect("trait schema resolves in its host");
    assert_eq!(
        resolved["properties"],
        json!({
            "retention": {"type": "string", "enum": ["P30D"]},
            "pair": {"items": [{"type": "string", "enum": ["P30D"]}]},
            "count": {"type": "integer"},
            "sample": {"default": {"$ref": "#/nope"}}
        })
    );

    let missing = json!({"x-gts-traits-schema": {"$ref": "#/$defs/Missing"}});
    assert!(matches!(
        SchemaResolver::new(&p).resolve_in(&missing["x-gts-traits-schema"], &missing),
        Err(StoreError::UnresolvedRefs(refs)) if refs == ["#/$defs/Missing"]
    ));
}

#[test]
fn test_resolve_in_keeps_fragment_recursion_targeting_the_host() {
    let p = MapProvider::new();
    // A host pointer would dangle in the fragment: it is rebased onto the
    // host's `$id`, and the host bundled.
    let host = json!({
        "$id": "gts://gts.x.test.host.v1~",
        "$schema": DRAFT_2020_12,
        "$defs": {"node": {"properties": {"next": {"$ref": "#/$defs/node"}}}},
        "x-gts-traits-schema": {"$ref": "#/$defs/node"}
    });
    let fragment = &host["x-gts-traits-schema"];
    let resolved = SchemaResolver::new(&p)
        .resolve_in(fragment, &host)
        .expect("recursion into the host is legal");
    assert_eq!(
        resolved.pointer("/properties/next"),
        Some(&json!({"$ref": "gts://gts.x.test.host.v1~#/$defs/node"}))
    );
    assert_eq!(
        resolved["$defs"]["gts://gts.x.test.host.v1~"]["$id"],
        "gts://gts.x.test.host.v1~"
    );
    crate::json_schema::validator_for(&resolved).expect("the bundle compiles");
    assert!(matches!(
        SchemaResolver::new(&p)
            .with_resolution_mode(ResolutionMode::Full)
            .resolve_in(fragment, &host),
        Err(StoreError::CircularRef)
    ));
    // Without an `$id` there is nothing to rebase onto.
    let anonymous = json!({
        "$defs": {"node": {"properties": {"next": {"$ref": "#/$defs/node"}}}},
        "x-gts-traits-schema": {"$ref": "#/$defs/node"}
    });
    assert!(matches!(
        SchemaResolver::new(&p).resolve_in(&anonymous["x-gts-traits-schema"], &anonymous),
        Err(StoreError::CircularRef)
    ));

    // An embedded resource keeps its `$id`, so `#` still names it.
    let host = json!({
        "$schema": DRAFT_2020_12,
        "x-gts-traits-schema": {
            "$id": "https://example.test/trait",
            "properties": {"next": {"$ref": "#"}}
        }
    });
    let fragment = &host["x-gts-traits-schema"];
    let resolved = SchemaResolver::new(&p)
        .resolve_in(fragment, &host)
        .expect("recursion inside an embedded resource is kept");
    assert_eq!(
        resolved.pointer("/properties/next/properties/next"),
        Some(&json!({"$ref": "#"}))
    );
    assert!(matches!(
        SchemaResolver::new(&p)
            .with_resolution_mode(ResolutionMode::Full)
            .resolve_in(fragment, &host),
        Err(StoreError::CircularRef)
    ));
}
