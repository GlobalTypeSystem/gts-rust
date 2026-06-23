//! `$ref` resolution for registered GTS schemas: inlining `gts://` and local
//! `#/` references into a self-contained body. A `$ref` target is inlined at a
//! non-root position, so its root-only keys (`$id`/`$schema` and the `x-gts-*`
//! type-level modifiers) are stripped on the way in. `allOf` and the other
//! combinators are preserved verbatim — composition is left to the JSON Schema
//! validator, not flattened here.
//!
//! [`SchemaResolver`] depends only on the narrow [`SchemaProvider`] lookup, not
//! on `GtsStore` directly; the store implements `SchemaProvider` and exposes
//! `resolve_schema_refs`/`try_resolve_schema_refs` as thin wrappers.

use serde_json::Value;

use crate::gts::GTS_URI_PREFIX;
use crate::store::StoreError;

/// Read-only schema lookup the resolver needs from its host.
///
/// Implemented by `GtsStore`; abstracts the resolver away from the store's
/// internals so it can be exercised against any registry-like source.
pub(crate) trait SchemaProvider {
    /// The registered schema document for the canonical type id `type_id`, or
    /// `None` if no *schema* with that id is registered. Implementations must
    /// return `None` for non-schema entities so a `$ref` to one stays
    /// unresolved rather than silently inlining a non-schema body.
    fn schema_content(&self, type_id: &str) -> Option<&Value>;
}

/// Inlines `$ref`s in a JSON Schema using a [`SchemaProvider`] for lookups.
pub(crate) struct SchemaResolver<'a> {
    provider: &'a dyn SchemaProvider,
}

impl<'a> SchemaResolver<'a> {
    pub(crate) fn new(provider: &'a dyn SchemaProvider) -> Self {
        Self { provider }
    }

    /// Best-effort `$ref` resolution: resolvable `gts://` `$ref`s are replaced
    /// with the referenced schema content; external refs that cannot be
    /// resolved are preserved in the returned value rather than removed. Use
    /// [`Self::try_resolve`] when unresolved refs must be treated as an error.
    pub(crate) fn resolve(&self, schema: &Value) -> Value {
        let mut visited = std::collections::HashSet::new();
        let mut cycle_found = false;
        let mut unresolved_refs = Vec::new();
        self.resolve_inner(schema, &mut visited, &mut cycle_found, &mut unresolved_refs)
    }

    /// Like [`Self::resolve`] but returns an error if any external `$ref`
    /// cannot be resolved or a circular `$ref` is detected.
    ///
    /// Uses DFS-path cycle detection: a `$ref` target is held in the seen-set
    /// only while its resolution is in progress on the current DFS stack and
    /// removed once that subtree finishes. Re-entry into an in-progress target
    /// is a true cycle. Multiple independent occurrences of the same `$ref`
    /// (e.g. duplicate refs in `allOf`) are NOT flagged — redundant manual
    /// aggregation across an `$id` chain is allowed.
    ///
    /// # Errors
    /// Returns [`StoreError::UnresolvedRefs`] if any external `$ref` cannot be
    /// resolved, or [`StoreError::CircularRef`] if a circular `$ref` is
    /// detected.
    pub(crate) fn try_resolve(&self, schema: &Value) -> Result<Value, StoreError> {
        let mut visited = std::collections::HashSet::new();
        let mut cycle_found = false;
        let mut unresolved_refs = Vec::new();
        let resolved =
            self.resolve_inner(schema, &mut visited, &mut cycle_found, &mut unresolved_refs);
        if cycle_found {
            Err(StoreError::CircularRef)
        } else if !unresolved_refs.is_empty() {
            Err(StoreError::UnresolvedRefs(unresolved_refs))
        } else {
            Ok(resolved)
        }
    }

    #[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
    fn resolve_inner(
        &self,
        schema: &Value,
        visited: &mut std::collections::HashSet<String>,
        cycle_found: &mut bool,
        unresolved_refs: &mut Vec<String>,
    ) -> Value {
        // Recursively resolve $ref references in the schema
        match schema {
            Value::Object(map) => {
                if let Some(Value::String(ref_uri)) = map.get("$ref") {
                    // Handle internal JSON Schema references like #/$defs/GtsInstanceId
                    // These should be inlined to match schemars 0.8 behavior (is_referenceable=false)
                    match ref_uri.as_str() {
                        "#/$defs/GtsInstanceId" => {
                            return crate::GtsInstanceId::json_schema_value();
                        }
                        "#/$defs/GtsTypeId" | "#/$defs/GtsSchemaId" => {
                            return crate::GtsTypeId::json_schema_value();
                        }
                        s if s.starts_with("#/") => {
                            // Other internal references - keep as-is
                            let mut new_map = serde_json::Map::new();
                            for (k, v) in map {
                                new_map.insert(
                                    k.clone(),
                                    self.resolve_inner(v, visited, cycle_found, unresolved_refs),
                                );
                            }
                            return Value::Object(new_map);
                        }
                        _ => {} // Fall through to external ref handling
                    }

                    // Normalize the ref: strip gts:// prefix to get canonical GTS ID
                    let canonical_ref = ref_uri.strip_prefix(GTS_URI_PREFIX).unwrap_or(ref_uri);
                    let (lookup_ref, pointer_fragment) =
                        if let Some((id, fragment)) = canonical_ref.split_once('#') {
                            let pointer = if fragment.is_empty() {
                                Some("")
                            } else if fragment.starts_with('/') {
                                Some(fragment)
                            } else {
                                None
                            };
                            (id, pointer)
                        } else {
                            (canonical_ref, None)
                        };

                    // Cycle detection: skip if we've already visited this ref
                    if visited.contains(canonical_ref) {
                        // Circular $ref detected. Keep this `$ref` in lenient
                        // output to avoid weakening the schema while preventing
                        // infinite recursion.
                        *cycle_found = true;
                        let mut new_map = serde_json::Map::new();
                        for (k, v) in map {
                            new_map.insert(
                                k.clone(),
                                if k == "$ref" {
                                    v.clone()
                                } else {
                                    self.resolve_inner(v, visited, cycle_found, unresolved_refs)
                                },
                            );
                        }
                        return Value::Object(new_map);
                    }

                    // Try to resolve the reference using the canonical ID
                    if let Some(content) = self.provider.schema_content(lookup_ref) {
                        let target_content = match pointer_fragment {
                            Some("") => Some(content),
                            Some(pointer) => content.pointer(pointer),
                            None if canonical_ref.contains('#') => None,
                            None => Some(content),
                        };

                        if let Some(target_content) = target_content {
                            // Mark as visited before recursing
                            visited.insert(canonical_ref.to_owned());
                            // Recursively resolve refs in the referenced schema
                            let mut resolved = self.resolve_inner(
                                target_content,
                                visited,
                                cycle_found,
                                unresolved_refs,
                            );
                            visited.remove(canonical_ref);

                            // The target is inlined at a non-root position (e.g. an
                            // `allOf` branch), so drop keys that are only meaningful at
                            // a type root: `$id`/`$schema` (URL/dialect resolution) and
                            // the type-level modifiers (they describe the referenced
                            // type, not the host; trait composition lives in
                            // `effective_traits`/`effective_traits_schema`). Everything
                            // else is preserved verbatim.
                            if let Value::Object(ref mut resolved_map) = resolved {
                                resolved_map.remove("$id");
                                resolved_map.remove("$schema");
                                resolved_map.remove(crate::schema_modifiers::X_GTS_FINAL);
                                resolved_map.remove(crate::schema_modifiers::X_GTS_ABSTRACT);
                                resolved_map.remove(crate::schema_traits::X_GTS_TRAITS);
                                resolved_map.remove(crate::schema_traits::X_GTS_TRAITS_SCHEMA);
                            }

                            // If the original object has only $ref, return the resolved schema
                            if map.len() == 1 {
                                return resolved;
                            }

                            // Otherwise combine the resolved schema with the
                            // siblings via `allOf`. A last-wins merge would let a
                            // sibling drop or loosen the target's constraints
                            // (e.g. `required`, `additionalProperties`).
                            match resolved {
                                Value::Object(resolved_map) => {
                                    let mut siblings = serde_json::Map::new();
                                    for (k, v) in map {
                                        if k != "$ref" {
                                            siblings.insert(
                                                k.clone(),
                                                self.resolve_inner(
                                                    v,
                                                    visited,
                                                    cycle_found,
                                                    unresolved_refs,
                                                ),
                                            );
                                        }
                                    }
                                    if siblings.is_empty() {
                                        return Value::Object(resolved_map);
                                    }
                                    let mut merged = serde_json::Map::new();
                                    merged.insert(
                                        "allOf".to_owned(),
                                        Value::Array(vec![
                                            Value::Object(resolved_map),
                                            Value::Object(siblings),
                                        ]),
                                    );
                                    return Value::Object(merged);
                                }
                                // Non-object target (e.g. a boolean schema via
                                // a pointer fragment) with siblings: `$ref`
                                // wins per JSON Schema precedence.
                                other => return other,
                            }
                        }
                    }
                    if !ref_uri.starts_with('#') {
                        unresolved_refs.push(ref_uri.clone());
                    }

                    // If we can't resolve, keep the $ref in lenient output. Dropping
                    // it would silently weaken the schema, especially when only
                    // annotation siblings such as `description` remain.
                    let mut new_map = serde_json::Map::new();
                    for (k, v) in map {
                        new_map.insert(
                            k.clone(),
                            if k == "$ref" {
                                v.clone()
                            } else {
                                self.resolve_inner(v, visited, cycle_found, unresolved_refs)
                            },
                        );
                    }
                    return Value::Object(new_map);
                }

                // `allOf` (and every other keyword) is handled by the generic
                // recursion below: each branch is resolved in place, preserving
                // the composition verbatim. We deliberately do NOT flatten
                // branches into one object — that dropped non-property keywords
                // and collapsed same-named properties to last-wins instead of
                // intersection. Trait composition lives in `effective_traits`/
                // `effective_traits_schema`, not in this resolved body.

                // Recursively process all properties
                let mut new_map = serde_json::Map::new();
                for (k, v) in map {
                    new_map.insert(
                        k.clone(),
                        self.resolve_inner(v, visited, cycle_found, unresolved_refs),
                    );
                }
                Value::Object(new_map)
            }
            Value::Array(arr) => Value::Array(
                arr.iter()
                    .map(|v| self.resolve_inner(v, visited, cycle_found, unresolved_refs))
                    .collect(),
            ),
            _ => schema.clone(),
        }
    }
}

#[cfg(test)]
#[path = "schema_resolver_test.rs"]
mod schema_resolver_test;
