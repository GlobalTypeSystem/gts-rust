//! `$ref` resolution for registered GTS schemas: inlining `gts://` and local
//! `#/` references into a self-contained body. A `$ref` target is inlined at a
//! non-root position, so its root-only keys (`$id`/`$schema` and the `x-gts-*`
//! type-level modifiers) are stripped on the way in. `allOf` and the other
//! combinators are preserved verbatim — composition is left to the JSON Schema
//! validator, not flattened here.
//!
//! [`SchemaResolver`] depends only on the narrow [`SchemaProvider`] lookup, not
//! on `GtsStore` directly; the store implements `SchemaProvider` and exposes
//! `resolve_schema_refs` as a thin wrapper.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::gts::GTS_ID_URI_PREFIX;

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

/// Maximum `$ref` chain depth before resolution fails.
const MAX_REF_CHAIN_DEPTH: usize = 32;

/// Inlines `$ref`s in a JSON Schema using a [`SchemaProvider`] for lookups.
pub(crate) struct SchemaResolver<'a> {
    provider: &'a dyn SchemaProvider,
    /// Every document the current resolution has met, and every resource
    /// embedded in one, mapped to its document. A same-document reference
    /// inside an embedded resource resolves from that resource.
    documents: RefCell<HashMap<ObjectKey, ObjectKey>>,
}

/// A JSON object, by identity.
type ObjectKey = *const Map<String, Value>;

impl<'a> SchemaResolver<'a> {
    pub(crate) fn new(provider: &'a dyn SchemaProvider) -> Self {
        Self {
            provider,
            documents: RefCell::default(),
        }
    }

    /// Strict `$ref` resolution that returns an error if any supported local
    /// JSON Pointer or external GTS `$ref` cannot be resolved, or a circular
    /// `$ref` is detected.
    ///
    /// Uses DFS-path cycle detection: a `$ref` target is held in the seen-set
    /// only while its resolution is in progress on the current DFS stack and
    /// removed once that subtree finishes. Re-entry into an in-progress target
    /// is a true cycle. Multiple independent occurrences of the same `$ref`
    /// (e.g. duplicate refs in `allOf`) are NOT flagged — redundant manual
    /// aggregation across an `$id` chain is allowed.
    ///
    /// # Errors
    /// Returns [`StoreError::UnresolvedRefs`] if any supported local JSON
    /// Pointer or external GTS `$ref` cannot be resolved, or
    /// [`StoreError::CircularRef`] if a circular `$ref` is detected.
    pub(crate) fn resolve(&self, schema: &Value) -> Result<Value, StoreError> {
        self.documents.borrow_mut().clear();
        self.register_document(schema);
        let mut visited = std::collections::HashSet::new();
        let mut cycle_found = false;
        let mut unresolved_refs = Vec::new();
        let resolved = self.resolve_inner(
            schema,
            schema,
            schema,
            &mut visited,
            &mut cycle_found,
            &mut unresolved_refs,
        );
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
        local_root: &Value,
        root_doc: &Value,
        visited: &mut std::collections::HashSet<String>,
        cycle_found: &mut bool,
        unresolved_refs: &mut Vec<String>,
    ) -> Value {
        // Recursively resolve $ref references in the schema
        match schema {
            Value::Object(map) => {
                let local_root = if self.is_embedded(schema) {
                    schema
                } else {
                    local_root
                };
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
                        s if s == "#" || s.starts_with("#/") => {
                            if let Some(pointer) = ref_uri.strip_prefix('#') {
                                let local_ref_key =
                                    format!("local:{:p}:{ref_uri}", std::ptr::from_ref(local_root));
                                if visited.contains(&local_ref_key) {
                                    // Only recursion inside the document being
                                    // resolved keeps the same anchor.
                                    if !self.is_in(local_root, root_doc) {
                                        *cycle_found = true;
                                    }
                                    return Value::Object(map.clone());
                                }
                                if visited.len() >= MAX_REF_CHAIN_DEPTH {
                                    unresolved_refs.push(format!(
                                        "{ref_uri} (nested deeper than \
                                         {MAX_REF_CHAIN_DEPTH} chained $refs)"
                                    ));
                                    return Value::Object(map.clone());
                                }
                                if let Some((target, target_root)) =
                                    self.locate(local_root, pointer)
                                {
                                    visited.insert(local_ref_key.clone());
                                    let resolved = self.resolve_inner(
                                        target,
                                        target_root,
                                        root_doc,
                                        visited,
                                        cycle_found,
                                        unresolved_refs,
                                    );
                                    visited.remove(&local_ref_key);
                                    return self.resolved_ref_with_siblings(
                                        map,
                                        resolved,
                                        local_root,
                                        root_doc,
                                        visited,
                                        cycle_found,
                                        unresolved_refs,
                                    );
                                }
                                unresolved_refs.push(ref_uri.clone());
                            }

                            // Unsupported or missing internal references are
                            // kept in the lenient output after being reported.
                            let mut new_map = serde_json::Map::new();
                            for (k, v) in map {
                                new_map.insert(
                                    k.clone(),
                                    self.resolve_inner(
                                        v,
                                        local_root,
                                        root_doc,
                                        visited,
                                        cycle_found,
                                        unresolved_refs,
                                    ),
                                );
                            }
                            return Value::Object(new_map);
                        }
                        _ => {} // Fall through to external ref handling
                    }

                    // Normalize the ref: strip gts:// prefix to get canonical GTS ID
                    let canonical_ref = ref_uri.strip_prefix(GTS_ID_URI_PREFIX).unwrap_or(ref_uri);
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
                                    self.resolve_inner(
                                        v,
                                        local_root,
                                        root_doc,
                                        visited,
                                        cycle_found,
                                        unresolved_refs,
                                    )
                                },
                            );
                        }
                        return Value::Object(new_map);
                    }

                    if visited.len() >= MAX_REF_CHAIN_DEPTH {
                        unresolved_refs.push(format!(
                            "{canonical_ref} (nested deeper than \
                             {MAX_REF_CHAIN_DEPTH} chained $refs)"
                        ));
                        return Value::Object(map.clone());
                    }

                    // Try to resolve the reference using the canonical ID
                    if let Some(content) = self.provider.schema_content(lookup_ref) {
                        self.register_document(content);
                        let target_content = match pointer_fragment {
                            Some(pointer) => self.locate(content, pointer),
                            None if canonical_ref.contains('#') => None,
                            None => Some((content, content)),
                        };

                        if let Some((target_content, target_root)) = target_content {
                            // Mark as visited before recursing
                            visited.insert(canonical_ref.to_owned());
                            // Recursively resolve refs in the referenced schema
                            let mut resolved = self.resolve_inner(
                                target_content,
                                target_root,
                                root_doc,
                                visited,
                                cycle_found,
                                unresolved_refs,
                            );
                            visited.remove(canonical_ref);

                            // Strip keys meaningful only at the referenced type's root.
                            if let Value::Object(ref mut resolved_map) = resolved {
                                resolved_map.remove("$id");
                                resolved_map.remove("$schema");
                                resolved_map.remove(crate::schema_modifiers::X_GTS_FINAL);
                                resolved_map.remove(crate::schema_modifiers::X_GTS_ABSTRACT);
                                resolved_map.remove(crate::schema_traits::X_GTS_TRAITS);
                                resolved_map.remove(crate::schema_traits::X_GTS_TRAITS_SCHEMA);
                            }

                            return self.resolved_ref_with_siblings(
                                map,
                                resolved,
                                local_root,
                                root_doc,
                                visited,
                                cycle_found,
                                unresolved_refs,
                            );
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
                                self.resolve_inner(
                                    v,
                                    local_root,
                                    root_doc,
                                    visited,
                                    cycle_found,
                                    unresolved_refs,
                                )
                            },
                        );
                    }
                    return Value::Object(new_map);
                }

                // Flattening combinators would change intersection semantics.

                // Recursively process all properties
                let mut new_map = serde_json::Map::new();
                for (k, v) in map {
                    new_map.insert(
                        k.clone(),
                        self.resolve_inner(
                            v,
                            local_root,
                            root_doc,
                            visited,
                            cycle_found,
                            unresolved_refs,
                        ),
                    );
                }
                Value::Object(new_map)
            }
            Value::Array(arr) => Value::Array(
                arr.iter()
                    .map(|v| {
                        self.resolve_inner(
                            v,
                            local_root,
                            root_doc,
                            visited,
                            cycle_found,
                            unresolved_refs,
                        )
                    })
                    .collect(),
            ),
            _ => schema.clone(),
        }
    }

    /// Combines a resolved `$ref` target with its sibling constraints.
    #[allow(clippy::too_many_arguments)]
    fn resolved_ref_with_siblings(
        &self,
        map: &serde_json::Map<String, Value>,
        resolved: Value,
        local_root: &Value,
        root_doc: &Value,
        visited: &mut std::collections::HashSet<String>,
        cycle_found: &mut bool,
        unresolved_refs: &mut Vec<String>,
    ) -> Value {
        // If the original object has only $ref, return the resolved schema.
        if map.len() == 1 {
            return resolved;
        }

        // Otherwise combine the resolved schema with the siblings via `allOf`.
        // A last-wins merge would let a sibling drop or loosen the target's
        // constraints (e.g. `required`, `additionalProperties`).
        let resolved_contains_local_ref = crate::schema_modifiers::contains_local_ref(&resolved);
        match resolved {
            Value::Object(resolved_map) => {
                // Keep recursive local pointers at their authored position.
                if resolved_contains_local_ref {
                    let mut preserved = serde_json::Map::new();
                    for (k, v) in map {
                        if k == "$ref" {
                            preserved.insert(k.clone(), v.clone());
                        } else {
                            preserved.insert(
                                k.clone(),
                                self.resolve_inner(
                                    v,
                                    local_root,
                                    root_doc,
                                    visited,
                                    cycle_found,
                                    unresolved_refs,
                                ),
                            );
                        }
                    }
                    return Value::Object(preserved);
                }

                let mut siblings = serde_json::Map::new();
                for (k, v) in map {
                    if k != "$ref" {
                        siblings.insert(
                            k.clone(),
                            self.resolve_inner(
                                v,
                                local_root,
                                root_doc,
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
                    Value::Array(vec![Value::Object(resolved_map), Value::Object(siblings)]),
                );
                Value::Object(merged)
            }
            // Non-object target (e.g. a boolean schema via a pointer fragment)
            // with siblings: `$ref` wins per JSON Schema precedence.
            other => other,
        }
    }

    /// Records `document` and the resources embedded in it, once.
    fn register_document(&self, document: &Value) {
        let Some(key) = object_key(document) else {
            return;
        };
        let mut documents = self.documents.borrow_mut();
        if documents.insert(key, key).is_some() {
            return;
        }
        for resource in crate::schema_modifiers::embedded_resources(document) {
            documents.insert(resource, key);
        }
    }

    /// Whether `node` is a resource embedded in a document.
    fn is_embedded(&self, node: &Value) -> bool {
        object_key(node).is_some_and(|key| {
            self.documents
                .borrow()
                .get(&key)
                .is_some_and(|document| *document != key)
        })
    }

    /// Whether `resource` is `document` or embedded in it.
    fn is_in(&self, resource: &Value, document: &Value) -> bool {
        std::ptr::eq(resource, document)
            || object_key(resource).is_some_and(|key| {
                self.documents.borrow().get(&key).copied() == object_key(document)
            })
    }

    /// The node JSON Pointer `pointer` names inside `resource`, and the
    /// resource its own same-document references resolve from: the innermost
    /// one on the way there.
    fn locate<'v>(&self, resource: &'v Value, pointer: &str) -> Option<(&'v Value, &'v Value)> {
        let mut node = resource;
        let mut innermost = resource;
        if !pointer.is_empty() {
            for token in pointer.strip_prefix('/')?.split('/') {
                let token = token.replace("~1", "/").replace("~0", "~");
                node = match node {
                    Value::Object(map) => map.get(&token)?,
                    Value::Array(items) => items.get(array_index(&token)?)?,
                    _ => return None,
                };
                if self.is_embedded(node) {
                    innermost = node;
                }
            }
        }
        Some((node, innermost))
    }
}

fn object_key(value: &Value) -> Option<ObjectKey> {
    value.as_object().map(std::ptr::from_ref)
}

/// An array index token, read as `serde_json`'s own pointer lookup reads one.
fn array_index(token: &str) -> Option<usize> {
    if token.starts_with('+') || (token.starts_with('0') && token.len() != 1) {
        return None;
    }
    token.parse().ok()
}

#[cfg(test)]
mod tests {
    use crate::schema_modifiers::contains_local_ref;
    use serde_json::json;

    #[test]
    fn local_ref_detection_only_visits_schema_locations() {
        assert!(!contains_local_ref(&json!({
            "const": {"$ref": "#/$defs/literal"},
            "default": {"$ref": "#/$defs/literal"},
            "examples": [{"$ref": "#/$defs/literal"}],
            "enum": [{"$ref": "#/$defs/literal"}],
            "properties": {
                "$ref": {"const": "a property name, not a reference"}
            }
        })));

        assert!(contains_local_ref(&json!({
            "properties": {
                "child": {"$ref": "#/$defs/child"}
            }
        })));
    }
}

#[cfg(test)]
#[path = "schema_resolver_test.rs"]
mod schema_resolver_test;
