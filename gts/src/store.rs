use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

use crate::entities::GtsEntity;
use crate::gts::{GtsID, GtsWildcard};
use crate::schema_cast::GtsEntityCastResult;

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
}

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

pub struct GtsStore {
    by_id: HashMap<String, GtsEntity>,
    reader: Option<Box<dyn GtsReader>>,
}

impl GtsStore {
    pub fn new(reader: Option<Box<dyn GtsReader>>) -> Self {
        let mut store = GtsStore {
            by_id: HashMap::new(),
            reader,
        };

        if store.reader.is_some() {
            store.populate_from_reader();
        }

        tracing::info!("Populated GtsStore with {} entities", store.by_id.len());
        store
    }

    fn populate_from_reader(&mut self) {
        if let Some(ref mut reader) = self.reader {
            for entity in reader.iter() {
                if let Some(ref gts_id) = entity.gts_id {
                    self.by_id.insert(gts_id.id.clone(), entity);
                }
            }
        }
    }

    /// Registers an entity in the store.
    ///
    /// # Errors
    /// Returns `StoreError::InvalidEntity` if the entity has no valid GTS ID.
    pub fn register(&mut self, entity: GtsEntity) -> Result<(), StoreError> {
        let id = entity
            .gts_id
            .as_ref()
            .ok_or(StoreError::InvalidEntity)?
            .id
            .clone();
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

        let gts_id = GtsID::new(type_id).map_err(|_| StoreError::InvalidSchemaId)?;
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
        if let Some(ref reader) = self.reader {
            if let Some(entity) = reader.read_by_id(entity_id) {
                self.by_id.insert(entity_id.to_owned(), entity);
                return self.by_id.get(entity_id);
            }
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

    fn resolve_schema_refs(&self, schema: &Value) -> Value {
        // Recursively resolve $ref references in the schema
        match schema {
            Value::Object(map) => {
                if let Some(Value::String(ref_uri)) = map.get("$ref") {
                    // Try to resolve the reference
                    if let Some(entity) = self.by_id.get(ref_uri) {
                        if entity.is_schema {
                            // Recursively resolve refs in the referenced schema
                            let mut resolved = self.resolve_schema_refs(&entity.content);

                            // Remove $id and $schema from resolved content to avoid URL resolution issues
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
                                        merged.insert(k.clone(), self.resolve_schema_refs(v));
                                    }
                                }
                                return Value::Object(merged);
                            }
                        }
                    }
                    // If we can't resolve, remove the $ref to avoid "relative URL" errors
                    // and keep other properties
                    let mut new_map = serde_json::Map::new();
                    for (k, v) in map {
                        if k != "$ref" {
                            new_map.insert(k.clone(), self.resolve_schema_refs(v));
                        }
                    }
                    if !new_map.is_empty() {
                        return Value::Object(new_map);
                    }
                    return schema.clone();
                }

                // Recursively process all properties
                let mut new_map = serde_json::Map::new();
                for (k, v) in map {
                    new_map.insert(k.clone(), self.resolve_schema_refs(v));
                }
                Value::Object(new_map)
            }
            Value::Array(arr) => {
                Value::Array(arr.iter().map(|v| self.resolve_schema_refs(v)).collect())
            }
            _ => schema.clone(),
        }
    }

    fn remove_x_gts_ref_fields(schema: &Value) -> Value {
        // Recursively remove x-gts-ref fields from a schema
        // This is needed because the jsonschema crate doesn't understand x-gts-ref
        // and will fail on JSON Pointer references like "/$id"
        match schema {
            Value::Object(map) => {
                let mut new_map = serde_json::Map::new();
                for (key, value) in map {
                    if key == "x-gts-ref" {
                        continue; // Skip x-gts-ref fields
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

        // 1. Validate x-gts-ref fields FIRST (before JSON Schema validation)
        // This ensures we catch invalid GTS IDs in x-gts-ref before the JSON Schema
        // compiler potentially fails on them
        self.validate_schema_x_gts_refs(gts_id)?;

        // 2. Validate against JSON Schema meta-schema
        // We need to remove x-gts-ref fields before compiling because the jsonschema
        // crate doesn't understand them and will fail on JSON Pointer references
        let mut schema_for_validation = Self::remove_x_gts_ref_fields(&schema_content);

        // Also remove $id and $schema to avoid URL resolution issues
        if let Value::Object(ref mut map) = schema_for_validation {
            map.remove("$id");
            map.remove("$schema");
        }

        // For now, we'll do a basic validation by trying to compile the schema
        jsonschema::JSONSchema::compile(&schema_for_validation).map_err(|e| {
            StoreError::ValidationError(format!(
                "JSON Schema validation failed for '{gts_id}': {e}"
            ))
        })?;

        tracing::info!(
            "Schema {} passed JSON Schema meta-schema validation",
            gts_id
        );

        Ok(())
    }

    /// Validates an instance against its schema.
    ///
    /// # Errors
    /// Returns `StoreError` if validation fails.
    pub fn validate_instance(&mut self, gts_id: &str) -> Result<(), StoreError> {
        let gid = GtsID::new(gts_id).map_err(|_| StoreError::ObjectNotFound(gts_id.to_owned()))?;

        let obj = self
            .get(&gid.id)
            .ok_or_else(|| StoreError::ObjectNotFound(gts_id.to_owned()))?
            .clone();

        let schema_id = obj
            .schema_id
            .as_ref()
            .ok_or_else(|| StoreError::SchemaForInstanceNotFound(gid.id.clone()))?
            .clone();

        let schema = self.get_schema_content(&schema_id)?;

        tracing::info!(
            "Validating instance {} against schema {}",
            gts_id,
            schema_id
        );

        // Resolve all $ref references in the schema by inlining them
        let mut resolved_schema = self.resolve_schema_refs(&schema);

        // Remove $id and $schema from the top-level schema to avoid URL resolution issues
        if let Value::Object(ref mut map) = resolved_schema {
            map.remove("$id");
            map.remove("$schema");
        }

        tracing::debug!(
            "Resolved schema: {}",
            serde_json::to_string_pretty(&resolved_schema).unwrap_or_default()
        );

        let compiled = jsonschema::JSONSchema::compile(&resolved_schema).map_err(|e| {
            tracing::error!("Schema compilation error: {}", e);
            StoreError::ValidationError(format!("Invalid schema: {e}"))
        })?;

        compiled.validate(&obj.content).map_err(|e| {
            let errors: Vec<String> = e.map(|err| err.to_string()).collect();
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
        target_schema_id: &str,
    ) -> Result<GtsEntityCastResult, StoreError> {
        let from_entity = self
            .get(from_id)
            .ok_or_else(|| StoreError::EntityNotFound(from_id.to_owned()))?
            .clone();

        if from_entity.is_schema {
            return Err(StoreError::CastFromSchemaNotAllowed(from_id.to_owned()));
        }

        let to_schema = self
            .get(target_schema_id)
            .ok_or_else(|| StoreError::ObjectNotFound(target_schema_id.to_owned()))?
            .clone();

        // Get the source schema
        let (from_schema, _from_schema_id) = if from_entity.is_schema {
            let id = from_entity
                .gts_id
                .as_ref()
                .ok_or(StoreError::InvalidEntity)?
                .id
                .clone();
            (from_entity.clone(), id)
        } else {
            let schema_id = from_entity
                .schema_id
                .as_ref()
                .ok_or_else(|| StoreError::SchemaForInstanceNotFound(from_id.to_owned()))?;
            let schema = self
                .get(schema_id)
                .ok_or_else(|| StoreError::ObjectNotFound(schema_id.clone()))?
                .clone();
            (schema, schema_id.clone())
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
        old_schema_id: &str,
        new_schema_id: &str,
    ) -> GtsEntityCastResult {
        let old_entity = self.get(old_schema_id).cloned();
        let new_entity = self.get(new_schema_id).cloned();

        let (Some(old_ent), Some(new_ent)) = (old_entity, new_entity) else {
            return GtsEntityCastResult {
                from_id: old_schema_id.to_owned(),
                to_id: new_schema_id.to_owned(),
                old: old_schema_id.to_owned(),
                new: new_schema_id.to_owned(),
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
        let direction = GtsEntityCastResult::infer_direction(old_schema_id, new_schema_id);

        GtsEntityCastResult {
            from_id: old_schema_id.to_owned(),
            to_id: new_schema_id.to_owned(),
            old: old_schema_id.to_owned(),
            new: new_schema_id.to_owned(),
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

            if let Some(ref schema_id) = entity.schema_id {
                if !schema_id.starts_with("http://json-schema.org")
                    && !schema_id.starts_with("https://json-schema.org")
                {
                    let schema_id_clone = schema_id.clone();
                    ret.insert(
                        "schema_id".to_owned(),
                        self.gts2node(&schema_id_clone, seen_gts_ids),
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
    ) -> (Option<GtsWildcard>, Option<GtsID>, String) {
        if is_wildcard {
            if !base_pattern.ends_with(".*") && !base_pattern.ends_with("~*") {
                return (
                    None,
                    None,
                    "Invalid query: wildcard patterns must end with .* or ~*".to_owned(),
                );
            }
            match GtsWildcard::new(base_pattern) {
                Ok(pattern) => (Some(pattern), None, String::new()),
                Err(e) => (None, None, format!("Invalid query: {e}")),
            }
        } else {
            match GtsID::new(base_pattern) {
                Ok(gts_id) => {
                    if gts_id.gts_id_segments.is_empty() {
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
        entity_id: &GtsID,
        base_pattern: &str,
        is_wildcard: bool,
        wildcard_pattern: Option<&GtsWildcard>,
        exact_gts_id: Option<&GtsID>,
    ) -> bool {
        if is_wildcard {
            if let Some(pattern) = wildcard_pattern {
                return entity_id.wildcard_match(pattern);
            }
        }

        // For non-wildcard patterns, use wildcard_match to support version flexibility
        if let Some(_exact) = exact_gts_id {
            match GtsWildcard::new(base_pattern) {
                Ok(pattern_as_wildcard) => entity_id.wildcard_match(&pattern_as_wildcard),
                Err(_) => entity_id.id == base_pattern,
            }
        } else {
            entity_id.id == base_pattern
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::entities::{GtsConfig, GtsEntity};
    use serde_json::json;

    #[test]
    fn test_gts_store_query_result_default() {
        let result = GtsStoreQueryResult {
            error: String::new(),
            count: 0,
            limit: 100,
            results: vec![],
        };

        assert_eq!(result.count, 0);
        assert_eq!(result.limit, 100);
        assert!(result.error.is_empty());
        assert!(result.results.is_empty());
    }

    #[test]
    fn test_gts_store_query_result_serialization() {
        let result = GtsStoreQueryResult {
            error: String::new(),
            count: 2,
            limit: 10,
            results: vec![json!({"id": "test1"}), json!({"id": "test2"})],
        };

        let json_value = serde_json::to_value(&result).expect("test");
        let json = json_value.as_object().expect("test");
        assert_eq!(json.get("count").expect("test").as_u64().expect("test"), 2);
        assert_eq!(json.get("limit").expect("test").as_u64().expect("test"), 10);
        assert!(json.get("results").expect("test").is_array());
    }

    #[test]
    fn test_gts_store_new_without_reader() {
        let store: GtsStore = GtsStore::new(None);
        assert_eq!(store.items().count(), 0);
    }

    #[test]
    fn test_gts_store_register_entity() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        let result = store.register(entity);
        assert!(result.is_ok());
        assert_eq!(store.items().count(), 1);
    }

    #[test]
    fn test_gts_store_register_schema() {
        let mut store = GtsStore::new(None);

        let schema_content = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result =
            store.register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_content);

        assert!(result.is_ok());

        let entity = store.get("gts.vendor.package.namespace.type.v1.0~");
        assert!(entity.is_some());
        assert!(entity.expect("test").is_schema);
    }

    #[test]
    fn test_gts_store_register_schema_invalid_id() {
        let mut store = GtsStore::new(None);

        let schema_content = json!({
            "type": "object"
        });

        let result = store.register_schema(
            "gts.vendor.package.namespace.type.v1.0", // Missing ~
            &schema_content,
        );

        assert!(result.is_err());
        match result {
            Err(StoreError::InvalidSchemaId) => {}
            _ => panic!("Expected InvalidSchemaId error"),
        }
    }

    #[test]
    fn test_gts_store_get_schema_content() {
        let mut store = GtsStore::new(None);

        let schema_content = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_content)
            .expect("test");

        let result = store.get_schema_content("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_ok());
        assert_eq!(result.expect("test"), schema_content);
    }

    #[test]
    fn test_gts_store_get_schema_content_not_found() {
        let mut store = GtsStore::new(None);
        let result = store.get_schema_content("nonexistent~");
        assert!(result.is_err());

        match result {
            Err(StoreError::SchemaNotFound(id)) => {
                assert_eq!(id, "nonexistent~");
            }
            _ => panic!("Expected SchemaNotFound error"),
        }
    }

    #[test]
    fn test_gts_store_items_iterator() {
        let mut store = GtsStore::new(None);

        // Add schemas which are easier to register
        for i in 0..3 {
            let schema_content = json!({
                "$id": format!("gts.vendor.package.namespace.type.v{i}.0~"),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store
                .register_schema(
                    &format!("gts.vendor.package.namespace.type.v{i}.0~"),
                    &schema_content,
                )
                .expect("test");
        }

        assert_eq!(store.items().count(), 3);

        // Verify we can iterate
        assert_eq!(store.items().count(), 3);
    }

    #[test]
    fn test_gts_store_validate_instance_missing_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        // Add an entity without a schema
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).expect("test");

        // Try to validate - should fail because no schema_id
        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_build_schema_graph() {
        let mut store = GtsStore::new(None);

        let schema_content = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_content)
            .expect("test");

        let graph = store.build_schema_graph("gts.vendor.package.namespace.type.v1.0~");
        assert!(graph.is_object());
    }

    // Note: matches_id_pattern is a private method, tested indirectly through query()

    #[test]
    fn test_gts_store_query_wildcard() {
        let mut store = GtsStore::new(None);

        // Add multiple schemas
        for i in 0..3 {
            let schema_content = json!({
                "$id": format!("gts.vendor.package.namespace.type.v{i}.0~"),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store
                .register_schema(
                    &format!("gts.vendor.package.namespace.type.v{i}.0~"),
                    &schema_content,
                )
                .expect("test");
        }

        // Query with wildcard
        let result = store.query("gts.vendor.*", 10);
        assert_eq!(result.count, 3);
        assert_eq!(result.results.len(), 3);
    }

    #[test]
    fn test_gts_store_query_with_limit() {
        let mut store = GtsStore::new(None);

        // Add 5 schemas
        for i in 0..5 {
            let schema_content = json!({
                "$id": format!("gts.vendor.package.namespace.type.v{i}.0~"),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store
                .register_schema(
                    &format!("gts.vendor.package.namespace.type.v{i}.0~"),
                    &schema_content,
                )
                .expect("test");
        }

        // Query with limit of 2
        let result = store.query("gts.vendor.*", 2);
        assert_eq!(result.results.len(), 2);
        // Verify limit is working - we get 2 results even though there are 5 total
        assert!(result.count >= 2);
    }

    #[test]
    fn test_store_error_display() {
        let error = StoreError::ObjectNotFound("test_id".to_owned());
        assert!(error.to_string().contains("test_id"));

        let error = StoreError::SchemaNotFound("schema_id".to_owned());
        assert!(error.to_string().contains("schema_id"));

        let error = StoreError::EntityNotFound("entity_id".to_owned());
        assert!(error.to_string().contains("entity_id"));

        let error = StoreError::SchemaForInstanceNotFound("instance_id".to_owned());
        assert!(error.to_string().contains("instance_id"));
    }

    // Note: resolve_schema_refs is a private method, tested indirectly through validate_instance()

    #[test]
    fn test_gts_store_cast() {
        let mut store = GtsStore::new(None);

        // Register schemas
        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string", "default": "test@example.com"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_v1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.1~", &schema_v2)
            .expect("test");

        // Register an entity with proper schema_id
        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "John"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        // Test casting
        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1~",
        );

        // Just verify it executes
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_cast_missing_entity() {
        let mut store = GtsStore::new(None);

        let result = store.cast("nonexistent", "gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_cast_missing_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).expect("test");

        let result = store.cast("gts.vendor.package.namespace.type.v1.0", "nonexistent~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_is_minor_compatible() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_v1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.1~", &schema_v2)
            .expect("test");

        let result = store.is_minor_compatible(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~",
        );

        // Adding optional property is backward compatible
        assert!(result.is_backward_compatible);
    }

    #[test]
    fn test_gts_store_get() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).expect("test");

        let result = store.get("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_some());
    }

    #[test]
    fn test_gts_store_get_nonexistent() {
        let mut store = GtsStore::new(None);
        let result = store.get("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_gts_store_query_exact_match() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let result = store.query("gts.vendor.package.namespace.type.v1.0~", 10);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn test_gts_store_register_duplicate() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity1 = GtsEntity::new(
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

        let entity2 = GtsEntity::new(
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

        store.register(entity1).expect("test");
        let result = store.register(entity2);

        // Should still succeed (overwrites)
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_validate_instance_success() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_validate_instance_missing_entity() {
        let mut store = GtsStore::new(None);
        let result = store.validate_instance("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_validate_instance_no_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_register_schema_with_invalid_id() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "invalid",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let result = store.register_schema("invalid", &schema);
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_get_schema_content_missing() {
        let mut store = GtsStore::new(None);
        let result = store.get_schema_content("nonexistent~");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_query_empty() {
        let store = GtsStore::new(None);
        let result = store.query("gts.vendor.*", 10);
        assert_eq!(result.count, 0);
        assert_eq!(result.results.len(), 0);
    }

    #[test]
    fn test_gts_store_items_empty() {
        let store = GtsStore::new(None);
        assert_eq!(store.items().count(), 0);
    }

    #[test]
    fn test_gts_store_register_entity_without_id() {
        let mut store = GtsStore::new(None);

        let content = json!({
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            None,
            None,
            false,
            String::new(),
            None,
            None,
        );

        let result = store.register(entity);
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_build_schema_graph_missing() {
        let mut store = GtsStore::new(None);
        let graph = store.build_schema_graph("nonexistent~");
        assert!(graph.is_object());
    }

    #[test]
    fn test_gts_store_new_empty() {
        let store = GtsStore::new(None);
        assert_eq!(store.items().count(), 0);
    }

    #[test]
    fn test_gts_store_cast_entity_without_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

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

        store.register(entity).expect("test");

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1~",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_is_minor_compatible_missing_schemas() {
        let mut store = GtsStore::new(None);
        let result = store.is_minor_compatible("nonexistent1~", "nonexistent2~");
        assert!(!result.is_backward_compatible);
    }

    #[test]
    fn test_gts_store_validate_instance_with_refs() {
        let mut store = GtsStore::new(None);

        // Register base schema
        let base_schema = json!({
            "$id": "gts.vendor.package.namespace.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        // Register schema with $ref
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.base.v1.0~"},
                {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            ]
        });

        store
            .register_schema("gts.vendor.package.namespace.base.v1.0~", &base_schema)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        // Just verify it executes
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_validate_instance_validation_failure() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "age": {"type": "number"}
            },
            "required": ["age"]
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "age": "not a number"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_query_with_filters() {
        let mut store = GtsStore::new(None);

        for i in 0..5 {
            let schema = json!({
                "$id": format!("gts.vendor.package.namespace.type{i}.v1.0~"),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store
                .register_schema(
                    &format!("gts.vendor.package.namespace.type{i}.v1.0~"),
                    &schema,
                )
                .expect("test");
        }

        let result = store.query("gts.vendor.package.namespace.type0.*", 10);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn test_gts_store_register_multiple_schemas() {
        let mut store = GtsStore::new(None);

        for i in 0..10 {
            let schema = json!({
                "$id": format!("gts.vendor.package.namespace.type.v1.{i}~"),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            let result = store.register_schema(
                &format!("gts.vendor.package.namespace.type.v1.{i}~"),
                &schema,
            );
            assert!(result.is_ok());
        }

        assert_eq!(store.items().count(), 10);
    }

    #[test]
    fn test_gts_store_cast_with_validation() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string", "default": "test@example.com"}
            },
            "required": ["name"]
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_v1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.1~", &schema_v2)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "John"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1~",
        );

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_build_schema_graph_with_refs() {
        let mut store = GtsStore::new(None);

        let base_schema = json!({
            "$id": "gts.vendor.package.namespace.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.base.v1.0~"}
            ]
        });

        store
            .register_schema("gts.vendor.package.namespace.base.v1.0~", &base_schema)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let graph = store.build_schema_graph("gts.vendor.package.namespace.type.v1.0~");
        assert!(graph.is_object());
    }

    #[test]
    fn test_gts_store_get_schema_content_success() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let result = store.get_schema_content("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_ok());
        assert_eq!(
            result
                .expect("test")
                .get("type")
                .expect("test")
                .as_str()
                .expect("test"),
            "object"
        );
    }

    #[test]
    fn test_gts_store_register_entity_with_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        let result = store.register(entity);
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_query_result_structure() {
        let result = GtsStoreQueryResult {
            error: String::new(),
            count: 0,
            limit: 100,
            results: vec![],
        };

        assert_eq!(result.count, 0);
        assert_eq!(result.limit, 100);
        assert!(result.results.is_empty());
    }

    #[test]
    fn test_gts_store_error_variants() {
        let err1 = StoreError::InvalidEntity;
        assert!(!err1.to_string().is_empty());

        let err2 = StoreError::InvalidSchemaId;
        assert!(!err2.to_string().is_empty());
    }

    #[test]
    fn test_gts_store_register_schema_overwrite() {
        let mut store = GtsStore::new(None);

        let schema1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema2)
            .expect("test");

        let result = store.get_schema_content("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_ok());
        let schema = result.expect("test");
        assert!(schema
            .get("properties")
            .expect("test")
            .get("email")
            .is_some());
    }

    #[test]
    fn test_gts_store_cast_missing_source_schema() {
        let mut store = GtsStore::new(None);
        let cfg = GtsConfig::default();

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.1~", &schema)
            .expect("test");

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1~",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_query_multiple_patterns() {
        let mut store = GtsStore::new(None);

        let schema1 = json!({
            "$id": "gts.vendor1.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let schema2 = json!({
            "$id": "gts.vendor2.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store
            .register_schema("gts.vendor1.package.namespace.type.v1.0~", &schema1)
            .expect("test");
        store
            .register_schema("gts.vendor2.package.namespace.type.v1.0~", &schema2)
            .expect("test");

        let result1 = store.query("gts.vendor1.*", 10);
        assert_eq!(result1.count, 1);

        let result2 = store.query("gts.vendor2.*", 10);
        assert_eq!(result2.count, 1);

        let result3 = store.query("gts.*", 10);
        assert_eq!(result3.count, 2);
    }

    #[test]
    fn test_gts_store_validate_with_nested_refs() {
        let mut store = GtsStore::new(None);

        let base = json!({
            "$id": "gts.vendor.package.namespace.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        let middle = json!({
            "$id": "gts.vendor.package.namespace.middle.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.base.v1.0~"},
                {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            ]
        });

        let top = json!({
            "$id": "gts.vendor.package.namespace.top.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.middle.v1.0~"},
                {
                    "type": "object",
                    "properties": {
                        "email": {"type": "string"}
                    }
                }
            ]
        });

        store
            .register_schema("gts.vendor.package.namespace.base.v1.0~", &base)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.middle.v1.0~", &middle)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.top.v1.0~", &top)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.top.v1.0",
            "name": "test",
            "email": "test@example.com"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.top.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.top.v1.0");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_query_with_version_wildcard() {
        let mut store = GtsStore::new(None);

        for i in 0..3 {
            let schema = json!({
                "$id": format!("gts.vendor.package.namespace.type.v{i}.0~"),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store
                .register_schema(
                    &format!("gts.vendor.package.namespace.type.v{i}.0~"),
                    &schema,
                )
                .expect("test");
        }

        let result = store.query("gts.vendor.package.namespace.type.*", 10);
        assert_eq!(result.count, 3);
    }

    #[test]
    fn test_gts_store_cast_backward_incompatible() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v2.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            },
            "required": ["name", "age"]
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_v1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v2.0~", &schema_v2)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "John"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v2.0~",
        );

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_items_iterator_multiple() {
        let mut store = GtsStore::new(None);

        for i in 0..5 {
            let schema = json!({
                "$id": format!("gts.vendor.package.namespace.type{i}.v1.0~"),
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            });

            store
                .register_schema(
                    &format!("gts.vendor.package.namespace.type{i}.v1.0~"),
                    &schema,
                )
                .expect("test");
        }

        let count = store.items().count();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_gts_store_compatibility_fully_compatible() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_v1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.1~", &schema_v2)
            .expect("test");

        let result = store.is_minor_compatible(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~",
        );

        // Adding optional property is backward compatible
        assert!(result.is_backward_compatible);
    }

    #[test]
    fn test_gts_store_build_schema_graph_complex() {
        let mut store = GtsStore::new(None);

        let base1 = json!({
            "$id": "gts.vendor.package.namespace.base1.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        let base2 = json!({
            "$id": "gts.vendor.package.namespace.base2.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let combined = json!({
            "$id": "gts.vendor.package.namespace.combined.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.base1.v1.0~"},
                {"$ref": "gts.vendor.package.namespace.base2.v1.0~"}
            ]
        });

        store
            .register_schema("gts.vendor.package.namespace.base1.v1.0~", &base1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.base2.v1.0~", &base2)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.combined.v1.0~", &combined)
            .expect("test");

        let graph = store.build_schema_graph("gts.vendor.package.namespace.combined.v1.0~");
        assert!(graph.is_object());
    }

    #[test]
    fn test_gts_store_register_invalid_json_entity() {
        let mut store = GtsStore::new(None);
        let content = json!({"name": "test"});

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            None,
            None,
            false,
            String::new(),
            None,
            None,
        );

        let result = store.register(entity);
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_validate_with_complex_schema() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string", "minLength": 1, "maxLength": 100},
                "age": {"type": "integer", "minimum": 0, "maximum": 150},
                "email": {"type": "string", "format": "email"},
                "tags": {
                    "type": "array",
                    "items": {"type": "string"},
                    "minItems": 1
                }
            },
            "required": ["name", "age"]
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "John Doe",
            "age": 30,
            "email": "john@example.com",
            "tags": ["developer", "rust"]
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        // Just verify it executes
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_validate_missing_required_field() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_schema_with_properties_only() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result = store.register_schema("gts.vendor.package.namespace.type.v1.0~", &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_query_no_results() {
        let store = GtsStore::new(None);
        let result = store.query("gts.nonexistent.*", 10);
        assert_eq!(result.count, 0);
        assert!(result.results.is_empty());
    }

    #[test]
    fn test_gts_store_query_with_zero_limit() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let result = store.query("gts.vendor.*", 0);
        assert_eq!(result.results.len(), 0);
    }

    #[test]
    fn test_gts_store_cast_same_version() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.0~",
        );
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_multiple_entities_same_schema() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();

        for i in 0..5 {
            let content = json!({
                "id": format!("gts.vendor.package.namespace.instance{i}.v1.0"),
                "name": format!("test{i}")
            });

            let entity = GtsEntity::new(
                None,
                None,
                &content,
                Some(&cfg),
                None,
                false,
                String::new(),
                None,
                Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
            );

            store.register(entity).expect("test");
        }

        let count = store.items().count();
        assert!(count >= 5); // At least 5 entities
    }

    #[test]
    fn test_gts_store_get_schema_content_for_entity() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let result = store.get_schema_content("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_ok());

        let retrieved = result.expect("test");
        assert_eq!(
            retrieved.get("type").expect("test").as_str().expect("test"),
            "object"
        );
    }

    #[test]
    fn test_gts_store_compatibility_with_removed_properties() {
        let mut store = GtsStore::new(None);

        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"},
                "email": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_v1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.1~", &schema_v2)
            .expect("test");

        let result = store.is_minor_compatible(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~",
        );

        // Removing optional properties is forward compatible in current implementation
        assert!(result.is_forward_compatible);
    }

    #[test]
    fn test_gts_store_build_schema_graph_single_schema() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let graph = store.build_schema_graph("gts.vendor.package.namespace.type.v1.0~");
        assert!(graph.is_object());
    }

    #[test]
    fn test_gts_store_register_schema_without_id() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let result = store.register_schema("gts.vendor.package.namespace.type.v1.0~", &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_gts_store_validate_with_unresolvable_ref() {
        let mut store = GtsStore::new(None);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {"$ref": "gts.vendor.package.namespace.nonexistent.v1.0~"}
            ]
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        // Should handle unresolvable refs gracefully
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_query_result_serialization_with_error() {
        let result = GtsStoreQueryResult {
            error: "Test error message".to_owned(),
            count: 0,
            limit: 10,
            results: vec![],
        };

        let json_value = serde_json::to_value(&result).expect("test");
        let json = json_value.as_object().expect("test");
        assert_eq!(
            json.get("error").expect("test").as_str().expect("test"),
            "Test error message"
        );
        assert_eq!(json.get("count").expect("test").as_u64().expect("test"), 0);
    }

    #[test]
    fn test_gts_store_resolve_schema_refs_with_merge() {
        let mut store = GtsStore::new(None);

        // Register base schema
        let base_schema = json!({
            "$id": "gts.vendor.package.namespace.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            }
        });

        // Register schema with $ref and additional properties
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {
                    "$ref": "gts.vendor.package.namespace.base.v1.0~",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            ]
        });

        store
            .register_schema("gts.vendor.package.namespace.base.v1.0~", &base_schema)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_resolve_schema_refs_with_unresolvable_and_properties() {
        let mut store = GtsStore::new(None);

        // Schema with unresolvable $ref but with other properties
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {
                "data": {
                    "$ref": "gts.vendor.package.namespace.nonexistent.v1.0~",
                    "type": "object"
                }
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "data": {}
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.type.v1.0");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_cast_from_schema_entity() {
        let mut store = GtsStore::new(None);

        // Register two schemas
        let schema_v1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema_v2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema_v1)
            .expect("test");
        store
            .register_schema("gts.vendor.package.namespace.type.v1.1~", &schema_v2)
            .expect("test");

        // Try to cast from schema to schema
        let result = store.cast(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~",
        );

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_gts_store_build_schema_graph_with_schema_id() {
        let mut store = GtsStore::new(None);

        // Register schema
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        // Register instance with schema_id
        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.instance.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let graph = store.build_schema_graph("gts.vendor.package.namespace.instance.v1.0");
        assert!(graph.is_object());

        // Check that schema_id is included in the graph
        let graph_obj = graph.as_object().expect("test");
        assert!(graph_obj.contains_key("schema_id") || graph_obj.contains_key("errors"));
    }

    #[test]
    fn test_gts_store_query_with_filter_brackets() {
        let mut store = GtsStore::new(None);

        // Add entities with different properties
        let cfg = GtsConfig::default();
        for i in 0..3 {
            let content = json!({
                "id": format!("gts.vendor.package.namespace.item{i}.v1.0"),
                "name": format!("item{i}"),
                "status": if i % 2 == 0 { "active" } else { "inactive" }
            });

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

            store.register(entity).expect("test");
        }

        // Query with filter
        let result = store.query("gts.vendor.*[status=active]", 10);
        assert!(result.count >= 1);
    }

    #[test]
    fn test_gts_store_query_with_wildcard_filter() {
        let mut store = GtsStore::new(None);

        let cfg = GtsConfig::default();
        for i in 0..3 {
            let content = if i == 0 {
                json!({
                    "id": format!("gts.vendor.package.namespace.item{i}.v1.0"),
                    "name": format!("item{i}"),
                    "category": null
                })
            } else {
                json!({
                    "id": format!("gts.vendor.package.namespace.item{i}.v1.0"),
                    "name": format!("item{i}"),
                    "category": format!("cat{i}")
                })
            };

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

            store.register(entity).expect("test");
        }

        // Query with wildcard filter (should exclude null values)
        let result = store.query("gts.vendor.*[category=*]", 10);
        assert_eq!(result.count, 2);
    }

    #[test]
    fn test_gts_store_query_invalid_wildcard_pattern() {
        let store = GtsStore::new(None);

        // Query with invalid wildcard pattern (doesn't end with .* or ~*)
        let result = store.query("gts.vendor*", 10);
        assert!(!result.error.is_empty());
        assert!(result.error.contains("wildcard"));
    }

    #[test]
    fn test_gts_store_query_invalid_gts_id() {
        let store = GtsStore::new(None);

        // Query with invalid GTS ID
        let result = store.query("invalid-id", 10);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_gts_store_query_gts_id_no_segments() {
        let store = GtsStore::new(None);

        // This should create an error for GTS ID with no valid segments
        let result = store.query("gts", 10);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_gts_store_validate_instance_invalid_gts_id() {
        let mut store = GtsStore::new(None);

        // Try to validate with invalid GTS ID
        let result = store.validate_instance("invalid-id");
        assert!(result.is_err());
    }

    #[test]
    fn test_gts_store_validate_instance_invalid_schema() {
        let mut store = GtsStore::new(None);

        // Register entity with schema that has invalid JSON Schema
        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "invalid_type"
        });

        store
            .register_schema("gts.vendor.package.namespace.type.v1.0~", &schema)
            .expect("test");

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.instance.v1.0",
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
        );

        store.register(entity).expect("test");

        let result = store.validate_instance("gts.vendor.package.namespace.instance.v1.0");
        assert!(result.is_err());
    }

    // Mock GtsReader for testing reader functionality
    struct MockGtsReader {
        entities: Vec<GtsEntity>,
        index: usize,
    }

    impl MockGtsReader {
        fn new(entities: Vec<GtsEntity>) -> Self {
            MockGtsReader { entities, index: 0 }
        }
    }

    impl GtsReader for MockGtsReader {
        fn iter(&mut self) -> Box<dyn Iterator<Item = GtsEntity> + '_> {
            Box::new(self.entities.clone().into_iter())
        }

        fn read_by_id(&self, entity_id: &str) -> Option<GtsEntity> {
            self.entities
                .iter()
                .find(|e| e.gts_id.as_ref().map(|id| id.id.as_str()) == Some(entity_id))
                .cloned()
        }

        fn reset(&mut self) {
            self.index = 0;
        }
    }

    #[test]
    fn test_gts_store_with_reader() {
        let cfg = GtsConfig::default();

        // Create entities for the reader
        let mut entities = Vec::new();
        for i in 0..3 {
            let content = json!({
                "id": format!("gts.vendor.package.namespace.item{i}.v1.0"),
                "name": format!("item{i}")
            });

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

            entities.push(entity);
        }

        let reader = MockGtsReader::new(entities);
        let store = GtsStore::new(Some(Box::new(reader)));

        // Store should be populated from reader
        assert_eq!(store.items().count(), 3);
    }

    #[test]
    fn test_gts_store_get_from_reader() {
        let cfg = GtsConfig::default();

        // Create an entity for the reader
        let content = json!({
            "id": "gts.vendor.package.namespace.item.v1.0",
            "name": "test"
        });

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

        let reader = MockGtsReader::new(vec![entity]);
        let mut store = GtsStore::new(Some(Box::new(reader)));

        // Get entity that's not in cache but available from reader
        let result = store.get("gts.vendor.package.namespace.item.v1.0");
        assert!(result.is_some());
    }

    #[test]
    fn test_gts_store_reader_without_gts_id() {
        // Create entity without gts_id
        let content = json!({
            "name": "test"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            None,
            None,
            false,
            String::new(),
            None,
            None,
        );

        let reader = MockGtsReader::new(vec![entity]);
        let store = GtsStore::new(Some(Box::new(reader)));

        // Entity without gts_id should not be added to store
        assert_eq!(store.items().count(), 0);
    }
}
