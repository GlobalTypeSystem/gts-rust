//! `$ref` resolution for registered GTS schemas: inlining `gts://` and local
//! `#/` references into the body. A `$ref` target is inlined at a non-root
//! position, so its root-only keys (`$id`/`$schema` and the `x-gts-*`
//! type-level modifiers) are stripped on the way in. `allOf` and the other
//! combinators are preserved verbatim — composition is left to the JSON Schema
//! validator, not flattened here.
//!
//! Only schema positions are walked; `default`, `const` and other data are
//! copied verbatim.
//!
//! [`SchemaResolver`] depends only on the narrow [`SchemaProvider`] lookup, not
//! on `GtsStore` directly; the store implements `SchemaProvider` and exposes
//! `resolve_schema_refs` as a thin wrapper.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use jsonschema::Draft;
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

/// How completely references must be inlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ResolutionMode {
    /// Recursive same-document `$ref`s are kept, so recursive schemas resolve.
    #[default]
    Partial,
    /// No reference may remain; recursion is [`StoreError::CircularRef`].
    Full,
}

/// Inlines `$ref`s in a JSON Schema using a [`SchemaProvider`] for lookups.
pub(crate) struct SchemaResolver<'a> {
    provider: &'a dyn SchemaProvider,
    resolution_mode: ResolutionMode,
    /// Keep resource references when resolving a recursive fragment.
    contextual: bool,
    /// Every document the current resolution has met, and every resource
    /// embedded in one, mapped to its document. A same-document reference
    /// inside an embedded resource resolves from that resource.
    documents: RefCell<HashMap<ObjectKey, ObjectKey>>,
    /// The host document's dialect; GTS forbids dialect switches.
    dialect: Cell<Draft>,
    /// Whether the dialect is Draft-07, which ignores `$ref` siblings.
    ignores_ref_siblings: Cell<bool>,
    /// Whether the node being resolved is its host document.
    resolves_host: Cell<bool>,
}

/// A JSON object, by identity.
type ObjectKey = *const Map<String, Value>;

#[derive(Default)]
struct Walk {
    /// `$ref` targets on the current DFS path.
    visited: HashSet<String>,
    /// External targets whose type-level metadata is omitted.
    inlined_roots: HashSet<ObjectKey>,
    cycle_found: bool,
    unresolved_refs: Vec<String>,
    needs_bundle: bool,
}

impl<'a> SchemaResolver<'a> {
    pub(crate) fn new(provider: &'a dyn SchemaProvider) -> Self {
        Self {
            provider,
            resolution_mode: ResolutionMode::Partial,
            contextual: false,
            documents: RefCell::default(),
            dialect: Cell::new(Draft::default()),
            ignores_ref_siblings: Cell::default(),
            resolves_host: Cell::default(),
        }
    }

    pub(crate) fn with_resolution_mode(mut self, resolution_mode: ResolutionMode) -> Self {
        self.resolution_mode = resolution_mode;
        self
    }

    /// # Errors
    /// See [`Self::resolve_in`].
    pub(crate) fn resolve(&self, schema: &Value) -> Result<Value, StoreError> {
        self.resolve_in(schema, schema)
    }

    /// Resolves `node`, the document `host` or a schema inside it; local
    /// pointers and the dialect come from `host`. Under
    /// [`ResolutionMode::Partial`], a fragment's recursive pointer into `host`
    /// is rebased onto `host`'s `$id`, and `host` is bundled into the result.
    ///
    /// Cycle detection is per DFS path: repeated independent `$ref`s to one
    /// target (e.g. duplicates in `allOf`) are not recursion.
    ///
    /// # Errors
    /// [`StoreError::CircularRef`] for recursion that is not kept;
    /// [`StoreError::UnresolvedRefs`] for an unresolvable `$ref`, or under
    /// [`ResolutionMode::Full`] any reference left (`#anchor`, `$dynamicRef`).
    pub(crate) fn resolve_in(&self, node: &Value, host: &Value) -> Result<Value, StoreError> {
        self.documents.borrow_mut().clear();
        let dialect = Draft::default().detect(host);
        self.dialect.set(dialect);
        self.ignores_ref_siblings.set(dialect == Draft::Draft7);
        self.resolves_host.set(std::ptr::eq(node, host));
        self.register_document(host);
        let mut walk = Walk::default();
        let resolved = self.resolve_inner(node, host, host, &mut walk);
        if (walk.cycle_found || walk.needs_bundle)
            && walk.unresolved_refs.is_empty()
            && self.resolution_mode == ResolutionMode::Partial
            && !self.resolves_host.get()
            && !self.contextual
            && resource_uri(host).is_some()
        {
            return Self {
                contextual: true,
                ..Self::new(self.provider)
            }
            .resolve_in(node, host);
        }
        if walk.cycle_found {
            return Err(StoreError::CircularRef);
        }
        if !walk.unresolved_refs.is_empty() {
            return Err(StoreError::UnresolvedRefs(walk.unresolved_refs));
        }
        if self.resolution_mode == ResolutionMode::Full {
            let remaining = remaining_refs(&resolved, dialect);
            if !remaining.is_empty() {
                return Err(StoreError::UnresolvedRefs(remaining));
            }
        }
        if walk.needs_bundle {
            return self.bundle_resources(resolved, host);
        }
        Ok(resolved)
    }

    /// Embed authored resources so references keep their original targets.
    fn bundle_resources(&self, mut fragment: Value, host: &Value) -> Result<Value, StoreError> {
        let mut resources = HashMap::new();
        let mut pending = vec![host];
        while let Some(document) = pending.pop() {
            let id = resource_uri(document).ok_or(StoreError::CircularRef)?;
            if resources.insert(id.to_owned(), document).is_some() {
                continue;
            }
            let mut references = Vec::new();
            crate::schema_modifiers::for_each_schema_node_in_dialect(
                document,
                self.dialect.get(),
                &mut |node, _, _| {
                    if let Some(reference) = node
                        .get("$ref")
                        .and_then(Value::as_str)
                        .and_then(|uri| uri.strip_prefix(GTS_ID_URI_PREFIX))
                    {
                        let (id, _) = reference.split_once('#').unwrap_or((reference, ""));
                        references.push(id.to_owned());
                    }
                },
            );
            for reference in references {
                if resources.contains_key(&format!("{GTS_ID_URI_PREFIX}{reference}")) {
                    continue;
                }
                let document = self.provider.schema_content(&reference).ok_or_else(|| {
                    StoreError::UnresolvedRefs(vec![format!("{GTS_ID_URI_PREFIX}{reference}")])
                })?;
                pending.push(document);
            }
        }
        let defs = if self.dialect.get() == Draft::Draft7 {
            "definitions"
        } else {
            "$defs"
        };
        let Some(Value::Object(defs)) = fragment
            .as_object_mut()
            .map(|map| map.entry(defs).or_insert_with(|| Value::Object(Map::new())))
        else {
            return Err(StoreError::CircularRef);
        };
        for (id, document) in resources {
            let mut key = id;
            while defs.contains_key(&key) {
                key.push('_');
            }
            defs.insert(key, document.clone());
        }
        Ok(fragment)
    }

    fn resolve_inner(
        &self,
        schema: &Value,
        local_root: &Value,
        root_doc: &Value,
        walk: &mut Walk,
    ) -> Value {
        let Value::Object(map) = schema else {
            return schema.clone();
        };
        let local_root = if self.is_embedded(schema) {
            schema
        } else {
            local_root
        };
        let Some(Value::String(ref_uri)) = map.get("$ref") else {
            // Flattening combinators would change intersection semantics.
            return Value::Object(self.resolve_keywords(schema, local_root, root_doc, walk));
        };

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
                let pointer = &ref_uri[1..];
                if self.contextual
                    && pointer.is_empty()
                    && let Some(id) = resource_uri(local_root)
                {
                    walk.needs_bundle = true;
                    let resolved = serde_json::json!({"$ref": format!("{id}{ref_uri}")});
                    return self
                        .resolved_ref_with_siblings(schema, resolved, local_root, root_doc, walk);
                }
                let local_ref_key = format!("local:{:p}:{ref_uri}", std::ptr::from_ref(local_root));
                if walk.visited.contains(&local_ref_key) {
                    let mut kept = map.clone();
                    if (self.contextual || self.rebases_recursion(local_root, root_doc))
                        && let Some(id) = resource_uri(local_root)
                    {
                        kept.insert("$ref".to_owned(), Value::String(format!("{id}{ref_uri}")));
                        walk.needs_bundle = true;
                    } else if !self.keeps_recursion(local_root, root_doc) {
                        walk.cycle_found = true;
                    }
                    return Value::Object(kept);
                }
                if walk.visited.len() >= MAX_REF_CHAIN_DEPTH {
                    walk.unresolved_refs.push(format!(
                        "{ref_uri} (nested deeper than {MAX_REF_CHAIN_DEPTH} chained $refs)"
                    ));
                    return Value::Object(map.clone());
                }
                if let Some((target, target_root)) = self.locate(local_root, pointer) {
                    walk.visited.insert(local_ref_key.clone());
                    let resolved = self.resolve_inner(target, target_root, root_doc, walk);
                    walk.visited.remove(&local_ref_key);
                    return self
                        .resolved_ref_with_siblings(schema, resolved, local_root, root_doc, walk);
                }
                walk.unresolved_refs.push(ref_uri.clone());

                // Keep the reference while collecting resolution errors.
                return Value::Object(self.resolve_keywords(schema, local_root, root_doc, walk));
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
        if walk.visited.contains(canonical_ref) {
            // Keep the cycle edge rather than weakening the schema.
            if self.contextual {
                walk.needs_bundle = true;
            } else {
                walk.cycle_found = true;
            }
            return Value::Object(self.resolve_keywords(schema, local_root, root_doc, walk));
        }

        if walk.visited.len() >= MAX_REF_CHAIN_DEPTH {
            walk.unresolved_refs.push(format!(
                "{canonical_ref} (nested deeper than {MAX_REF_CHAIN_DEPTH} chained $refs)"
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
                walk.visited.insert(canonical_ref.to_owned());
                let inlined_root =
                    object_key(target_content).filter(|key| walk.inlined_roots.insert(*key));
                // Recursively resolve refs in the referenced schema
                let mut resolved = self.resolve_inner(target_content, target_root, root_doc, walk);
                if let Some(key) = inlined_root {
                    walk.inlined_roots.remove(&key);
                }
                walk.visited.remove(canonical_ref);

                // Strip keys meaningful only at the referenced type's root.
                if let Value::Object(ref mut resolved_map) = resolved {
                    resolved_map.retain(|key, _| !is_type_root_keyword(key));
                }

                return self
                    .resolved_ref_with_siblings(schema, resolved, local_root, root_doc, walk);
            }
        }
        if !ref_uri.starts_with('#') {
            walk.unresolved_refs.push(ref_uri.clone());
        }

        // Keep unresolved references while collecting errors; dropping them
        // would weaken the schema.
        Value::Object(self.resolve_keywords(schema, local_root, root_doc, walk))
    }

    /// `node`'s keywords with its subschemas resolved and data copied.
    fn resolve_keywords(
        &self,
        node: &Value,
        local_root: &Value,
        root_doc: &Value,
        walk: &mut Walk,
    ) -> Map<String, Value> {
        let Value::Object(map) = node else {
            return Map::new();
        };
        let subschemas = crate::schema_modifiers::subschemas_of(node, self.dialect.get());
        let inlined = walk.inlined_roots.contains(&std::ptr::from_ref(map));
        map.iter()
            .filter(|(key, _)| !inlined || !is_type_root_keyword(key))
            .map(|(k, v)| {
                let v = self.resolve_subschemas(v, &subschemas, local_root, root_doc, walk);
                (k.clone(), v)
            })
            .collect()
    }

    fn resolve_subschemas(
        &self,
        value: &Value,
        subschemas: &[&Value],
        local_root: &Value,
        root_doc: &Value,
        walk: &mut Walk,
    ) -> Value {
        if crate::schema_modifiers::is_direct_subresource(value, subschemas) {
            return self.resolve_inner(value, local_root, root_doc, walk);
        }
        match value {
            Value::Object(map) if !subschemas.is_empty() => Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        let v = self.resolve_subschemas(v, subschemas, local_root, root_doc, walk);
                        (k.clone(), v)
                    })
                    .collect(),
            ),
            Value::Array(items) if !subschemas.is_empty() => Value::Array(
                items
                    .iter()
                    .map(|v| self.resolve_subschemas(v, subschemas, local_root, root_doc, walk))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }

    /// Combines a resolved `$ref` target with its sibling constraints.
    fn resolved_ref_with_siblings(
        &self,
        node: &Value,
        resolved: Value,
        local_root: &Value,
        root_doc: &Value,
        walk: &mut Walk,
    ) -> Value {
        let Value::Object(map) = node else {
            return resolved;
        };
        // If the original object has only $ref, return the resolved schema.
        if map.len() == 1 {
            return resolved;
        }

        // Keep recursive local pointers as authored. Their siblings are resolved
        // even in Draft-07: a kept pointer can still reach into one (a root
        // `$ref` into its `definitions`).
        if crate::schema_modifiers::contains_local_ref(&resolved, self.dialect.get()) {
            return Value::Object(self.resolve_keywords(node, local_root, root_doc, walk));
        }

        // Draft-07 ignores `$ref` siblings; a root keeps its `$id` and `$schema`.
        if self.ignores_ref_siblings.get() {
            if !root_doc
                .as_object()
                .is_some_and(|root| std::ptr::eq(root, map))
            {
                return resolved;
            }
            let mut identity: Map<String, Value> = ["$id", "$schema"]
                .into_iter()
                .filter_map(|key| map.get(key).map(|v| (key.to_owned(), v.clone())))
                .collect();
            if identity.is_empty() {
                return resolved;
            }
            identity.insert("allOf".to_owned(), Value::Array(vec![resolved]));
            return Value::Object(identity);
        }

        // Later dialects apply siblings with the target: the target joins their
        // `allOf`, so `unevaluated*` siblings still see its annotations.
        let mut all_of = vec![resolved];
        let mut composed = Map::new();
        for (k, v) in self.resolve_keywords(node, local_root, root_doc, walk) {
            match (k.as_str(), v) {
                ("$ref", _) => {}
                ("allOf", Value::Array(branches)) => all_of.extend(branches),
                // Malformed: left for the meta-schema to reject.
                ("allOf", v) => all_of.push(serde_json::json!({ "allOf": v })),
                (_, v) => {
                    composed.insert(k, v);
                }
            }
        }
        composed.insert("allOf".to_owned(), Value::Array(all_of));
        Value::Object(composed)
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

    /// Whether a recursive pointer resolved from `resource` may stay: it must
    /// name the same node in the result. An embedded resource keeps its `$id`;
    /// `root_doc` stays the root only when resolved whole, not as a fragment.
    fn keeps_recursion(&self, resource: &Value, root_doc: &Value) -> bool {
        self.resolution_mode == ResolutionMode::Partial
            && self.is_in(resource, root_doc)
            && (self.resolves_host.get() || !std::ptr::eq(resource, root_doc))
    }

    /// Whether a recursive pointer resolved from `root_doc` must be rebased: it
    /// would dangle in a fragment.
    fn rebases_recursion(&self, resource: &Value, root_doc: &Value) -> bool {
        self.resolution_mode == ResolutionMode::Partial
            && !self.resolves_host.get()
            && std::ptr::eq(resource, root_doc)
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

/// The resource URI used when bundling or rebasing references.
fn resource_uri(resource: &Value) -> Option<&str> {
    resource
        .get("$id")
        .and_then(Value::as_str)
        .filter(|id| id.contains("://") && !id.contains('#'))
}

fn object_key(value: &Value) -> Option<ObjectKey> {
    value.as_object().map(std::ptr::from_ref)
}

fn is_type_root_keyword(key: &str) -> bool {
    matches!(
        key,
        "$id"
            | "$schema"
            | crate::schema_modifiers::X_GTS_FINAL
            | crate::schema_modifiers::X_GTS_ABSTRACT
            | crate::schema_traits::X_GTS_TRAITS
            | crate::schema_traits::X_GTS_TRAITS_SCHEMA
    )
}

/// References left in schema positions of `schema`.
fn remaining_refs(schema: &Value, dialect: Draft) -> Vec<String> {
    let mut refs = Vec::new();
    crate::schema_modifiers::for_each_schema_node_in_dialect(
        schema,
        dialect,
        &mut |node, _, scope| {
            let keywords: &[&str] = match scope.dialect {
                Draft::Draft201909 => &["$ref", "$recursiveRef"],
                Draft::Draft202012 | Draft::Unknown => &["$ref", "$dynamicRef"],
                _ => &["$ref"],
            };
            refs.extend(
                keywords
                    .iter()
                    .filter_map(|keyword| node.get(*keyword).and_then(Value::as_str))
                    .map(str::to_owned),
            );
        },
    );
    refs
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
        let draft = jsonschema::Draft::default();
        assert!(!contains_local_ref(
            &json!({
                "const": {"$ref": "#/$defs/literal"},
                "default": {"$ref": "#/$defs/literal"},
                "examples": [{"$ref": "#/$defs/literal"}],
                "enum": [{"$ref": "#/$defs/literal"}],
                "properties": {
                    "$ref": {"const": "a property name, not a reference"}
                }
            }),
            draft
        ));

        assert!(contains_local_ref(
            &json!({
                "properties": {
                    "child": {"$ref": "#/$defs/child"}
                }
            }),
            draft
        ));
    }
}

#[cfg(test)]
#[path = "schema_resolver_test.rs"]
mod schema_resolver_test;
