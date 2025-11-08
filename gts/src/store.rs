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
    #[error("Entity must have a valid gts_id")]
    InvalidEntity,
    #[error("Schema type_id must end with '~'")]
    InvalidSchemaId,
}

pub trait GtsReader: Send {
    fn iter(&mut self) -> Box<dyn Iterator<Item = GtsEntity> + '_>;
    fn read_by_id(&self, entity_id: &str) -> Option<GtsEntity>;
    fn reset(&mut self);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsStoreQueryResult {
    pub error: String,
    pub count: usize,
    pub limit: usize,
    pub results: Vec<Value>,
}

impl GtsStoreQueryResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        if !self.error.is_empty() {
            map.insert("error".to_string(), Value::String(self.error.clone()));
        }
        map.insert("count".to_string(), Value::Number(self.count.into()));
        map.insert("limit".to_string(), Value::Number(self.limit.into()));
        map.insert("error".to_string(), Value::String(self.error.clone()));
        map.insert("results".to_string(), Value::Array(self.results.clone()));
        map
    }
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

    pub fn register(&mut self, entity: GtsEntity) -> Result<(), StoreError> {
        if entity.gts_id.is_none() {
            return Err(StoreError::InvalidEntity);
        }
        let id = entity.gts_id.as_ref().unwrap().id.clone();
        self.by_id.insert(id, entity);
        Ok(())
    }

    pub fn register_schema(&mut self, type_id: &str, schema: Value) -> Result<(), StoreError> {
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
        self.by_id.insert(type_id.to_string(), entity);
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
                self.by_id.insert(entity_id.to_string(), entity);
                return self.by_id.get(entity_id);
            }
        }

        None
    }

    pub fn get_schema_content(&mut self, type_id: &str) -> Result<Value, StoreError> {
        if let Some(entity) = self.get(type_id) {
            return Ok(entity.content.clone());
        }
        Err(StoreError::SchemaNotFound(type_id.to_string()))
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
                                let mut merged = resolved_map.clone();
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

    pub fn validate_instance(&mut self, gts_id: &str) -> Result<(), StoreError> {
        let gid = GtsID::new(gts_id).map_err(|_| StoreError::ObjectNotFound(gts_id.to_string()))?;

        let obj = self
            .get(&gid.id)
            .ok_or_else(|| StoreError::ObjectNotFound(gts_id.to_string()))?
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
            StoreError::SchemaNotFound(format!("Invalid schema: {}", e))
        })?;

        compiled.validate(&obj.content).map_err(|e| {
            let errors: Vec<String> = e.map(|err| err.to_string()).collect();
            StoreError::SchemaNotFound(format!("Validation failed: {}", errors.join(", ")))
        })?;

        Ok(())
    }

    pub fn cast(
        &mut self,
        from_id: &str,
        target_schema_id: &str,
    ) -> Result<GtsEntityCastResult, StoreError> {
        let from_entity = self
            .get(from_id)
            .ok_or_else(|| StoreError::EntityNotFound(from_id.to_string()))?
            .clone();

        let to_schema = self
            .get(target_schema_id)
            .ok_or_else(|| StoreError::ObjectNotFound(target_schema_id.to_string()))?
            .clone();

        // Get the source schema
        let (from_schema, _from_schema_id) = if from_entity.is_schema {
            (
                from_entity.clone(),
                from_entity.gts_id.as_ref().unwrap().id.clone(),
            )
        } else {
            let schema_id = from_entity
                .schema_id
                .as_ref()
                .ok_or_else(|| StoreError::SchemaForInstanceNotFound(from_id.to_string()))?;
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

        if old_entity.is_none() || new_entity.is_none() {
            return GtsEntityCastResult {
                from_id: old_schema_id.to_string(),
                to_id: new_schema_id.to_string(),
                old: old_schema_id.to_string(),
                new: new_schema_id.to_string(),
                direction: "unknown".to_string(),
                added_properties: Vec::new(),
                removed_properties: Vec::new(),
                changed_properties: Vec::new(),
                is_fully_compatible: false,
                is_backward_compatible: false,
                is_forward_compatible: false,
                incompatibility_reasons: vec!["Schema not found".to_string()],
                backward_errors: vec!["Schema not found".to_string()],
                forward_errors: vec!["Schema not found".to_string()],
                casted_entity: None,
                error: None,
            };
        }

        let old_schema = &old_entity.unwrap().content;
        let new_schema = &new_entity.unwrap().content;

        // Use the cast method's compatibility checking logic
        let (is_backward, backward_errors) =
            GtsEntityCastResult::check_backward_compatibility(old_schema, new_schema);
        let (is_forward, forward_errors) =
            GtsEntityCastResult::check_forward_compatibility(old_schema, new_schema);

        // Determine direction
        let direction = GtsEntityCastResult::infer_direction(old_schema_id, new_schema_id);

        GtsEntityCastResult {
            from_id: old_schema_id.to_string(),
            to_id: new_schema_id.to_string(),
            old: old_schema_id.to_string(),
            new: new_schema_id.to_string(),
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
        ret.insert("id".to_string(), Value::String(gts_id.to_string()));

        if seen_gts_ids.contains(gts_id) {
            return Value::Object(ret);
        }

        seen_gts_ids.insert(gts_id.to_string());

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
                ret.insert("refs".to_string(), Value::Object(refs));
            }

            if let Some(ref schema_id) = entity.schema_id {
                if !schema_id.starts_with("http://json-schema.org")
                    && !schema_id.starts_with("https://json-schema.org")
                {
                    let schema_id_clone = schema_id.clone();
                    ret.insert(
                        "schema_id".to_string(),
                        self.gts2node(&schema_id_clone, seen_gts_ids),
                    );
                }
            } else {
                let mut errors = ret
                    .get("errors")
                    .and_then(|e| e.as_array())
                    .cloned()
                    .unwrap_or_default();
                errors.push(Value::String("Schema not recognized".to_string()));
                ret.insert("errors".to_string(), Value::Array(errors));
            }
        } else {
            let mut errors = ret
                .get("errors")
                .and_then(|e| e.as_array())
                .cloned()
                .unwrap_or_default();
            errors.push(Value::String("Entity not found".to_string()));
            ret.insert("errors".to_string(), Value::Array(errors));
        }

        Value::Object(ret)
    }

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
        let filter_str = if !filt.is_empty() {
            filt.rsplitn(2, ']').nth(1).unwrap_or("")
        } else {
            ""
        };
        let filters = self.parse_query_filters(filter_str);

        // Validate and create pattern
        let (wildcard_pattern, exact_gts_id, error) =
            self.validate_query_pattern(base_pattern, is_wildcard);
        if !error.is_empty() {
            result.error = error;
            return result;
        }

        // Filter entities
        for entity in self.by_id.values() {
            if result.results.len() >= limit {
                break;
            }

            if !entity.content.is_object() || entity.gts_id.is_none() {
                continue;
            }

            // Check if ID matches the pattern
            if !self.matches_id_pattern(
                entity.gts_id.as_ref().unwrap(),
                base_pattern,
                is_wildcard,
                wildcard_pattern.as_ref(),
                exact_gts_id.as_ref(),
            ) {
                continue;
            }

            // Check filters
            if !self.matches_filters(&entity.content, &filters) {
                continue;
            }

            result.results.push(entity.content.clone());
        }

        result.count = result.results.len();
        result
    }

    fn parse_query_filters(&self, filter_str: &str) -> HashMap<String, String> {
        let mut filters = HashMap::new();
        if filter_str.is_empty() {
            return filters;
        }

        let parts: Vec<&str> = filter_str.split(',').map(|p| p.trim()).collect();
        for part in parts {
            if let Some((k, v)) = part.split_once('=') {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                filters.insert(k.trim().to_string(), v.to_string());
            }
        }

        filters
    }

    fn validate_query_pattern(
        &self,
        base_pattern: &str,
        is_wildcard: bool,
    ) -> (Option<GtsWildcard>, Option<GtsID>, String) {
        if is_wildcard {
            if !base_pattern.ends_with(".*") && !base_pattern.ends_with("~*") {
                return (
                    None,
                    None,
                    "Invalid query: wildcard patterns must end with .* or ~*".to_string(),
                );
            }
            match GtsWildcard::new(base_pattern) {
                Ok(pattern) => (Some(pattern), None, String::new()),
                Err(e) => (None, None, format!("Invalid query: {}", e)),
            }
        } else {
            match GtsID::new(base_pattern) {
                Ok(gts_id) => {
                    if gts_id.gts_id_segments.is_empty() {
                        (
                            None,
                            None,
                            "Invalid query: GTS ID has no valid segments".to_string(),
                        )
                    } else {
                        (None, Some(gts_id), String::new())
                    }
                }
                Err(e) => (None, None, format!("Invalid query: {}", e)),
            }
        }
    }

    fn matches_id_pattern(
        &self,
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

    fn matches_filters(&self, entity_content: &Value, filters: &HashMap<String, String>) -> bool {
        if filters.is_empty() {
            return true;
        }

        if let Some(obj) = entity_content.as_object() {
            for (key, value) in filters {
                let entity_value = obj
                    .get(key)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "".to_string());

                // Support wildcard in filter values
                if value == "*" {
                    if entity_value.is_empty() || entity_value == "null" {
                        return false;
                    }
                } else if entity_value != format!("\"{}\"", value) && entity_value != *value {
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
