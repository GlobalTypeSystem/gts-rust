use serde_json::Value;
use std::collections::BTreeSet;

use crate::gts::{GTS_URI_PREFIX, GtsTypeId};

/// Why a single `$ref` string is not a valid GTS reference.
///
/// Carried by [`ExtractRefsError::InvalidRef`] and rendered into the
/// path-tracked message the store surfaces as `StoreError::InvalidRef`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidRefReason {
    #[error("must be a local ref (starting with '#') or a GTS URI (starting with 'gts://')")]
    NotGtsUri,
    #[error("must reference a GTS type id (a valid identifier ending with '~'), got '{0}'")]
    InvalidTypeId(String),
    #[error(
        "has an unsupported fragment '#{0}'; only an empty fragment or a '/'-prefixed JSON \
         Pointer is allowed"
    )]
    UnsupportedFragment(String),
}

/// Failure modes of [`extract_gts_refs`]. Store-independent, like the module.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExtractRefsError {
    #[error("at '{path}': '{ref_uri}' {reason}")]
    InvalidRef {
        path: String,
        ref_uri: String,
        reason: InvalidRefReason,
    },
    #[error("schema nests deeper than the maximum scan depth of {0}")]
    TooDeep(usize),
}

/// How a single `$ref` value is interpreted under GTS rules.
enum RefKind<'a> {
    /// Internal JSON Pointer (`#`, `#/...`); not an external dependency.
    Local,
    /// External GTS type dependency: the canonical id (scheme + fragment
    /// stripped).
    External { id: &'a str },
}

/// The single, canonical interpretation of a `$ref` string. Both schema
/// validation ([`crate::store::GtsStore::validate_schema_refs`], via
/// [`extract_gts_refs`]) and dependency extraction share this definition so the
/// two cannot drift.
///
/// External references MUST use the `gts://` scheme; a bare id (no scheme) is
/// rejected, matching what the store is able to register and retrieve.
fn classify_ref(ref_uri: &str) -> Result<RefKind<'_>, InvalidRefReason> {
    // Local JSON Pointers (`#`, `#/...`) are always valid and not external edges.
    if ref_uri.starts_with('#') {
        return Ok(RefKind::Local);
    }

    // Everything else must be a `gts://` URI; a bare id or any other scheme is
    // not a ref the store can resolve.
    let Some(rest) = ref_uri.strip_prefix(GTS_URI_PREFIX) else {
        return Err(InvalidRefReason::NotGtsUri);
    };

    // A GTS `$ref` may carry a JSON Pointer fragment selecting a sub-schema of
    // the target document (e.g. `gts://...~#/x-gts-traits-schema`). Validate the
    // id portion as a type id and require any fragment to be empty or a
    // `/`-prefixed JSON Pointer - the exact shapes `SchemaResolver` is able to
    // dereference.
    let (id, fragment) = match rest.split_once('#') {
        Some((id, frag)) => (id, Some(frag)),
        None => (rest, None),
    };
    if GtsTypeId::try_new(id).is_err() {
        return Err(InvalidRefReason::InvalidTypeId(id.to_owned()));
    }
    if let Some(frag) = fragment
        && !frag.is_empty()
        && !frag.starts_with('/')
    {
        return Err(InvalidRefReason::UnsupportedFragment(frag.to_owned()));
    }
    Ok(RefKind::External { id })
}

/// Direct external type references of a **raw** schema - the `$ref`
/// dependency edges of this node, store-independent and pure.
///
/// Recurses the whole value, so the type body, `$defs`, combinators, and
/// `x-gts-traits-schema` are all covered (raw, before `allOf` flattening
/// drops `x-gts-*`). For every external `$ref` it returns the canonical GTS id:
/// the `gts://` scheme prefix and any `#...` pointer fragment are stripped.
///
/// Every `$ref` is validated against the single canonical definition
/// ([`classify_ref`]): external refs must use the `gts://` scheme and resolve to
/// a valid GTS **type** id (a valid identifier ending with `~`). A malformed or
/// bare-id ref is rejected up front rather than surfacing later as a failed
/// lookup, with a path-tracked error.
///
/// Excludes internal `#/...` references (e.g. `#/$defs/...`) and `x-gts-ref`
/// (a constraint on instance values, not a schema dependency to inline). Does
/// NOT include the `$id`-chain parent - that edge is derived structurally from
/// the id, not from content.
///
/// This is the **canonical, strict** ref definition for validation/resolution.
/// The lenient `GtsEntity` walkers (`extract_gts_ids_with_paths`,
/// `extract_ref_strings_with_paths`) feed the dependency graph / display
/// instead and intentionally diverge — they must not be conflated with this.
///
/// # Errors
/// [`ExtractRefsError::InvalidRef`] if a `$ref` is not a valid GTS reference
/// (see [`InvalidRefReason`]); [`ExtractRefsError::TooDeep`] if the schema nests past
/// the scan cap (a deeper ref could not be validated).
pub fn extract_gts_refs(schema: &Value) -> Result<BTreeSet<String>, ExtractRefsError> {
    let mut refs = BTreeSet::new();
    collect_gts_refs(schema, "", 0, &mut refs)?;
    Ok(refs)
}

fn collect_gts_refs(
    value: &Value,
    path: &str,
    depth: usize,
    out: &mut BTreeSet<String>,
) -> Result<(), ExtractRefsError> {
    const MAX_REF_SCAN_DEPTH: usize = 64;
    if depth > MAX_REF_SCAN_DEPTH {
        return Err(ExtractRefsError::TooDeep(MAX_REF_SCAN_DEPTH));
    }

    match value {
        Value::Object(map) => {
            if let Some(Value::String(ref_uri)) = map.get("$ref") {
                let ref_path = if path.is_empty() {
                    "$ref".to_owned()
                } else {
                    format!("{path}.$ref")
                };
                match classify_ref(ref_uri) {
                    Ok(RefKind::Local) => {}
                    Ok(RefKind::External { id }) => {
                        out.insert(id.to_owned());
                    }
                    Err(reason) => {
                        return Err(ExtractRefsError::InvalidRef {
                            path: ref_path,
                            ref_uri: ref_uri.clone(),
                            reason,
                        });
                    }
                }
            }
            for (key, v) in map {
                if key == "$ref" {
                    continue; // already classified above
                }
                // Data-valued keywords carry instance data, not subschemas, so a
                // `$ref` nested inside them is literal data, not a dependency edge.
                if matches!(key.as_str(), "const" | "default" | "examples" | "enum") {
                    continue;
                }
                let nested = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_gts_refs(v, &nested, depth + 1, out)?;
            }
        }
        Value::Array(items) => {
            for (idx, v) in items.iter().enumerate() {
                let nested = format!("{path}[{idx}]");
                collect_gts_refs(v, &nested, depth + 1, out)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_gts_refs_body_traits_and_normalization() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                // gts:// scheme ref in the body
                "a": {"$ref": "gts://gts.x.dep.ns.a.v1~"},
                // another gts:// scheme ref
                "b": {"$ref": "gts://gts.x.dep.ns.b.v1~"},
                // ref with a pointer fragment - fragment stripped to the id
                "c": {"$ref": "gts://gts.x.dep.ns.c.v1~#/properties/inner"},
                // internal JSON Pointer ref - excluded
                "d": {"$ref": "#/$defs/Local"},
                // x-gts-ref is an instance-value constraint, not a schema dep
                "e": {"type": "string", "x-gts-ref": "gts.x.notdep.ns.e.v1~"},
                // duplicate of `a` - must dedupe
                "f": {"$ref": "gts://gts.x.dep.ns.a.v1~"}
            },
            // refs nested in combinators must be found
            "allOf": [{"$ref": "gts://gts.x.dep.ns.allof.v1~"}],
            // refs inside x-gts-traits-schema must be found
            "x-gts-traits-schema": {
                "type": "object",
                "properties": {"t": {"$ref": "gts://gts.x.dep.ns.trait.v1~"}}
            }
        });

        let refs = extract_gts_refs(&schema).unwrap();
        let expected: BTreeSet<String> = [
            "gts.x.dep.ns.a.v1~",
            "gts.x.dep.ns.b.v1~",
            "gts.x.dep.ns.c.v1~",
            "gts.x.dep.ns.allof.v1~",
            "gts.x.dep.ns.trait.v1~",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();

        assert_eq!(refs, expected);
    }

    #[test]
    fn extract_gts_refs_none() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"id": {"type": "string"}},
            "x-gts-traits-schema": {"type": "object"}
        });
        assert!(extract_gts_refs(&schema).unwrap().is_empty());
    }

    #[test]
    fn extract_gts_refs_ignores_data_valued_keywords() {
        // A `$ref` inside `const`/`default`/`examples`/`enum` is data, not a
        // dependency edge, so it must not be classified even when malformed.
        let schema = json!({
            "type": "object",
            "properties": {
                "a": {"const": {"$ref": "not-a-schema-ref"}},
                "b": {"default": {"nested": {"$ref": "also-not-a-ref"}}},
                "c": {"enum": [{"$ref": "still-data"}]},
                "d": {"examples": [{"$ref": "example-data"}]},
                // A real schema ref alongside the data must still be found.
                "e": {"$ref": "gts://gts.x.dep.ns.real.v1~"}
            }
        });
        let refs = extract_gts_refs(&schema).unwrap();
        let expected: BTreeSet<String> = ["gts.x.dep.ns.real.v1~".to_owned()].into_iter().collect();
        assert_eq!(refs, expected);
    }

    #[test]
    fn extract_gts_refs_rejects_bare_id() {
        // A bare GTS id (no `gts://` scheme) is not a ref the store can resolve;
        // it must be rejected, matching `validate_ref_uris`.
        let bare_ref = json!({
            "type": "object",
            "properties": {"a": {"$ref": "gts.x.dep.ns.a.v1~"}}
        });
        assert!(matches!(
            extract_gts_refs(&bare_ref),
            Err(ExtractRefsError::InvalidRef {
                reason: InvalidRefReason::NotGtsUri,
                ..
            })
        ));
    }

    #[test]
    fn extract_gts_refs_rejects_invalid() {
        let instance_ref = json!({
            "type": "object",
            "properties": {"a": {"$ref": "gts://gts.x.dep.ns.a.v1"}}
        });
        assert!(matches!(
            extract_gts_refs(&instance_ref),
            Err(ExtractRefsError::InvalidRef {
                reason: InvalidRefReason::InvalidTypeId(_),
                ..
            })
        ));

        let garbage_ref = json!({
            "type": "object",
            "properties": {"a": {"$ref": "gts://not a gts id"}}
        });
        assert!(matches!(
            extract_gts_refs(&garbage_ref),
            Err(ExtractRefsError::InvalidRef {
                reason: InvalidRefReason::InvalidTypeId(_),
                ..
            })
        ));
    }

    #[test]
    fn extract_gts_refs_reports_path() {
        // An invalid ref must carry the JSON path to the offending `$ref`.
        let schema = json!({
            "properties": {"order": {"$ref": "invalid-ref"}}
        });
        let err = extract_gts_refs(&schema).unwrap_err();
        assert!(
            err.to_string().contains("properties.order.$ref"),
            "error should report the path, got: {err}"
        );
    }

    #[test]
    fn extract_gts_refs_rejects_unsupported_fragment() {
        let schema = json!({"$ref": "gts://gts.x.dep.ns.a.v1~#bad"});
        assert!(matches!(
            extract_gts_refs(&schema),
            Err(ExtractRefsError::InvalidRef {
                reason: InvalidRefReason::UnsupportedFragment(_),
                ..
            })
        ));
    }

    #[test]
    fn extract_gts_refs_rejects_too_deep() {
        // Nesting past the scan cap must error rather than silently pass.
        let mut schema = json!({"type": "object"});
        for _ in 0..70 {
            schema = json!({"properties": {"nested": schema}});
        }
        assert_eq!(
            extract_gts_refs(&schema),
            Err(ExtractRefsError::TooDeep(64))
        );
    }

    #[test]
    fn extract_gts_refs_found_in_arrays() {
        // Refs appearing directly as array elements must be collected.
        let schema = json!({
            "oneOf": [
                {"$ref": "gts://gts.x.dep.ns.one.v1~"},
                {"$ref": "gts://gts.x.dep.ns.two.v1~"}
            ]
        });
        let refs = extract_gts_refs(&schema).unwrap();
        let expected: BTreeSet<String> = ["gts.x.dep.ns.one.v1~", "gts.x.dep.ns.two.v1~"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(refs, expected);
    }

    #[test]
    fn extract_gts_refs_empty_fragment_normalizes_to_id() {
        // A trailing empty fragment (`#`) is stripped to the canonical id.
        let schema = json!({"$ref": "gts://gts.x.dep.ns.a.v1~#"});
        let refs = extract_gts_refs(&schema).unwrap();
        assert_eq!(refs, BTreeSet::from(["gts.x.dep.ns.a.v1~".to_owned()]));
    }

    #[test]
    fn extract_gts_refs_internal_pointer_only_excluded() {
        // A schema with only internal `#/...` pointers has no external edges.
        let schema = json!({
            "type": "object",
            "properties": {"a": {"$ref": "#/$defs/Local"}},
            "$defs": {"Local": {"type": "string"}}
        });
        assert!(extract_gts_refs(&schema).unwrap().is_empty());
    }
}
