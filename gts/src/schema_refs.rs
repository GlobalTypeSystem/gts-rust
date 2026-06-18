use serde_json::Value;
use std::collections::BTreeSet;

use crate::gts::{GTS_URI_PREFIX, GtsTypeId};
use crate::store::StoreError;

/// Direct external type references of a **raw** schema - the `$ref`
/// dependency edges of this node, store-independent and pure.
///
/// Recurses the whole value, so the type body, `$defs`, combinators, and
/// `x-gts-traits-schema` are all covered (raw, before `allOf` flattening
/// drops `x-gts-*`). For every external `$ref` it returns the canonical GTS id:
/// the `gts://` scheme prefix and any `#...` pointer fragment are stripped. A
/// bare id `$ref` (no `gts://`) is normalized the same way.
///
/// Each external `$ref` MUST resolve to a valid GTS **type** id (a valid
/// identifier ending with `~`); a malformed ref is rejected up front rather
/// than surfacing later as a failed lookup.
///
/// Excludes internal `#/...` references (e.g. `#/$defs/...`) and `x-gts-ref`
/// (a constraint on instance values, not a schema dependency to inline). Does
/// NOT include the `$id`-chain parent - that edge is derived structurally from
/// the id, not from content.
///
/// # Errors
/// `StoreError::InvalidRef` if an external `$ref` is not a valid GTS type id.
pub fn extract_gts_refs(schema: &Value) -> Result<BTreeSet<String>, StoreError> {
    let mut refs = BTreeSet::new();
    collect_gts_refs(schema, 0, &mut refs)?;
    Ok(refs)
}

fn collect_gts_refs(
    value: &Value,
    depth: usize,
    out: &mut BTreeSet<String>,
) -> Result<(), StoreError> {
    const MAX_REF_SCAN_DEPTH: usize = 64;
    if depth > MAX_REF_SCAN_DEPTH {
        return Ok(());
    }

    match value {
        Value::Object(map) => {
            if let Some(Value::String(ref_uri)) = map.get("$ref")
                && !ref_uri.starts_with('#')
            {
                let canonical = ref_uri.strip_prefix(GTS_URI_PREFIX).unwrap_or(ref_uri);
                let id = canonical.split_once('#').map_or(canonical, |(id, _)| id);
                if GtsTypeId::try_new(id).is_err() {
                    return Err(StoreError::InvalidRef(format!(
                        "'{ref_uri}' must reference a GTS type id (a valid identifier ending with '~')"
                    )));
                }
                out.insert(id.to_owned());
            }
            for v in map.values() {
                collect_gts_refs(v, depth + 1, out)?;
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_gts_refs(v, depth + 1, out)?;
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
                // bare id ref (no scheme) - normalized the same way
                "b": {"$ref": "gts.x.dep.ns.b.v1~"},
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
    fn extract_gts_refs_rejects_invalid() {
        let instance_ref = json!({
            "type": "object",
            "properties": {"a": {"$ref": "gts://gts.x.dep.ns.a.v1"}}
        });
        assert!(matches!(
            extract_gts_refs(&instance_ref),
            Err(StoreError::InvalidRef(_))
        ));

        let garbage_ref = json!({
            "type": "object",
            "properties": {"a": {"$ref": "gts://not a gts id"}}
        });
        assert!(matches!(
            extract_gts_refs(&garbage_ref),
            Err(StoreError::InvalidRef(_))
        ));
    }
}
