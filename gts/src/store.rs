use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};
use thiserror::Error;

use crate::entities::GtsEntity;
use crate::gts::{GTS_URI_PREFIX, GtsId, GtsIdPattern};
use crate::schema_cast::GtsEntityCastResult;

/// Custom retriever for resolving gts:// URI scheme references in JSON Schema validation
struct GtsRetriever {
    store: Arc<RwLock<HashMap<String, Value>>>,
}

impl GtsRetriever {
    fn new(store_map: &HashMap<String, GtsEntity>) -> Self {
        let mut schemas = HashMap::new();

        // Pre-populate with all schemas from the store
        for (id, entity) in store_map {
            if entity.is_schema {
                // Store with gts:// URI format
                let uri = format!("{GTS_URI_PREFIX}{id}");
                schemas.insert(uri, entity.content.clone());
            }
        }

        Self {
            store: Arc::new(RwLock::new(schemas)),
        }
    }
}

impl jsonschema::Retrieve for GtsRetriever {
    #[allow(clippy::cognitive_complexity)]
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let uri_str = uri.as_str();

        tracing::debug!("GtsRetriever: Attempting to retrieve URI: {uri_str}");

        // Only handle gts:// URIs
        if !uri_str.starts_with(GTS_URI_PREFIX) {
            tracing::warn!("GtsRetriever: Unknown scheme for URI: {uri_str}");
            return Err(format!("Unknown scheme for URI: {uri_str}").into());
        }

        let store = self.store.read().map_err(|e| format!("Lock error: {e}"))?;

        tracing::debug!("GtsRetriever: Store contains {} schemas", store.len());

        if let Some(schema) = store.get(uri_str) {
            tracing::debug!("GtsRetriever: Successfully retrieved schema for {uri_str}");
            Ok(schema.clone())
        } else {
            tracing::warn!("GtsRetriever: Schema not found: {uri_str}");
            tracing::debug!(
                "GtsRetriever: Available URIs: {:?}",
                store.keys().collect::<Vec<_>>()
            );
            Err(format!("Schema not found: {uri_str}").into())
        }
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("JSON object with GTS ID '{0}' not found in store")]
    ObjectNotFound(String),
    #[error("JSON schema with GTS ID '{0}' not found in store")]
    SchemaNotFound(String),
    #[error("JSON entity with GTS ID '{0}' not found in store")]
    EntityNotFound(String),
    #[error("Can't determine JSON schema ID for instance with GTS ID '{0}'")]
    SchemaForInstanceNotFound(String),
    #[error(
        "Cannot cast from schema ID '{0}'. The from_id must be an instance (not ending with '~')"
    )]
    CastFromSchemaNotAllowed(String),
    #[error("Entity must have a valid gts_id")]
    InvalidEntity,
    #[error("Schema type_id must end with '~'")]
    InvalidSchemaId,
    #[error("{0}")]
    ValidationError(String),
    #[error("Invalid $ref: {0}")]
    InvalidRef(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveSchemaRefsError {
    CircularRef,
    UnresolvedRefs(Vec<String>),
}

impl fmt::Display for ResolveSchemaRefsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CircularRef => write!(f, "circular $ref detected"),
            Self::UnresolvedRefs(refs) => write!(f, "unresolved $ref(s): {}", refs.join(", ")),
        }
    }
}

impl std::error::Error for ResolveSchemaRefsError {}

pub trait GtsReader: Send {
    fn iter(&mut self) -> Box<dyn Iterator<Item = GtsEntity> + '_>;
    fn read_by_id(&self, entity_id: &str) -> Option<GtsEntity>;
    fn reset(&mut self);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsStoreQueryResult {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    pub count: usize,
    pub limit: usize,
    pub results: Vec<Value>,
}

/// Fully-resolved, self-contained view of a GTS type.
///
/// A pure value computed from store contents — the library holds **no cache**
/// of these. Because schemas are append-only by versioned id (a new version is
/// a new `type_id`), a `ResolvedType` is safe for a *consumer* to cache forever
/// keyed by `type_id`.
#[derive(Debug, Clone)]
pub struct ResolvedTypeSchema {
    /// Type body with all `#/` and `gts://` `$ref`s inlined.
    pub resolved_schema: Value,
    /// Chain-merged (RFC 7396) and default-materialized trait values.
    pub effective_traits: Value,
    /// Dialect-pinned, `allOf`-composed, `$ref`-inlined effective traits schema.
    pub effective_traits_schema: Value,
    /// `true` when the type declares `x-gts-abstract: true`.
    pub is_abstract: bool,
}

pub struct GtsStore {
    by_id: HashMap<String, GtsEntity>,
    reader: Option<Box<dyn GtsReader>>,
}

impl Default for GtsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl GtsStore {
    /// Empty, reader-free store. Callers populate it explicitly via
    /// [`Self::register`] / [`Self::register_schema`]. With no [`GtsReader`],
    /// `get` and resolution never fall back to lazy I/O — the store sees
    /// exactly what was registered.
    #[must_use]
    pub fn new() -> Self {
        GtsStore {
            by_id: HashMap::new(),
            reader: None,
        }
    }

    /// Store backed by a [`GtsReader`], eagerly populated from it. `get` falls
    /// back to the reader for ids not yet cached.
    #[must_use]
    pub fn with_reader(reader: Box<dyn GtsReader>) -> Self {
        let mut store = GtsStore {
            by_id: HashMap::new(),
            reader: Some(reader),
        };
        store.populate_from_reader();
        tracing::info!("Populated GtsStore with {} entities", store.by_id.len());
        store
    }

    fn populate_from_reader(&mut self) {
        if let Some(ref mut reader) = self.reader {
            for entity in reader.iter() {
                // Use effective_id() which handles both GTS IDs and anonymous instance IDs
                if let Some(id) = entity.effective_id() {
                    self.by_id.insert(id, entity);
                }
            }
        }
    }

    /// Registers an entity in the store.
    ///
    /// # Errors
    /// Returns `StoreError::InvalidEntity` if the entity has no effective ID.
    pub fn register(&mut self, entity: GtsEntity) -> Result<(), StoreError> {
        let id = entity.effective_id().ok_or(StoreError::InvalidEntity)?;
        self.by_id.insert(id, entity);
        Ok(())
    }

    /// Registers a schema in the store.
    ///
    /// # Errors
    /// Returns `StoreError::InvalidSchemaId` if the `type_id` doesn't end with '~'.
    pub fn register_schema(&mut self, type_id: &str, schema: &Value) -> Result<(), StoreError> {
        if !type_id.ends_with('~') {
            return Err(StoreError::InvalidSchemaId);
        }

        let gts_id = GtsId::try_new(type_id).map_err(|_| StoreError::InvalidSchemaId)?;
        let entity = GtsEntity::new(
            None,
            None,
            schema,
            None,
            Some(gts_id),
            true,
            String::new(),
            None,
            None,
        );
        self.by_id.insert(type_id.to_owned(), entity);
        Ok(())
    }

    pub fn get(&mut self, entity_id: &str) -> Option<&GtsEntity> {
        // Check cache first
        if self.by_id.contains_key(entity_id) {
            return self.by_id.get(entity_id);
        }

        // Try to fetch from reader
        if let Some(ref reader) = self.reader
            && let Some(entity) = reader.read_by_id(entity_id)
        {
            self.by_id.insert(entity_id.to_owned(), entity);
            return self.by_id.get(entity_id);
        }

        None
    }

    /// Gets the content of a schema by its type ID.
    ///
    /// # Errors
    /// Returns `StoreError::SchemaNotFound` if the schema is not found.
    pub fn get_schema_content(&mut self, type_id: &str) -> Result<Value, StoreError> {
        if let Some(entity) = self.get(type_id) {
            return Ok(entity.content.clone());
        }
        Err(StoreError::SchemaNotFound(type_id.to_owned()))
    }

    pub fn items(&self) -> impl Iterator<Item = (&String, &GtsEntity)> {
        self.by_id.iter()
    }

    /// Best-effort `$ref` resolution for a JSON Schema.
    ///
    /// This method recursively traverses the schema and replaces resolvable
    /// `gts://` `$ref`s with the actual schema content from the store. External
    /// refs that cannot be resolved are preserved in the returned value rather
    /// than removed. Use [`Self::resolve_schema_refs_checked`] when unresolved
    /// refs must be treated as an error.
    ///
    /// # Arguments
    ///
    /// * `schema` - The JSON Schema value that may contain `$ref` references
    ///
    /// # Returns
    ///
    /// A new `serde_json::Value` with all resolvable `$ref` references inlined
    /// and unresolved refs left intact.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use gts::GtsStore;
    /// let store = GtsStore::new();
    ///
    /// // Add schemas to store
    /// store.add_schema_json("parent.v1~", parent_schema)?;
    /// store.add_schema_json("child.v1~", child_schema_with_ref)?;
    ///
    /// // Resolve references
    /// let inlined = store.resolve_schema_refs(&child_schema_with_ref);
    /// ```
    #[must_use]
    pub fn resolve_schema_refs(&self, schema: &Value) -> Value {
        let mut visited = std::collections::HashSet::new();
        let mut cycle_found = false;
        let mut unresolved_refs = Vec::new();
        self.resolve_schema_refs_inner(schema, &mut visited, &mut cycle_found, &mut unresolved_refs)
    }

    /// Like [`resolve_schema_refs`] but returns an error if any external
    /// `$ref` cannot be resolved or a circular `$ref` is detected.
    ///
    /// Uses DFS-path cycle detection: a `$ref` target is held in the seen-set
    /// only while its resolution is in progress on the current DFS stack and
    /// removed once that subtree finishes. Re-entry into an in-progress
    /// target is a true cycle. Multiple independent occurrences of the same
    /// `$ref` (e.g. duplicate refs in `allOf`) are NOT flagged — redundant
    /// manual aggregation across an `$id` chain is allowed.
    ///
    /// # Errors
    /// Returns [`ResolveSchemaRefsError::UnresolvedRefs`] if any external
    /// `$ref` cannot be resolved, or [`ResolveSchemaRefsError::CircularRef`]
    /// if a circular `$ref` is detected.
    pub fn resolve_schema_refs_checked(
        &self,
        schema: &Value,
    ) -> Result<Value, ResolveSchemaRefsError> {
        let mut visited = std::collections::HashSet::new();
        let mut cycle_found = false;
        let mut unresolved_refs = Vec::new();
        let resolved = self.resolve_schema_refs_inner(
            schema,
            &mut visited,
            &mut cycle_found,
            &mut unresolved_refs,
        );
        if cycle_found {
            Err(ResolveSchemaRefsError::CircularRef)
        } else if !unresolved_refs.is_empty() {
            Err(ResolveSchemaRefsError::UnresolvedRefs(unresolved_refs))
        } else {
            Ok(resolved)
        }
    }

    #[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
    fn resolve_schema_refs_inner(
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
                                    self.resolve_schema_refs_inner(
                                        v,
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
                                    self.resolve_schema_refs_inner(
                                        v,
                                        visited,
                                        cycle_found,
                                        unresolved_refs,
                                    )
                                },
                            );
                        }
                        return Value::Object(new_map);
                    }

                    // Try to resolve the reference using canonical ID
                    if let Some(entity) = self.by_id.get(lookup_ref)
                        && entity.is_schema
                    {
                        let target_content = match pointer_fragment {
                            Some("") => Some(&entity.content),
                            Some(pointer) => entity.content.pointer(pointer),
                            None if canonical_ref.contains('#') => None,
                            None => Some(&entity.content),
                        };

                        if let Some(target_content) = target_content {
                            // Mark as visited before recursing
                            visited.insert(canonical_ref.to_owned());
                            // Recursively resolve refs in the referenced schema
                            let mut resolved = self.resolve_schema_refs_inner(
                                target_content,
                                visited,
                                cycle_found,
                                unresolved_refs,
                            );
                            visited.remove(canonical_ref);

                            // Remove $id and $schema from resolved content to avoid URL resolution issues
                            // Note: $defs for GtsInstanceId/GtsTypeId are inlined during resolution (see match above)
                            if let Value::Object(ref mut resolved_map) = resolved {
                                resolved_map.remove("$id");
                                resolved_map.remove("$schema");
                            }

                            // If the original object has only $ref, return the resolved schema
                            if map.len() == 1 {
                                return resolved;
                            }

                            // Otherwise, merge the resolved schema with other properties
                            if let Value::Object(resolved_map) = resolved {
                                let mut merged = resolved_map;
                                for (k, v) in map {
                                    if k != "$ref" {
                                        merged.insert(
                                            k.clone(),
                                            self.resolve_schema_refs_inner(
                                                v,
                                                visited,
                                                cycle_found,
                                                unresolved_refs,
                                            ),
                                        );
                                    }
                                }
                                return Value::Object(merged);
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
                                self.resolve_schema_refs_inner(
                                    v,
                                    visited,
                                    cycle_found,
                                    unresolved_refs,
                                )
                            },
                        );
                    }
                    return Value::Object(new_map);
                }

                // Special handling for allOf arrays - merge $ref resolved schemas
                if let Some(Value::Array(all_of_array)) = map.get("allOf") {
                    let mut resolved_all_of = Vec::new();
                    let mut merged_properties = serde_json::Map::new();
                    let mut merged_required: Vec<String> = Vec::new();

                    for item in all_of_array {
                        let resolved_item = self.resolve_schema_refs_inner(
                            item,
                            visited,
                            cycle_found,
                            unresolved_refs,
                        );

                        match resolved_item {
                            Value::Object(ref item_map) => {
                                // If this item still has a $ref, keep it in allOf
                                if item_map.contains_key("$ref") {
                                    resolved_all_of.push(resolved_item);
                                } else {
                                    // Merge properties and required fields from resolved items
                                    if let Some(Value::Object(props_map)) =
                                        item_map.get("properties")
                                    {
                                        for (k, v) in props_map {
                                            merged_properties.insert(k.clone(), v.clone());
                                        }
                                    }
                                    if let Some(Value::Array(req_array)) = item_map.get("required")
                                    {
                                        for v in req_array {
                                            if let Value::String(s) = v
                                                && !merged_required.contains(s)
                                            {
                                                merged_required.push(s.to_owned());
                                            }
                                        }
                                    }
                                }
                            }
                            _ => resolved_all_of.push(resolved_item),
                        }
                    }

                    // If we have merged properties, create a single schema instead of allOf
                    if !merged_properties.is_empty() {
                        let mut merged_schema = serde_json::Map::new();

                        // Copy all properties except allOf
                        for (k, v) in map {
                            if k != "allOf" {
                                merged_schema.insert(k.clone(), v.clone());
                            }
                        }

                        // Add merged properties and required fields
                        merged_schema
                            .insert("properties".to_owned(), Value::Object(merged_properties));
                        if !merged_required.is_empty() {
                            merged_schema.insert(
                                "required".to_owned(),
                                Value::Array(
                                    merged_required.into_iter().map(Value::String).collect(),
                                ),
                            );
                        }

                        return Value::Object(merged_schema);
                    }
                }

                // Recursively process all properties
                let mut new_map = serde_json::Map::new();
                for (k, v) in map {
                    new_map.insert(
                        k.clone(),
                        self.resolve_schema_refs_inner(v, visited, cycle_found, unresolved_refs),
                    );
                }
                Value::Object(new_map)
            }
            Value::Array(arr) => Value::Array(
                arr.iter()
                    .map(|v| {
                        self.resolve_schema_refs_inner(v, visited, cycle_found, unresolved_refs)
                    })
                    .collect(),
            ),
            _ => schema.clone(),
        }
    }

    fn remove_x_gts_ref_fields(schema: &Value) -> Value {
        // Recursively remove x-gts-ref fields from a schema.
        // This is needed because the jsonschema crate doesn't understand x-gts-ref
        // and will fail on JSON Pointer references like "/$id".
        //
        // Additionally, when x-gts-ref removal leaves combinator branches (oneOf/
        // anyOf/allOf) as empty objects `{}`, those combinator keywords themselves
        // must be removed. Otherwise the jsonschema crate treats the empty branches
        // as match-everything schemas, causing e.g. oneOf to reject valid instances
        // because "more than one branch matched".
        match schema {
            Value::Object(map) => {
                let mut new_map = serde_json::Map::new();
                for (key, value) in map {
                    if key == "x-gts-ref" {
                        continue;
                    }
                    // For combinator keywords, check if all branches become
                    // empty objects after stripping; if so, drop the keyword.
                    if (key == "oneOf" || key == "anyOf" || key == "allOf")
                        && Self::is_all_empty_after_strip(value)
                    {
                        continue;
                    }
                    new_map.insert(key.clone(), Self::remove_x_gts_ref_fields(value));
                }
                Value::Object(new_map)
            }
            Value::Array(arr) => {
                Value::Array(arr.iter().map(Self::remove_x_gts_ref_fields).collect())
            }
            _ => schema.clone(),
        }
    }

    /// Returns true if `value` is an array where every element becomes an empty
    /// object after recursively stripping `x-gts-ref`.
    fn is_all_empty_after_strip(value: &Value) -> bool {
        if let Some(arr) = value.as_array() {
            arr.iter().all(|item| {
                let stripped = Self::remove_x_gts_ref_fields(item);
                stripped.as_object().is_some_and(serde_json::Map::is_empty)
            })
        } else {
            false
        }
    }

    fn validate_schema_x_gts_refs(&mut self, gts_id: &str) -> Result<(), StoreError> {
        if !gts_id.ends_with('~') {
            return Err(StoreError::SchemaNotFound(format!(
                "ID '{gts_id}' is not a schema (must end with '~')"
            )));
        }

        let schema_entity = self
            .get(gts_id)
            .ok_or_else(|| StoreError::SchemaNotFound(gts_id.to_owned()))?;

        if !schema_entity.is_schema {
            return Err(StoreError::SchemaNotFound(format!(
                "Entity '{gts_id}' is not a schema"
            )));
        }

        tracing::info!("Validating schema x-gts-ref fields for {}", gts_id);

        // Validate x-gts-ref constraints in the schema
        let validator = crate::x_gts_ref::XGtsRefValidator::new();
        let x_gts_ref_errors = validator.validate_schema(&schema_entity.content, "", None);

        if !x_gts_ref_errors.is_empty() {
            let error_messages: Vec<String> = x_gts_ref_errors
                .iter()
                .map(|err| {
                    if err.field_path.is_empty() {
                        err.reason.clone()
                    } else {
                        format!("{}: {}", err.field_path, err.reason)
                    }
                })
                .collect();
            let error_message =
                format!("x-gts-ref validation failed: {}", error_messages.join("; "));
            return Err(StoreError::ValidationError(error_message));
        }

        Ok(())
    }

    /// Validates all `$ref` values in a schema.
    ///
    /// Rules:
    /// - Local refs (starting with `#`) are always valid
    /// - External refs must use `gts://` URI format
    /// - The GTS ID after `gts://` must be a valid GTS identifier
    ///
    /// # Errors
    /// Returns `StoreError::InvalidRef` if any `$ref` is invalid.
    fn validate_schema_refs(schema: &Value, path: &str) -> Result<(), StoreError> {
        match schema {
            Value::Object(map) => {
                // Check $ref if present
                if let Some(Value::String(ref_uri)) = map.get("$ref") {
                    let current_path = if path.is_empty() {
                        "$ref".to_owned()
                    } else {
                        format!("{path}.$ref")
                    };

                    // Local refs (JSON Pointer) are always valid
                    if ref_uri.starts_with('#') {
                        // Valid local ref
                    }
                    // GTS refs must use gts:// URI format and target a schema
                    // (type) document. Only `entity.is_schema` documents are
                    // registered for retrieval (see `GtsRetriever::new`), so an
                    // instance-id ref would pass a plain `is_valid` check here
                    // and then fail later during retrieval. Require a type id up
                    // front via `GtsTypeId::try_new` (valid GTS id ending in `~`).
                    else if let Some(gts_id) = ref_uri.strip_prefix(GTS_URI_PREFIX) {
                        if crate::GtsTypeId::try_new(gts_id).is_err() {
                            return Err(StoreError::InvalidRef(format!(
                                "at '{current_path}': '{ref_uri}' must reference a GTS type id \
                                 (a valid identifier ending with '~'), got '{gts_id}'"
                            )));
                        }
                    }
                    // Any other external ref is invalid
                    else {
                        return Err(StoreError::InvalidRef(format!(
                            "at '{current_path}': '{ref_uri}' must be a local ref (starting with '#') \
                             or a GTS URI (starting with 'gts://')"
                        )));
                    }
                }

                // Recursively validate nested objects
                for (key, value) in map {
                    if key == "$ref" {
                        continue; // Already validated above
                    }
                    let nested_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    Self::validate_schema_refs(value, &nested_path)?;
                }
            }
            Value::Array(arr) => {
                for (idx, item) in arr.iter().enumerate() {
                    let nested_path = format!("{path}[{idx}]");
                    Self::validate_schema_refs(item, &nested_path)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Validates a schema against JSON Schema meta-schema and x-gts-ref constraints.
    ///
    /// # Errors
    /// Returns `StoreError` if validation fails.
    pub fn validate_schema(&mut self, gts_id: &str) -> Result<(), StoreError> {
        if !gts_id.ends_with('~') {
            return Err(StoreError::SchemaNotFound(format!(
                "ID '{gts_id}' is not a schema (must end with '~')"
            )));
        }

        let schema_entity = self
            .get(gts_id)
            .ok_or_else(|| StoreError::SchemaNotFound(gts_id.to_owned()))?;

        if !schema_entity.is_schema {
            return Err(StoreError::SchemaNotFound(format!(
                "Entity '{gts_id}' is not a schema"
            )));
        }

        let schema_content = schema_entity.content.clone();
        if !schema_content.is_object() {
            return Err(StoreError::SchemaNotFound(format!(
                "Schema '{gts_id}' content must be a dictionary"
            )));
        }

        tracing::info!("Validating schema {}", gts_id);

        // 1. Validate $ref fields - must be local (#...) or gts:// URIs
        Self::validate_schema_refs(&schema_content, "")?;

        // 2. Validate x-gts-ref fields (before JSON Schema validation)
        // This ensures we catch invalid GTS IDs in x-gts-ref before the JSON Schema
        // compiler potentially fails on them
        self.validate_schema_x_gts_refs(gts_id)?;

        // 3. Validate against JSON Schema meta-schema
        // We need to remove x-gts-ref fields before compiling because the jsonschema
        // crate doesn't understand them and will fail on JSON Pointer references
        let mut schema_for_validation = Self::remove_x_gts_ref_fields(&schema_content);

        // Check if schema contains gts:// references
        let has_gts_refs = schema_for_validation.to_string().contains("gts://");

        if has_gts_refs {
            // Skip jsonschema compilation for schemas with gts:// references during registration
            // This allows forward references (schemas referencing other schemas that don't exist yet)
            // Full validation with reference resolution will happen during instance validation
            tracing::debug!(
                "Schema {} contains gts:// references, skipping compilation during registration",
                gts_id
            );
        } else {
            // For schemas without gts:// references, validate the structure
            // Remove $id and $schema to avoid URL resolution issues
            if let Value::Object(ref mut map) = schema_for_validation {
                map.remove("$id");
                map.remove("$schema");
            }

            jsonschema::validator_for(&schema_for_validation).map_err(|e| {
                StoreError::ValidationError(format!(
                    "JSON Schema validation failed for '{gts_id}': {e}"
                ))
            })?;
        }

        tracing::info!(
            "Schema {} passed JSON Schema meta-schema validation",
            gts_id
        );

        Ok(())
    }

    /// Validates a chained schema ID by checking each derived schema against its base.
    ///
    /// For a chained ID like `gts.A~B~C~`, validates:
    /// - B (derived from A) is compatible with A
    /// - C (derived from A~B) is compatible with A~B
    ///
    /// The heavy lifting is delegated to [`crate::schema_compat`].
    ///
    /// # Errors
    /// Returns `StoreError::ValidationError` if any derived schema loosens base constraints.
    pub(crate) fn validate_schema_chain(&mut self, gts_id: &str) -> Result<(), StoreError> {
        let gid = GtsId::try_new(gts_id)
            .map_err(|e| StoreError::ValidationError(format!("Invalid GTS ID: {e}")))?;

        // Single-segment schemas have no parent to validate against
        if gid.segments().len() < 2 {
            return Ok(());
        }

        // Build pairs of (base_id, derived_id) for each adjacent level
        // Note: segment.segment already includes the trailing '~' for type segments
        let segments = &gid.segments();
        for i in 0..segments.len() - 1 {
            let base_id = format!(
                "gts.{}",
                segments[..=i]
                    .iter()
                    .map(gts_id::GtsIdSegment::raw)
                    .collect::<Vec<_>>()
                    .join("")
            );
            let derived_id = format!(
                "gts.{}",
                segments[..=i + 1]
                    .iter()
                    .map(gts_id::GtsIdSegment::raw)
                    .collect::<Vec<_>>()
                    .join("")
            );

            // Check x-gts-final: if the base type is final, derivation is not allowed.
            if let Some(base_entity) = self.get(&base_id)
                && base_entity
                    .content
                    .get(crate::schema_modifiers::X_GTS_FINAL)
                    == Some(&Value::Bool(true))
            {
                return Err(StoreError::ValidationError(format!(
                    "base type '{base_id}' is final and cannot be extended"
                )));
            }

            tracing::info!(
                "OP#12: Validating schema chain pair: base={} derived={}",
                base_id,
                derived_id
            );

            // Get and resolve both schemas
            let base_content = self.get_schema_content(&base_id).map_err(|_| {
                StoreError::ValidationError(format!(
                    "Base schema '{base_id}' not found for chain validation"
                ))
            })?;
            let derived_content = self.get_schema_content(&derived_id).map_err(|_| {
                StoreError::ValidationError(format!(
                    "Derived schema '{derived_id}' not found for chain validation"
                ))
            })?;

            let base_resolved = self
                .resolve_schema_refs_checked(&base_content)
                .map_err(|e| StoreError::ValidationError(format!("Schema '{base_id}' has {e}")))?;
            let derived_resolved =
                self.resolve_schema_refs_checked(&derived_content)
                    .map_err(|e| {
                        StoreError::ValidationError(format!("Schema '{derived_id}' has {e}"))
                    })?;

            // Extract effective schemas and compare via schema_compat module
            let base_eff = crate::schema_compat::extract_effective_schema(&base_resolved);
            let derived_eff = crate::schema_compat::extract_effective_schema(&derived_resolved);

            let errors = crate::schema_compat::validate_schema_compatibility(
                &base_eff,
                &derived_eff,
                &base_id,
                &derived_id,
            );

            if !errors.is_empty() {
                return Err(StoreError::ValidationError(format!(
                    "Schema '{}' is not compatible with base '{}': {}",
                    derived_id,
                    base_id,
                    errors.join("; ")
                )));
            }
        }

        Ok(())
    }

    /// OP#13: Validates schema traits across the inheritance chain.
    ///
    /// Walks the chain from base to leaf, collects `x-gts-traits-schema` and
    /// `x-gts-traits` from each level's **raw** content (before allOf
    /// flattening which would drop `x-gts-*` keys), resolves `$ref` inside
    /// collected trait schemas, then validates.
    ///
    /// Abstract types short-circuit before the chain is even walked — they are
    /// templates, not deployable entities, so trait completeness is not enforced
    /// (descendants close required traits).
    ///
    /// # Errors
    /// Returns `StoreError::ValidationError` if trait validation fails.
    pub(crate) fn validate_schema_traits(&mut self, gts_id: &str) -> Result<(), StoreError> {
        // Abstract types are templates — trait completeness is not enforced
        // (descendants close required traits).
        if self
            .get(gts_id)
            .is_some_and(|e| Self::content_is_abstract(&e.content))
        {
            return Ok(());
        }
        self.effective_traits(gts_id)?
            .validate(true)
            .map_err(|errors| Self::wrap_trait_error(gts_id, &errors))
    }

    /// `true` when a schema document declares `x-gts-abstract: true`.
    pub(crate) fn content_is_abstract(content: &Value) -> bool {
        content.get(crate::schema_modifiers::X_GTS_ABSTRACT) == Some(&Value::Bool(true))
    }

    /// Wrap trait-validation error messages in a `StoreError` tagged with the
    /// offending type id — the single home for this phrasing.
    fn wrap_trait_error(gts_id: &str, errors: &[String]) -> StoreError {
        StoreError::ValidationError(format!(
            "Schema '{gts_id}' trait validation failed: {}",
            errors.join("; ")
        ))
    }

    /// Build the [`EffectiveTraits`](crate::schema_traits::EffectiveTraits) for
    /// `type_id` by walking its `$id` chain (root → leaf).
    ///
    /// Collects `x-gts-traits-schema` subschemas and `x-gts-traits` values from
    /// each level's **raw** content (before `resolve_schema_refs` flattens
    /// `allOf` and drops the `x-gts-*` extension keys), inlines JSON Pointer
    /// `$ref`s against their host document, resolves any `gts://` `$ref`s inside
    /// the collected subschemas, RFC 7396-merges the values (descendant
    /// last-wins for scalars/arrays, recursive merge for objects, `null` deletes
    /// the key), then composes the effective trait-schema and materializes the
    /// values. The leaf's `$schema` dialect is re-injected into the composed
    /// schema. Shared by [`Self::validate_schema_traits`] and
    /// [`Self::validate_and_resolve_type_schema`].
    ///
    /// # Errors
    /// `StoreError::ValidationError` if the id is invalid, an ancestor schema is
    /// missing, or a `$ref` inside a trait schema fails to resolve.
    pub(crate) fn effective_traits(
        &mut self,
        type_id: &str,
    ) -> Result<crate::schema_traits::EffectiveTraits, StoreError> {
        let gid = GtsId::try_new(type_id)
            .map_err(|e| StoreError::ValidationError(format!("Invalid GTS ID: {e}")))?;
        let segments = &gid.segments();

        let mut trait_schemas: Vec<Value> = Vec::new();
        let mut merged_traits = serde_json::Map::new();

        for i in 0..segments.len() {
            let schema_id = format!(
                "gts.{}",
                segments[..=i]
                    .iter()
                    .map(gts_id::GtsIdSegment::raw)
                    .collect::<Vec<_>>()
                    .join("")
            );

            let content = self.get_schema_content(&schema_id).map_err(|_| {
                StoreError::ValidationError(format!(
                    "Schema '{schema_id}' not found for trait validation"
                ))
            })?;

            // Collect this level's trait schemas, then inline any JSON Pointer
            // (`#/...`) `$ref`s against this host document (`content`) while it
            // is still the document root — see `inline_local_pointers`.
            let mut level_trait_schemas: Vec<Value> = Vec::new();
            crate::schema_traits::collect_trait_schema_from_value(
                &content,
                &mut level_trait_schemas,
            );
            for ts in level_trait_schemas {
                trait_schemas.push(crate::schema_traits::inline_local_pointers(&ts, &content));
            }

            let mut level_traits = serde_json::Map::new();
            crate::schema_traits::collect_traits_from_value(&content, &mut level_traits);
            crate::schema_traits::merge_rfc7396_into(&mut merged_traits, &level_traits);
        }

        let mut resolved_trait_schemas: Vec<Value> = Vec::with_capacity(trait_schemas.len());
        for ts in &trait_schemas {
            let resolved = self.resolve_schema_refs_checked(ts).map_err(|e| {
                StoreError::ValidationError(format!("Schema '{type_id}' trait schema has {e}"))
            })?;
            resolved_trait_schemas.push(resolved);
        }

        // Dialect comes from the leaf document's `$schema`, re-injected into the
        // composed trait schema because the inline fragment had its root-only
        // `$schema` stripped when embedded.
        let dialect = self
            .get(type_id)
            .and_then(|leaf| leaf.content.get("$schema").and_then(Value::as_str))
            .map(str::to_owned);

        Ok(crate::schema_traits::build_effective_traits(
            &resolved_trait_schemas,
            &Value::Object(merged_traits),
            dialect.as_deref(),
        ))
    }

    /// Validate a type and return its fully-resolved [`ResolvedTypeSchema`] in a
    /// single call. Every type it depends on (its `$id`-chain ancestors and
    /// the targets of its `gts://` `$ref`s) must already be registered.
    ///
    /// Runs `validate_schema` (meta-schema + `x-gts-ref`) and
    /// `validate_schema_chain` (OP#12), then a single resolve pass: the body is
    /// inlined and the effective traits schema/values are built **exactly once**.
    /// When the leaf is not abstract, the built artifacts validate themselves
    /// (OP#13) before being returned — no rebuild. Abstract types skip the
    /// completeness check (descendants close required traits) but still produce
    /// artifacts.
    ///
    /// Uncached: a consumer that calls this repeatedly for the same `type_id`
    /// should cache the result (safe forever — versioned ids are immutable).
    ///
    /// # Errors
    /// `StoreError::ValidationError` if any validation stage fails or a
    /// dependency is missing from the store; `StoreError::SchemaNotFound` if the
    /// type is not registered.
    pub fn validate_and_resolve_type_schema(
        &mut self,
        type_id: &str,
    ) -> Result<ResolvedTypeSchema, StoreError> {
        self.validate_schema(type_id)?;
        self.validate_schema_chain(type_id)?;

        let content = self.get_schema_content(type_id)?;
        let resolved_schema = self
            .resolve_schema_refs_checked(&content)
            .map_err(|e| StoreError::ValidationError(format!("Schema '{type_id}' has {e}")))?;

        // Abstract types still produce artifacts but skip the completeness check.
        let is_abstract = Self::content_is_abstract(&content);
        let traits = self.effective_traits(type_id)?;

        if !is_abstract {
            traits
                .validate(true)
                .map_err(|errors| Self::wrap_trait_error(type_id, &errors))?;
        }

        Ok(ResolvedTypeSchema {
            resolved_schema,
            effective_traits: traits.values,
            effective_traits_schema: traits.schema,
            is_abstract,
        })
    }

    /// Validate a caller-supplied instance payload against `type_id`'s schema.
    ///
    /// Stateless: no registered instance is required, but the type and its
    /// `$ref`/chain dependencies must be registered. Rejects abstract types
    /// (OP#6) and enforces `x-gts-ref`.
    ///
    /// # Errors
    /// `StoreError::ValidationError` on schema-compile failure, JSON Schema
    /// validation failure, abstract type, or `x-gts-ref` violation;
    /// `StoreError::SchemaNotFound` if the type is not registered.
    pub fn validate_payload(&mut self, type_id: &str, payload: &Value) -> Result<(), StoreError> {
        let content = self.get_schema_content(type_id)?;

        // Abstract types cannot have direct instances (OP#6).
        if Self::content_is_abstract(&content) {
            return Err(StoreError::ValidationError(format!(
                "type '{type_id}' is abstract and cannot have direct instances"
            )));
        }

        // Payload validation needs only the resolved type body — traits are
        // schema-level metadata (§9.7) and never appear in instances, so the
        // effective-traits build is deliberately skipped here.
        let resolved_schema = self
            .resolve_schema_refs_checked(&content)
            .map_err(|e| StoreError::ValidationError(format!("Schema '{type_id}' has {e}")))?;

        // Strip x-gts-ref before compiling (unknown keyword to jsonschema); keep
        // a retriever for any residual gts:// refs, mirroring validate_instance.
        let schema_for_validation = Self::remove_x_gts_ref_fields(&resolved_schema);
        let retriever = GtsRetriever::new(&self.by_id);
        let validator = jsonschema::options()
            .with_retriever(retriever)
            .build(&schema_for_validation)
            .map_err(|e| {
                StoreError::ValidationError(format!("Invalid schema for '{type_id}': {e}"))
            })?;

        let errors: Vec<String> = validator
            .iter_errors(payload)
            .map(|e| e.to_string())
            .collect();
        if !errors.is_empty() {
            return Err(StoreError::ValidationError(format!(
                "Validation failed: {}",
                errors.join(", ")
            )));
        }

        let xref = crate::x_gts_ref::XGtsRefValidator::new();
        let xref_errors = xref.validate_instance(payload, &resolved_schema, "");
        if !xref_errors.is_empty() {
            let msgs: Vec<String> = xref_errors
                .iter()
                .map(|e| {
                    if e.field_path.is_empty() {
                        e.reason.clone()
                    } else {
                        format!("{}: {}", e.field_path, e.reason)
                    }
                })
                .collect();
            return Err(StoreError::ValidationError(format!(
                "x-gts-ref validation failed: {}",
                msgs.join("; ")
            )));
        }

        Ok(())
    }

    /// Validates an instance against its schema.
    ///
    /// # Errors
    /// Returns `StoreError` if validation fails.
    pub fn validate_instance(&mut self, instance_id: &str) -> Result<(), StoreError> {
        // Try to parse as GTS ID first (for well-known instances)
        // If that fails, use the instance_id directly (for anonymous instances with UUIDs)
        let lookup_id = if let Ok(gid) = GtsId::try_new(instance_id) {
            gid.id().to_owned()
        } else {
            instance_id.to_owned()
        };

        let obj = self
            .get(&lookup_id)
            .ok_or_else(|| StoreError::ObjectNotFound(instance_id.to_owned()))?
            .clone();

        let type_id = obj
            .type_id
            .as_ref()
            .ok_or_else(|| StoreError::SchemaForInstanceNotFound(lookup_id.clone()))?
            .clone();

        let schema = self.get_schema_content(&type_id)?;

        // Check x-gts-abstract: abstract types cannot have direct instances.
        if Self::content_is_abstract(&schema) {
            return Err(StoreError::ValidationError(format!(
                "type '{type_id}' is abstract and cannot have direct instances"
            )));
        }

        tracing::info!(
            "Validating instance {} against schema {}",
            instance_id,
            type_id
        );

        // Resolve references before JSON Schema compilation. Missing external
        // refs are fatal here; otherwise validation could silently ignore part
        // of the intended schema.
        let schema_with_internal_refs_resolved = self
            .resolve_schema_refs_checked(&schema)
            .map_err(|e| StoreError::ValidationError(format!("Schema '{type_id}' has {e}")))?;

        // Remove x-gts-ref fields before jsonschema validation.
        // x-gts-ref is a GTS extension unknown to the jsonschema crate; leaving it
        // inside oneOf/anyOf/allOf branches would cause those branches to be treated
        // as empty match-everything schemas, breaking combinator semantics.
        let schema_with_internal_refs_resolved =
            Self::remove_x_gts_ref_fields(&schema_with_internal_refs_resolved);

        tracing::debug!(
            "Schema for validation: {}",
            serde_json::to_string_pretty(&schema_with_internal_refs_resolved).unwrap_or_default()
        );

        // Create custom retriever for gts:// URI resolution
        let retriever = GtsRetriever::new(&self.by_id);

        // Build validator with custom retriever to handle gts:// references
        // Internal #/ references have already been resolved by resolve_schema_refs
        // The retriever will resolve any $ref to gts:// URIs automatically
        let validator = jsonschema::options()
            .with_retriever(retriever)
            .build(&schema_with_internal_refs_resolved)
            .map_err(|e| {
                tracing::error!("Schema compilation error: {}", e);
                StoreError::ValidationError(format!(
                    "Invalid schema: {e}\nContent: {}\nSchema: {}",
                    serde_json::to_string_pretty(&obj.content).unwrap_or_default(),
                    serde_json::to_string_pretty(&schema_with_internal_refs_resolved)
                        .unwrap_or_default()
                ))
            })?;

        validator.validate(&obj.content).map_err(|_| {
            let errors: Vec<String> = validator
                .iter_errors(&obj.content)
                .map(|err| err.to_string())
                .collect();
            StoreError::ValidationError(format!("Validation failed: {}", errors.join(", ")))
        })?;

        // Validate x-gts-ref constraints
        let validator = crate::x_gts_ref::XGtsRefValidator::new();
        let x_gts_ref_errors = validator.validate_instance(&obj.content, &schema, "");

        if !x_gts_ref_errors.is_empty() {
            let error_messages: Vec<String> = x_gts_ref_errors
                .iter()
                .map(|err| {
                    if err.field_path.is_empty() {
                        err.reason.clone()
                    } else {
                        format!("{}: {}", err.field_path, err.reason)
                    }
                })
                .collect();
            let error_message =
                format!("x-gts-ref validation failed: {}", error_messages.join("; "));
            return Err(StoreError::ValidationError(error_message));
        }

        Ok(())
    }

    /// Casts an entity from one schema to another.
    ///
    /// # Errors
    /// Returns `StoreError` if the cast fails.
    pub fn cast(
        &mut self,
        from_id: &str,
        target_type_id: &str,
    ) -> Result<GtsEntityCastResult, StoreError> {
        let from_entity = self
            .get(from_id)
            .ok_or_else(|| StoreError::EntityNotFound(from_id.to_owned()))?
            .clone();

        if from_entity.is_schema {
            return Err(StoreError::CastFromSchemaNotAllowed(from_id.to_owned()));
        }

        let to_schema = self
            .get(target_type_id)
            .ok_or_else(|| StoreError::ObjectNotFound(target_type_id.to_owned()))?
            .clone();

        // Get the source schema
        let (from_schema, _from_type_id) = if from_entity.is_schema {
            let id = from_entity
                .gts_id
                .as_ref()
                .ok_or(StoreError::InvalidEntity)?
                .id()
                .to_owned();
            (from_entity.clone(), id)
        } else {
            let type_id = from_entity
                .type_id
                .as_ref()
                .ok_or_else(|| StoreError::SchemaForInstanceNotFound(from_id.to_owned()))?;
            let schema = self
                .get(type_id)
                .ok_or_else(|| StoreError::ObjectNotFound(type_id.clone()))?
                .clone();
            (schema, type_id.clone())
        };

        // Create a resolver to handle $ref in schemas
        // TODO: Implement custom resolver
        let resolver = None;

        from_entity
            .cast(&to_schema, &from_schema, resolver)
            .map_err(|e| StoreError::SchemaNotFound(e.to_string()))
    }

    pub fn is_minor_compatible(
        &mut self,
        old_type_id: &str,
        new_type_id: &str,
    ) -> GtsEntityCastResult {
        let old_entity = self.get(old_type_id).cloned();
        let new_entity = self.get(new_type_id).cloned();

        let (Some(old_ent), Some(new_ent)) = (old_entity, new_entity) else {
            return GtsEntityCastResult {
                from_id: old_type_id.to_owned(),
                to_id: new_type_id.to_owned(),
                old: old_type_id.to_owned(),
                new: new_type_id.to_owned(),
                direction: "unknown".to_owned(),
                added_properties: Vec::new(),
                removed_properties: Vec::new(),
                changed_properties: Vec::new(),
                is_fully_compatible: false,
                is_backward_compatible: false,
                is_forward_compatible: false,
                incompatibility_reasons: vec!["Schema not found".to_owned()],
                backward_errors: vec!["Schema not found".to_owned()],
                forward_errors: vec!["Schema not found".to_owned()],
                casted_entity: None,
                error: None,
            };
        };

        let old_schema = &old_ent.content;
        let new_schema = &new_ent.content;

        // Use the cast method's compatibility checking logic
        let (is_backward, backward_errors) =
            GtsEntityCastResult::check_backward_compatibility(old_schema, new_schema);
        let (is_forward, forward_errors) =
            GtsEntityCastResult::check_forward_compatibility(old_schema, new_schema);

        // Determine direction
        let direction = GtsEntityCastResult::infer_direction(old_type_id, new_type_id);

        GtsEntityCastResult {
            from_id: old_type_id.to_owned(),
            to_id: new_type_id.to_owned(),
            old: old_type_id.to_owned(),
            new: new_type_id.to_owned(),
            direction,
            added_properties: Vec::new(),
            removed_properties: Vec::new(),
            changed_properties: Vec::new(),
            is_fully_compatible: is_backward && is_forward,
            is_backward_compatible: is_backward,
            is_forward_compatible: is_forward,
            incompatibility_reasons: Vec::new(),
            backward_errors,
            forward_errors,
            casted_entity: None,
            error: None,
        }
    }

    pub fn build_schema_graph(&mut self, gts_id: &str) -> Value {
        let mut seen_gts_ids = std::collections::HashSet::new();
        self.gts2node(gts_id, &mut seen_gts_ids)
    }

    fn gts2node(
        &mut self,
        gts_id: &str,
        seen_gts_ids: &mut std::collections::HashSet<String>,
    ) -> Value {
        let mut ret = serde_json::Map::new();
        ret.insert("id".to_owned(), Value::String(gts_id.to_owned()));

        if seen_gts_ids.contains(gts_id) {
            return Value::Object(ret);
        }

        seen_gts_ids.insert(gts_id.to_owned());

        // Clone the entity to avoid borrowing issues
        let entity_clone = self.get(gts_id).cloned();

        if let Some(entity) = entity_clone {
            let mut refs = serde_json::Map::new();

            // Collect ref IDs first to avoid borrow issues
            let ref_ids: Vec<_> = entity
                .gts_refs
                .iter()
                .filter(|r| {
                    r.id != gts_id
                        && !r.id.starts_with("http://json-schema.org")
                        && !r.id.starts_with("https://json-schema.org")
                })
                .map(|r| (r.source_path.clone(), r.id.clone()))
                .collect();

            for (source_path, ref_id) in ref_ids {
                refs.insert(source_path, self.gts2node(&ref_id, seen_gts_ids));
            }

            if !refs.is_empty() {
                ret.insert("refs".to_owned(), Value::Object(refs));
            }

            if let Some(ref type_id) = entity.type_id {
                if !type_id.starts_with("http://json-schema.org")
                    && !type_id.starts_with("https://json-schema.org")
                {
                    let type_id_clone = type_id.clone();
                    ret.insert(
                        "type_id".to_owned(),
                        self.gts2node(&type_id_clone, seen_gts_ids),
                    );
                }
            } else {
                let mut errors = ret
                    .get("errors")
                    .and_then(|e| e.as_array())
                    .cloned()
                    .unwrap_or_default();
                errors.push(Value::String("Schema not recognized".to_owned()));
                ret.insert("errors".to_owned(), Value::Array(errors));
            }
        } else {
            let mut errors = ret
                .get("errors")
                .and_then(|e| e.as_array())
                .cloned()
                .unwrap_or_default();
            errors.push(Value::String("Entity not found".to_owned()));
            ret.insert("errors".to_owned(), Value::Array(errors));
        }

        Value::Object(ret)
    }

    #[must_use]
    pub fn query(&self, expr: &str, limit: usize) -> GtsStoreQueryResult {
        let mut result = GtsStoreQueryResult {
            error: String::new(),
            count: 0,
            limit,
            results: Vec::new(),
        };

        // Parse the query expression
        let (base, _, filt) = expr.partition('[');
        let base_pattern = base.trim();
        let is_wildcard = base_pattern.contains('*');

        // Parse filters if present
        let filter_str = if filt.is_empty() {
            ""
        } else {
            filt.rsplit_once(']').map_or("", |x| x.0)
        };
        let filters = Self::parse_query_filters(filter_str);

        // Validate and create pattern
        let (wildcard_pattern, exact_gts_id, error) =
            Self::validate_query_pattern(base_pattern, is_wildcard);
        if !error.is_empty() {
            result.error = error;
            return result;
        }

        // Filter entities
        for entity in self.by_id.values() {
            if result.results.len() >= limit {
                break;
            }

            if !entity.content.is_object() {
                continue;
            }

            let Some(ref gts_id) = entity.gts_id else {
                continue;
            };

            // Check if ID matches the pattern
            if !Self::matches_id_pattern(
                gts_id,
                base_pattern,
                is_wildcard,
                wildcard_pattern.as_ref(),
                exact_gts_id.as_ref(),
            ) {
                continue;
            }

            // Check filters
            if !Self::matches_filters(&entity.content, &filters) {
                continue;
            }

            result.results.push(entity.content.clone());
        }

        result.count = result.results.len();
        result
    }

    fn parse_query_filters(filter_str: &str) -> HashMap<String, String> {
        let mut filters = HashMap::new();
        if filter_str.is_empty() {
            return filters;
        }

        let parts: Vec<&str> = filter_str.split(',').map(str::trim).collect();
        for part in parts {
            if let Some((k, v)) = part.split_once('=') {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                filters.insert(k.trim().to_owned(), v.to_owned());
            }
        }

        filters
    }

    fn validate_query_pattern(
        base_pattern: &str,
        is_wildcard: bool,
    ) -> (Option<GtsIdPattern>, Option<GtsId>, String) {
        if is_wildcard {
            if !base_pattern.ends_with(".*") && !base_pattern.ends_with("~*") {
                return (
                    None,
                    None,
                    "Invalid query: wildcard patterns must end with .* or ~*".to_owned(),
                );
            }
            match GtsIdPattern::try_new(base_pattern) {
                Ok(pattern) => (Some(pattern), None, String::new()),
                Err(e) => (None, None, format!("Invalid query: {e}")),
            }
        } else {
            match GtsId::try_new(base_pattern) {
                Ok(gts_id) => {
                    if gts_id.segments().is_empty() {
                        (
                            None,
                            None,
                            "Invalid query: GTS ID has no valid segments".to_owned(),
                        )
                    } else {
                        (None, Some(gts_id), String::new())
                    }
                }
                Err(e) => (None, None, format!("Invalid query: {e}")),
            }
        }
    }

    fn matches_id_pattern(
        entity_id: &GtsId,
        base_pattern: &str,
        is_wildcard: bool,
        wildcard_pattern: Option<&GtsIdPattern>,
        exact_gts_id: Option<&GtsId>,
    ) -> bool {
        if is_wildcard && let Some(pattern) = wildcard_pattern {
            return entity_id.matches_pattern(pattern);
        }

        // For non-wildcard patterns, use matches_pattern to support version flexibility
        if let Some(_exact) = exact_gts_id {
            match GtsIdPattern::try_new(base_pattern) {
                Ok(pattern_as_wildcard) => entity_id.matches_pattern(&pattern_as_wildcard),
                Err(_) => entity_id.id() == base_pattern,
            }
        } else {
            entity_id.id() == base_pattern
        }
    }

    fn matches_filters(entity_content: &Value, filters: &HashMap<String, String>) -> bool {
        if filters.is_empty() {
            return true;
        }

        if let Some(obj) = entity_content.as_object() {
            for (key, value) in filters {
                let entity_value = obj.get(key).map_or_else(String::new, ToString::to_string);

                // Support wildcard in filter values
                if value == "*" {
                    if entity_value.is_empty() || entity_value == "null" {
                        return false;
                    }
                } else if entity_value != format!("\"{value}\"") && entity_value != *value {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}

// Helper trait for string partitioning
trait StringPartition {
    fn partition(&self, delimiter: char) -> (&str, &str, &str);
}

impl StringPartition for str {
    fn partition(&self, delimiter: char) -> (&str, &str, &str) {
        if let Some(pos) = self.find(delimiter) {
            let (before, after_with_delim) = self.split_at(pos);
            let after = &after_with_delim[delimiter.len_utf8()..];
            (before, &after_with_delim[..delimiter.len_utf8()], after)
        } else {
            (self, "", "")
        }
    }
}
#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;
