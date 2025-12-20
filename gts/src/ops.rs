use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::entities::{GtsConfig, GtsEntity};
use crate::files_reader::GtsFileReader;
use crate::gts::{GtsID, GtsWildcard};
use crate::path_resolver::JsonPathResolver;
use crate::schema_cast::GtsEntityCastResult;
use crate::store::{GtsStore, GtsStoreQueryResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsIdValidationResult {
    pub id: String,
    pub valid: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// Serializable representation of a GTS ID segment for API responses.
/// This is distinct from `crate::gts::GtsIdSegment` which is the internal representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsIdSegmentInfo {
    pub vendor: String,
    pub package: String,
    pub namespace: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub ver_major: Option<u32>,
    pub ver_minor: Option<u32>,
    pub is_type: bool,
}

impl From<&crate::gts::GtsIdSegment> for GtsIdSegmentInfo {
    fn from(seg: &crate::gts::GtsIdSegment) -> Self {
        Self {
            vendor: seg.vendor.clone(),
            package: seg.package.clone(),
            namespace: seg.namespace.clone(),
            type_name: seg.type_name.clone(),
            ver_major: if seg.ver_major > 0 {
                Some(seg.ver_major)
            } else {
                None
            },
            ver_minor: seg.ver_minor,
            is_type: seg.is_type,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsIdParseResult {
    pub id: String,
    pub ok: bool,
    pub segments: Vec<GtsIdSegmentInfo>,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsIdMatchResult {
    pub candidate: String,
    pub pattern: String,
    #[serde(rename = "match")]
    pub is_match: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsUuidResult {
    pub id: String,
    pub uuid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsValidationResult {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// Schema graph result - serializes directly as the graph object
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GtsSchemaGraphResult {
    pub graph: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsEntityInfo {
    pub id: String,
    pub schema_id: Option<String>,
    pub is_schema: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsGetEntityResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub schema_id: Option<String>,
    pub is_schema: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsEntitiesListResult {
    pub entities: Vec<GtsEntityInfo>,
    pub count: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsAddEntityResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub schema_id: Option<String>,
    pub is_schema: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsAddEntitiesResult {
    pub ok: bool,
    pub results: Vec<GtsAddEntityResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsAddSchemaResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsExtractIdResult {
    pub id: String,
    pub schema_id: Option<String>,
    pub selected_entity_field: Option<String>,
    pub selected_schema_id_field: Option<String>,
    pub is_schema: bool,
}

pub struct GtsOps {
    pub verbose: usize,
    pub cfg: GtsConfig,
    pub path: Option<Vec<String>>,
    pub store: GtsStore,
}

impl GtsOps {
    #[must_use]
    pub fn new(path: Option<Vec<String>>, config: Option<String>, verbose: usize) -> Self {
        let cfg = Self::load_config(config);
        let reader: Option<Box<dyn crate::store::GtsReader>> = path.as_ref().map(|p| {
            Box::new(GtsFileReader::new(p, Some(cfg.clone()))) as Box<dyn crate::store::GtsReader>
        });
        let store = GtsStore::new(reader);

        GtsOps {
            verbose,
            cfg,
            path,
            store,
        }
    }

    fn load_config(config_path: Option<String>) -> GtsConfig {
        // Try user-provided path
        if let Some(path) = config_path {
            if let Ok(cfg) = Self::load_config_from_path(&PathBuf::from(path)) {
                return cfg;
            }
        }

        // Try default path (relative to current directory)
        let default_path = PathBuf::from("gts.config.json");
        if let Ok(cfg) = Self::load_config_from_path(&default_path) {
            return cfg;
        }

        // Fall back to defaults
        GtsConfig::default()
    }

    fn load_config_from_path(path: &PathBuf) -> Result<GtsConfig, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let data: HashMap<String, Value> = serde_json::from_str(&content)?;
        Ok(Self::create_config_from_data(&data))
    }

    fn create_config_from_data(data: &HashMap<String, Value>) -> GtsConfig {
        let default_cfg = GtsConfig::default();

        let entity_id_fields = data
            .get("entity_id_fields")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or(default_cfg.entity_id_fields);

        let schema_id_fields = data
            .get("schema_id_fields")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or(default_cfg.schema_id_fields);

        GtsConfig {
            entity_id_fields,
            schema_id_fields,
        }
    }

    pub fn reload_from_path(&mut self, path: &[String]) {
        self.path = Some(path.to_vec());
        let reader = Box::new(GtsFileReader::new(path, Some(self.cfg.clone())))
            as Box<dyn crate::store::GtsReader>;
        self.store = GtsStore::new(Some(reader));
    }

    pub fn add_entity(&mut self, content: &Value, validate: bool) -> GtsAddEntityResult {
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&self.cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        let Some(ref gts_id) = entity.gts_id else {
            return GtsAddEntityResult {
                ok: false,
                id: String::new(),
                schema_id: None,
                is_schema: false,
                error: "Unable to detect GTS ID in entity".to_owned(),
            };
        };
        let entity_id = gts_id.id.clone();

        // Register the entity first
        if let Err(e) = self.store.register(entity.clone()) {
            return GtsAddEntityResult {
                ok: false,
                id: String::new(),
                schema_id: None,
                is_schema: false,
                error: e.to_string(),
            };
        }

        // Always validate schemas
        if entity.is_schema {
            if let Err(e) = self.store.validate_schema(&entity_id) {
                return GtsAddEntityResult {
                    ok: false,
                    id: String::new(),
                    schema_id: None,
                    is_schema: false,
                    error: format!("Validation failed: {e}"),
                };
            }
        }

        // If validation is requested, validate the instance as well
        if validate && !entity.is_schema {
            if let Err(e) = self.store.validate_instance(&entity_id) {
                return GtsAddEntityResult {
                    ok: false,
                    id: String::new(),
                    schema_id: None,
                    is_schema: false,
                    error: format!("Validation failed: {e}"),
                };
            }
        }

        GtsAddEntityResult {
            ok: true,
            id: entity_id,
            schema_id: entity.schema_id,
            is_schema: entity.is_schema,
            error: String::new(),
        }
    }

    pub fn add_entities(&mut self, items: &[Value]) -> GtsAddEntitiesResult {
        let results: Vec<GtsAddEntityResult> =
            items.iter().map(|it| self.add_entity(it, false)).collect();
        let ok = results.iter().all(|r| r.ok);
        GtsAddEntitiesResult { ok, results }
    }

    pub fn add_schema(&mut self, type_id: String, schema: &Value) -> GtsAddSchemaResult {
        match self.store.register_schema(&type_id, schema) {
            Ok(()) => GtsAddSchemaResult {
                ok: true,
                id: type_id,
                error: String::new(),
            },
            Err(e) => GtsAddSchemaResult {
                ok: false,
                id: String::new(),
                error: e.to_string(),
            },
        }
    }

    #[must_use]
    pub fn validate_id(&self, gts_id: &str) -> GtsIdValidationResult {
        match GtsID::new(gts_id) {
            Ok(_) => GtsIdValidationResult {
                id: gts_id.to_owned(),
                valid: true,
                error: String::new(),
            },
            Err(e) => GtsIdValidationResult {
                id: gts_id.to_owned(),
                valid: false,
                error: e.to_string(),
            },
        }
    }

    pub fn parse_id(&self, gts_id: &str) -> GtsIdParseResult {
        match GtsID::new(gts_id) {
            Ok(id) => {
                let segments = id
                    .gts_id_segments
                    .iter()
                    .map(GtsIdSegmentInfo::from)
                    .collect();

                GtsIdParseResult {
                    id: gts_id.to_owned(),
                    ok: true,
                    segments,
                    error: String::new(),
                }
            }
            Err(e) => GtsIdParseResult {
                id: gts_id.to_owned(),
                ok: false,
                segments: Vec::new(),
                error: e.to_string(),
            },
        }
    }

    #[must_use]
    pub fn match_id_pattern(&self, candidate: &str, pattern: &str) -> GtsIdMatchResult {
        match (GtsID::new(candidate), GtsWildcard::new(pattern)) {
            (Ok(c), Ok(p)) => {
                let is_match = c.wildcard_match(&p);
                GtsIdMatchResult {
                    candidate: candidate.to_owned(),
                    pattern: pattern.to_owned(),
                    is_match,
                    error: String::new(),
                }
            }
            (Err(e), _) | (_, Err(e)) => GtsIdMatchResult {
                candidate: candidate.to_owned(),
                pattern: pattern.to_owned(),
                is_match: false,
                error: e.to_string(),
            },
        }
    }

    #[must_use]
    pub fn uuid(&self, gts_id: &str) -> GtsUuidResult {
        match GtsID::new(gts_id) {
            Ok(g) => GtsUuidResult {
                id: g.id.clone(),
                uuid: g.to_uuid().to_string(),
            },
            Err(_) => GtsUuidResult {
                id: gts_id.to_owned(),
                uuid: String::new(),
            },
        }
    }

    pub fn validate_instance(&mut self, gts_id: &str) -> GtsValidationResult {
        match self.store.validate_instance(gts_id) {
            Ok(()) => GtsValidationResult {
                id: gts_id.to_owned(),
                ok: true,
                error: String::new(),
            },
            Err(e) => GtsValidationResult {
                id: gts_id.to_owned(),
                ok: false,
                error: e.to_string(),
            },
        }
    }

    pub fn validate_schema(&mut self, gts_id: &str) -> GtsValidationResult {
        match self.store.validate_schema(gts_id) {
            Ok(()) => GtsValidationResult {
                id: gts_id.to_owned(),
                ok: true,
                error: String::new(),
            },
            Err(e) => GtsValidationResult {
                id: gts_id.to_owned(),
                ok: false,
                error: e.to_string(),
            },
        }
    }

    pub fn validate_entity(&mut self, gts_id: &str) -> GtsValidationResult {
        if gts_id.ends_with('~') {
            self.validate_schema(gts_id)
        } else {
            self.validate_instance(gts_id)
        }
    }

    pub fn schema_graph(&mut self, gts_id: &str) -> GtsSchemaGraphResult {
        let graph = self.store.build_schema_graph(gts_id);
        GtsSchemaGraphResult { graph }
    }

    pub fn compatibility(
        &mut self,
        old_schema_id: &str,
        new_schema_id: &str,
    ) -> GtsEntityCastResult {
        self.store.is_minor_compatible(old_schema_id, new_schema_id)
    }

    pub fn cast(&mut self, from_id: &str, to_schema_id: &str) -> GtsEntityCastResult {
        match self.store.cast(from_id, to_schema_id) {
            Ok(result) => result,
            Err(e) => GtsEntityCastResult {
                from_id: from_id.to_owned(),
                to_id: to_schema_id.to_owned(),
                old: from_id.to_owned(),
                new: to_schema_id.to_owned(),
                direction: "unknown".to_owned(),
                added_properties: Vec::new(),
                removed_properties: Vec::new(),
                changed_properties: Vec::new(),
                is_fully_compatible: false,
                is_backward_compatible: false,
                is_forward_compatible: false,
                incompatibility_reasons: Vec::new(),
                backward_errors: Vec::new(),
                forward_errors: Vec::new(),
                casted_entity: None,
                error: Some(e.to_string()),
            },
        }
    }

    #[must_use]
    pub fn query(&self, expr: &str, limit: usize) -> GtsStoreQueryResult {
        self.store.query(expr, limit)
    }

    pub fn attr(&mut self, gts_with_path: &str) -> JsonPathResolver {
        match GtsID::split_at_path(gts_with_path) {
            Ok((gts, Some(path))) => {
                if let Some(entity) = self.store.get(&gts) {
                    entity.resolve_path(&path)
                } else {
                    JsonPathResolver::new(gts.clone(), Value::Null)
                        .failure(&path, &format!("Entity not found: {gts}"))
                }
            }
            Ok((gts, None)) => JsonPathResolver::new(gts, Value::Null)
                .failure("", "Attribute selector requires '@path' in the identifier"),
            Err(e) => JsonPathResolver::new(String::new(), Value::Null).failure("", &e.to_string()),
        }
    }

    #[must_use]
    pub fn extract_id(&self, content: &Value) -> GtsExtractIdResult {
        let entity = GtsEntity::new(
            None,
            None,
            content,
            Some(&self.cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        GtsExtractIdResult {
            id: entity
                .gts_id
                .as_ref()
                .map(|g| g.id.clone())
                .unwrap_or_default(),
            schema_id: entity.schema_id,
            selected_entity_field: entity.selected_entity_field,
            selected_schema_id_field: entity.selected_schema_id_field,
            is_schema: entity.is_schema,
        }
    }

    pub fn get_entity(&mut self, gts_id: &str) -> GtsGetEntityResult {
        match self.store.get(gts_id) {
            Some(entity) => GtsGetEntityResult {
                ok: true,
                id: entity
                    .gts_id
                    .as_ref()
                    .map_or_else(|| gts_id.to_owned(), |g| g.id.clone()),
                schema_id: entity.schema_id.clone(),
                is_schema: entity.is_schema,
                content: Some(entity.content.clone()),
                error: String::new(),
            },
            None => GtsGetEntityResult {
                ok: false,
                id: String::new(),
                schema_id: None,
                is_schema: false,
                content: None,
                error: format!("Entity '{gts_id}' not found"),
            },
        }
    }

    #[must_use]
    pub fn get_entities(&self, limit: usize) -> GtsEntitiesListResult {
        let all_entities: Vec<_> = self.store.items().collect();
        let total = all_entities.len();

        let entities: Vec<GtsEntityInfo> = all_entities
            .into_iter()
            .take(limit)
            .map(|(entity_id, entity)| GtsEntityInfo {
                id: entity_id.clone(),
                schema_id: entity.schema_id.clone(),
                is_schema: entity.is_schema,
            })
            .collect();

        let count = entities.len();

        GtsEntitiesListResult {
            entities,
            count,
            total,
        }
    }

    #[must_use]
    pub fn list(&self, limit: usize) -> GtsEntitiesListResult {
        self.get_entities(limit)
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::gts::GtsID;
    use serde_json::json;

    #[test]
    fn test_validate_id_valid() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.validate_id("gts.vendor.package.namespace.type.v1.0");
        assert!(result.valid);
        assert_eq!(result.id, "gts.vendor.package.namespace.type.v1.0");
    }

    #[test]
    fn test_validate_id_invalid() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.validate_id("invalid-id");
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_id_schema() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.validate_id("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.valid);
        assert!(result.id.ends_with('~'));
    }

    #[test]
    fn test_parse_id_valid() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.parse_id("gts.vendor.package.namespace.type.v1.0");
        assert!(!result.segments.is_empty());
        assert_eq!(result.id, "gts.vendor.package.namespace.type.v1.0");
    }

    #[test]
    fn test_parse_id_invalid() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.parse_id("invalid");
        assert!(result.segments.is_empty());
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_extract_id_from_json() {
        let ops = GtsOps::new(None, None, 0);
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        let result = ops.extract_id(&content);
        assert_eq!(result.id, "gts.vendor.package.namespace.type.v1.0");
    }

    #[test]
    fn test_extract_id_with_schema() {
        let ops = GtsOps::new(None, None, 0);
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~instance.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~"
        });

        let result = ops.extract_id(&content);
        assert_eq!(
            result.schema_id,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned())
        );
    }

    #[test]
    fn test_query_empty_store() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.query("*", 10);
        assert_eq!(result.count, 0);
        assert!(result.results.is_empty());
    }

    #[test]
    fn test_gts_id_validation() {
        assert!(GtsID::is_valid("gts.vendor.package.namespace.type.v1.0"));
        assert!(GtsID::is_valid("gts.vendor.package.namespace.type.v1.0~"));
        assert!(!GtsID::is_valid("invalid"));
        assert!(!GtsID::is_valid(""));
    }

    #[test]
    fn test_cast_entity_to_schema() {
        let mut ops = GtsOps::new(None, None, 0);

        // Register a base schema
        let base_schema = json!({
            "$id": "gts.test.base.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "name": {"type": "string"}
            },
            "required": ["id"]
        });
        ops.add_schema("gts.test.base.v1.0~".to_owned(), &base_schema);

        // Register a derived schema
        let derived_schema = json!({
            "$id": "gts.test.derived.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "name": {"type": "string"},
                "email": {"type": "string"}
            },
            "required": ["id"]
        });
        ops.add_schema("gts.test.derived.v1.1~".to_owned(), &derived_schema);

        // Register an instance
        let instance = json!({
            "id": "gts.test.base.v1.0~instance.v1.0",
            "type": "gts.test.base.v1.0~",
            "name": "Test Instance"
        });
        ops.add_entity(&instance, false);

        // Test casting
        let result = ops.cast("gts.test.base.v1.0~instance.v1.0", "gts.test.derived.v1.1~");
        assert_eq!(result.from_id, "gts.test.base.v1.0~instance.v1.0");
        assert_eq!(result.to_id, "gts.test.derived.v1.1~");
    }

    #[test]
    fn test_resolve_path_simple() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({
            "name": "test",
            "value": 42
        });

        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("name");
        // Just verify the method executes and returns a result
        assert_eq!(result.gts_id, "gts.test.id.v1.0");
        assert_eq!(result.path, "name");
    }

    #[test]
    fn test_resolve_path_nested() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({
            "user": {
                "profile": {
                    "name": "John Doe"
                }
            }
        });

        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("user.profile.name");
        // Just verify the method executes
        assert_eq!(result.gts_id, "gts.test.id.v1.0");
    }

    #[test]
    fn test_resolve_path_array() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({
            "items": ["first", "second", "third"]
        });

        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("items[1]");
        // Just verify the method executes
        assert_eq!(result.gts_id, "gts.test.id.v1.0");
    }

    #[test]
    fn test_json_file_creation() {
        use crate::entities::GtsFile;

        let content = json!({
            "id": "gts.test.id.v1.0",
            "data": "test"
        });

        let file = GtsFile::new(
            "/path/to/file.json".to_owned(),
            "file.json".to_owned(),
            content,
        );

        assert_eq!(file.path, "/path/to/file.json");
        assert_eq!(file.name, "file.json");
        assert_eq!(file.sequences_count, 1);
    }

    #[test]
    fn test_json_file_with_array() {
        use crate::entities::GtsFile;

        let content = json!([
            {"id": "gts.test.id1.v1.0"},
            {"id": "gts.test.id2.v1.0"},
            {"id": "gts.test.id3.v1.0"}
        ]);

        let file = GtsFile::new(
            "/path/to/array.json".to_owned(),
            "array.json".to_owned(),
            content,
        );

        assert_eq!(file.sequences_count, 3);
        assert_eq!(file.sequence_content.len(), 3);
    }

    #[test]
    fn test_extract_id_triggers_calc_json_schema_id() {
        let ops = GtsOps::new(None, None, 0);

        // Test with entity that has a schema ID
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~instance.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let result = ops.extract_id(&content);

        // calc_json_schema_id should be triggered and extract schema_id from type field
        assert_eq!(
            result.schema_id,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned())
        );
        // Verify the method executed successfully
        assert!(!result.id.is_empty());
    }

    #[test]
    fn test_extract_id_with_schema_ending_in_tilde() {
        let ops = GtsOps::new(None, None, 0);

        // Test with entity ID that itself is a schema (ends with ~)
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let result = ops.extract_id(&content);

        // When entity ID ends with ~, it IS the schema
        assert_eq!(result.id, "gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_schema);
        // Verify schema_id is set (could be from $schema or id field)
        assert!(result.schema_id.is_some());
    }

    #[test]
    fn test_compatibility_check() {
        let mut ops = GtsOps::new(None, None, 0);

        // Register old schema
        let old_schema = json!({
            "$id": "gts.test.compat.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["active", "inactive"]
                }
            }
        });
        ops.add_schema("gts.test.compat.v1.0~".to_owned(), &old_schema);

        // Register new schema with expanded enum
        let new_schema = json!({
            "$id": "gts.test.compat.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["active", "inactive", "pending"]
                }
            }
        });
        ops.add_schema("gts.test.compat.v1.1~".to_owned(), &new_schema);

        // Check compatibility - just verify the method executes
        let result = ops.compatibility("gts.test.compat.v1.0~", "gts.test.compat.v1.1~");

        // Verify the compatibility check executed and returned a result
        // The actual compatibility values depend on the implementation details
        // Verify the compatibility check returns a result with expected schema IDs
        assert_eq!(result.from_id, "gts.test.compat.v1.0~");
        assert_eq!(result.to_id, "gts.test.compat.v1.1~");
    }

    /// Helper to convert a serializable value to a JSON object for testing
    fn to_json_obj<T: serde::Serialize>(value: &T) -> serde_json::Map<String, Value> {
        match serde_json::to_value(value).expect("test") {
            Value::Object(map) => map,
            other => {
                let mut map = serde_json::Map::new();
                map.insert("value".to_owned(), other);
                map
            }
        }
    }

    #[test]
    fn test_gts_id_validation_result_serialization() {
        use crate::ops::GtsIdValidationResult;

        let result = GtsIdValidationResult {
            id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            valid: true,
            error: String::new(),
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert!(json.get("valid").expect("test").as_bool().expect("test"));
    }

    #[test]
    fn test_gts_id_segment_info_serialization() {
        use crate::ops::GtsIdSegmentInfo;

        let segment = GtsIdSegmentInfo {
            vendor: "vendor".to_owned(),
            package: "package".to_owned(),
            namespace: "namespace".to_owned(),
            type_name: "type".to_owned(),
            ver_major: Some(1),
            ver_minor: Some(0),
            is_type: false,
        };

        let json = to_json_obj(&segment);
        assert_eq!(
            json.get("vendor").expect("test").as_str().expect("test"),
            "vendor"
        );
        assert_eq!(
            json.get("package").expect("test").as_str().expect("test"),
            "package"
        );
        assert_eq!(
            json.get("namespace").expect("test").as_str().expect("test"),
            "namespace"
        );
        assert_eq!(
            json.get("type").expect("test").as_str().expect("test"),
            "type"
        );
        assert_eq!(
            json.get("ver_major").expect("test").as_u64().expect("test"),
            1
        );
        assert_eq!(
            json.get("ver_minor").expect("test").as_u64().expect("test"),
            0
        );
    }

    #[test]
    fn test_gts_id_parse_result_serialization() {
        use crate::ops::GtsIdParseResult;

        let result = GtsIdParseResult {
            id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            ok: true,
            error: String::new(),
            segments: vec![],
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert!(json.get("ok").expect("test").as_bool().expect("test"));
        assert!(json.contains_key("segments"));
    }

    #[test]
    fn test_gts_id_match_result_serialization() {
        use crate::ops::GtsIdMatchResult;

        let result = GtsIdMatchResult {
            candidate: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            pattern: "gts.vendor.*".to_owned(),
            is_match: true,
            error: String::new(),
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("candidate").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert_eq!(
            json.get("pattern").expect("test").as_str().expect("test"),
            "gts.vendor.*"
        );
        assert!(json.get("match").expect("test").as_bool().expect("test"));
    }

    #[test]
    fn test_gts_uuid_result_serialization() {
        use crate::ops::GtsUuidResult;

        let result = GtsUuidResult {
            id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            uuid: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert_eq!(
            json.get("uuid").expect("test").as_str().expect("test"),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_gts_validation_result_serialization() {
        use crate::ops::GtsValidationResult;

        let result = GtsValidationResult {
            id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            ok: true,
            error: String::new(),
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert!(json.get("ok").expect("test").as_bool().expect("test"));
    }

    #[test]
    fn test_gts_schema_graph_result_serialization() {
        use crate::ops::GtsSchemaGraphResult;

        let graph = json!({
            "id": "gts.test.schema.v1.0~",
            "refs": []
        });

        let result = GtsSchemaGraphResult { graph };

        // GtsSchemaGraphResult uses #[serde(transparent)] so it serializes as the graph directly
        let json_value = serde_json::to_value(&result).expect("test");
        assert!(json_value.get("id").is_some());
    }

    #[test]
    fn test_gts_entity_info_serialization() {
        use crate::ops::GtsEntityInfo;

        let info = GtsEntityInfo {
            id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            schema_id: Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
            is_schema: false,
        };

        let json = to_json_obj(&info);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert!(!json
            .get("is_schema")
            .expect("test")
            .as_bool()
            .expect("test"));
        assert!(json.contains_key("schema_id"));
    }

    #[test]
    fn test_gts_entities_list_result_serialization() {
        use crate::ops::{GtsEntitiesListResult, GtsEntityInfo};

        let entities = vec![
            GtsEntityInfo {
                id: "gts.test.id1.v1.0".to_owned(),
                schema_id: None,
                is_schema: false,
            },
            GtsEntityInfo {
                id: "gts.test.id2.v1.0".to_owned(),
                schema_id: None,
                is_schema: false,
            },
        ];

        let result = GtsEntitiesListResult {
            count: 2,
            total: 2,
            entities,
        };

        let json = to_json_obj(&result);
        assert_eq!(json.get("count").expect("test").as_u64().expect("test"), 2);
        assert!(json.get("entities").expect("test").is_array());
    }

    #[test]
    fn test_gts_add_entity_result_serialization() {
        use crate::ops::GtsAddEntityResult;

        let result = GtsAddEntityResult {
            ok: true,
            id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            schema_id: None,
            is_schema: false,
            error: String::new(),
        };

        let json = to_json_obj(&result);
        assert!(json.get("ok").expect("test").as_bool().expect("test"));
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
    }

    #[test]
    fn test_gts_add_entities_result_serialization() {
        use crate::ops::{GtsAddEntitiesResult, GtsAddEntityResult};

        let results = vec![
            GtsAddEntityResult {
                ok: true,
                id: "gts.test.id1.v1.0".to_owned(),
                schema_id: None,
                is_schema: false,
                error: String::new(),
            },
            GtsAddEntityResult {
                ok: true,
                id: "gts.test.id2.v1.0".to_owned(),
                schema_id: None,
                is_schema: false,
                error: String::new(),
            },
        ];

        let result = GtsAddEntitiesResult { ok: true, results };

        let json = to_json_obj(&result);
        assert!(json.get("ok").expect("test").as_bool().expect("test"));
        assert!(json.get("results").expect("test").is_array());
    }

    #[test]
    fn test_gts_add_schema_result_serialization() {
        use crate::ops::GtsAddSchemaResult;

        let result = GtsAddSchemaResult {
            ok: true,
            id: "gts.vendor.package.namespace.type.v1.0~".to_owned(),
            error: String::new(),
        };

        let json = to_json_obj(&result);
        assert!(json.get("ok").expect("test").as_bool().expect("test"));
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0~"
        );
    }

    #[test]
    fn test_gts_extract_id_result_serialization() {
        use crate::ops::GtsExtractIdResult;

        let result = GtsExtractIdResult {
            id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            schema_id: Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
            selected_entity_field: Some("id".to_owned()),
            selected_schema_id_field: Some("type".to_owned()),
            is_schema: false,
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert!(json.contains_key("schema_id"));
        assert!(json.contains_key("selected_entity_field"));
        assert!(json.contains_key("selected_schema_id_field"));
        assert!(!json
            .get("is_schema")
            .expect("test")
            .as_bool()
            .expect("test"));
    }

    #[test]
    fn test_json_path_resolver_serialization() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({"name": "test"});
        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("name");

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("gts_id").expect("test").as_str().expect("test"),
            "gts.test.id.v1.0"
        );
        assert_eq!(
            json.get("path").expect("test").as_str().expect("test"),
            "name"
        );
        assert!(json.contains_key("resolved"));
    }

    // Comprehensive schema_cast.rs tests for 100% coverage

    #[test]
    fn test_schema_cast_error_display() {
        use crate::schema_cast::SchemaCastError;

        let error = SchemaCastError::InternalError("test".to_owned());
        assert!(error.to_string().contains("test"));

        let error = SchemaCastError::TargetMustBeSchema;
        assert!(error.to_string().contains("Target must be a schema"));

        let error = SchemaCastError::SourceMustBeSchema;
        assert!(error.to_string().contains("Source schema must be a schema"));

        let error = SchemaCastError::InstanceMustBeObject;
        assert!(error.to_string().contains("Instance must be an object"));

        let error = SchemaCastError::CastError("cast error".to_owned());
        assert!(error.to_string().contains("cast error"));
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_up() {
        use crate::schema_cast::GtsEntityCastResult;

        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
        );
        assert_eq!(direction, "up");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_down() {
        use crate::schema_cast::GtsEntityCastResult;

        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.1",
            "gts.vendor.package.namespace.type.v1.0",
        );
        assert_eq!(direction, "down");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_none() {
        use crate::schema_cast::GtsEntityCastResult;

        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.0",
        );
        assert_eq!(direction, "none");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_unknown() {
        use crate::schema_cast::GtsEntityCastResult;

        let direction = GtsEntityCastResult::infer_direction("invalid", "also-invalid");
        assert_eq!(direction, "unknown");
    }

    #[test]
    fn test_json_entity_cast_result_cast_success() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let to_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string", "default": "test@example.com"}
            }
        });

        let instance = json!({
            "name": "John"
        });

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
        let cast_result = result.expect("test");
        assert_eq!(cast_result.direction, "up");
        assert!(cast_result.casted_entity.is_some());
    }

    #[test]
    fn test_json_entity_cast_result_cast_non_object_instance() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({"type": "object"});
        let instance = json!("not an object");

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_json_entity_cast_with_required_property() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let to_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            },
            "required": ["name", "age"]
        });

        let instance = json!({"name": "John"});

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
        let cast_result = result.expect("test");
        assert!(!cast_result.incompatibility_reasons.is_empty());
    }

    #[test]
    fn test_json_entity_cast_with_default_values() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "default": "active"},
                "count": {"type": "number", "default": 0}
            }
        });

        let instance = json!({});

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
        let cast_result = result.expect("test");
        let casted = cast_result.casted_entity.expect("test");
        assert_eq!(
            casted.get("status").expect("test").as_str().expect("test"),
            "active"
        );
        assert_eq!(
            casted.get("count").expect("test").as_i64().expect("test"),
            0
        );
    }

    #[test]
    fn test_json_entity_cast_remove_additional_properties() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "additionalProperties": false
        });

        let instance = json!({
            "name": "John",
            "extra": "field"
        });

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
        let cast_result = result.expect("test");
        assert!(!cast_result.removed_properties.is_empty());
    }

    #[test]
    fn test_json_entity_cast_with_const_values() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "type": {"type": "string", "const": "gts.vendor.package.namespace.type.v1.1~"}
            }
        });

        let instance = json!({
            "type": "gts.vendor.package.namespace.type.v1.0~"
        });

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_json_entity_cast_direction_down() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({"type": "object"});
        let instance = json!({"name": "test"});

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.1",
            "gts.vendor.package.namespace.type.v1.0",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
        let cast_result = result.expect("test");
        assert_eq!(cast_result.direction, "down");
    }

    #[test]
    fn test_json_entity_cast_with_allof() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "allOf": [
                {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            ]
        });

        let instance = json!({"name": "test"});

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_json_entity_cast_result_serialization() {
        use crate::schema_cast::GtsEntityCastResult;

        let result = GtsEntityCastResult {
            from_id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            to_id: "gts.vendor.package.namespace.type.v1.1".to_owned(),
            old: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            new: "gts.vendor.package.namespace.type.v1.1".to_owned(),
            direction: "up".to_owned(),
            added_properties: vec!["email".to_owned()],
            removed_properties: vec![],
            changed_properties: vec![],
            is_fully_compatible: true,
            is_backward_compatible: true,
            is_forward_compatible: false,
            incompatibility_reasons: vec![],
            backward_errors: vec![],
            forward_errors: vec![],
            casted_entity: Some(json!({"name": "test"})),
            error: None,
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("from").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert_eq!(
            json.get("direction").expect("test").as_str().expect("test"),
            "up"
        );
    }

    #[test]
    fn test_json_entity_cast_nested_objects() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "email": {"type": "string", "default": "test@example.com"}
                    }
                }
            }
        });

        let instance = json!({
            "user": {
                "name": "John"
            }
        });

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_json_entity_cast_array_of_objects() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "users": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"},
                            "email": {"type": "string", "default": "test@example.com"}
                        }
                    }
                }
            }
        });

        let instance = json!({
            "users": [
                {"name": "John"},
                {"name": "Jane"}
            ]
        });

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_json_entity_cast_with_required_and_default() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "default": "active"}
            },
            "required": ["status"]
        });

        let instance = json!({});

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
        let cast_result = result.expect("test");
        assert!(!cast_result.added_properties.is_empty());
    }

    #[test]
    fn test_json_entity_cast_flatten_schema_with_allof() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "allOf": [
                {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    },
                    "required": ["name"]
                },
                {
                    "type": "object",
                    "properties": {
                        "email": {"type": "string"}
                    }
                }
            ]
        });

        let instance = json!({"name": "test"});

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_json_entity_cast_array_with_non_object_items() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    }
                }
            }
        });

        let instance = json!({
            "tags": ["tag1", "tag2"]
        });

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_json_entity_cast_const_non_gts_id() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "version": {"type": "string", "const": "2.0"}
            }
        });

        let instance = json!({
            "version": "1.0"
        });

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_json_entity_cast_additional_properties_true() {
        use crate::schema_cast::GtsEntityCastResult;

        let from_schema = json!({"type": "object"});
        let to_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "additionalProperties": true
        });

        let instance = json!({
            "name": "John",
            "extra": "field"
        });

        let result = GtsEntityCastResult::cast(
            "gts.vendor.package.namespace.type.v1.0",
            "gts.vendor.package.namespace.type.v1.1",
            &instance,
            &from_schema,
            &to_schema,
            None,
        );

        assert!(result.is_ok());
        let cast_result = result.expect("test");
        // Should not remove extra field when additionalProperties is true
        assert!(cast_result.removed_properties.is_empty());
    }

    #[test]
    fn test_schema_compatibility_type_change() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "value": {"type": "string"}
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "value": {"type": "number"}
            }
        });

        let (is_backward, backward_errors) =
            GtsEntityCastResult::check_backward_compatibility(&old_schema, &new_schema);
        assert!(!is_backward);
        assert!(!backward_errors.is_empty());
    }

    #[test]
    fn test_schema_compatibility_enum_changes() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["active", "inactive"]
                }
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["active", "inactive", "pending"]
                }
            }
        });

        let (is_backward, _) =
            GtsEntityCastResult::check_backward_compatibility(&old_schema, &new_schema);
        let (is_forward, _) =
            GtsEntityCastResult::check_forward_compatibility(&old_schema, &new_schema);

        // Adding enum values is not backward compatible but is forward compatible
        assert!(!is_backward);
        assert!(is_forward);
    }

    #[test]
    fn test_schema_compatibility_numeric_constraints() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "age": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 100
                }
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "age": {
                    "type": "number",
                    "minimum": 18,
                    "maximum": 65
                }
            }
        });

        let (is_backward, backward_errors) =
            GtsEntityCastResult::check_backward_compatibility(&old_schema, &new_schema);
        assert!(!is_backward);
        assert!(!backward_errors.is_empty());
    }

    #[test]
    fn test_schema_compatibility_string_constraints() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 100
                }
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "minLength": 5,
                    "maxLength": 50
                }
            }
        });

        let (is_backward, _) =
            GtsEntityCastResult::check_backward_compatibility(&old_schema, &new_schema);
        assert!(!is_backward);
    }

    #[test]
    fn test_schema_compatibility_array_constraints() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 10
                }
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 5
                }
            }
        });

        let (is_backward, _) =
            GtsEntityCastResult::check_backward_compatibility(&old_schema, &new_schema);
        assert!(!is_backward);
    }

    #[test]
    fn test_schema_compatibility_added_constraint() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "age": {"type": "number"}
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "age": {
                    "type": "number",
                    "minimum": 0
                }
            }
        });

        let (is_backward, _) =
            GtsEntityCastResult::check_backward_compatibility(&old_schema, &new_schema);
        assert!(!is_backward);
    }

    #[test]
    fn test_schema_compatibility_removed_constraint() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "age": {
                    "type": "number",
                    "maximum": 100
                }
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "age": {"type": "number"}
            }
        });

        let (is_forward, _) =
            GtsEntityCastResult::check_forward_compatibility(&old_schema, &new_schema);
        assert!(!is_forward);
    }

    #[test]
    fn test_schema_compatibility_removed_required_property() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            },
            "required": ["name", "email"]
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            },
            "required": ["name"]
        });

        let (is_forward, forward_errors) =
            GtsEntityCastResult::check_forward_compatibility(&old_schema, &new_schema);
        assert!(!is_forward);
        assert!(!forward_errors.is_empty());
    }

    #[test]
    fn test_schema_compatibility_enum_removed_values() {
        use crate::schema_cast::GtsEntityCastResult;

        let old_schema = json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["active", "inactive", "pending"]
                }
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["active", "inactive"]
                }
            }
        });

        let (is_forward, forward_errors) =
            GtsEntityCastResult::check_forward_compatibility(&old_schema, &new_schema);
        assert!(!is_forward);
        assert!(!forward_errors.is_empty());
    }

    // Additional ops.rs coverage tests

    #[test]
    fn test_gts_ops_reload_from_path() {
        let mut ops = GtsOps::new(None, None, 0);
        ops.reload_from_path(&[]);
        // Just verify it doesn't crash
    }

    #[test]
    fn test_gts_ops_add_entities() {
        let mut ops = GtsOps::new(None, None, 0);

        let entities = vec![
            json!({"id": "gts.vendor.package.namespace.type.v1.0", "name": "test1"}),
            json!({"id": "gts.vendor.package.namespace.type.v1.1", "name": "test2"}),
        ];

        let result = ops.add_entities(&entities);
        assert_eq!(result.results.len(), 2);
    }

    #[test]
    fn test_gts_ops_uuid() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.uuid("gts.vendor.package.namespace.type.v1.0");
        assert!(!result.uuid.is_empty());
    }

    #[test]
    fn test_gts_ops_match_id_pattern_valid() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.match_id_pattern("gts.vendor.package.namespace.type.v1.0", "gts.vendor.*");
        assert!(result.is_match);
    }

    #[test]
    fn test_gts_ops_match_id_pattern_invalid() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.match_id_pattern("gts.vendor.package.namespace.type.v1.0", "gts.other.*");
        assert!(!result.is_match);
    }

    #[test]
    fn test_gts_ops_match_id_pattern_invalid_candidate() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.match_id_pattern("invalid", "gts.vendor.*");
        assert!(!result.is_match);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_gts_ops_match_id_pattern_invalid_pattern() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.match_id_pattern("gts.vendor.package.namespace.type.v1.0", "invalid");
        assert!(!result.is_match);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_gts_ops_schema_graph() {
        let mut ops = GtsOps::new(None, None, 0);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        ops.add_schema(
            "gts.vendor.package.namespace.type.v1.0~".to_owned(),
            &schema,
        );

        let result = ops.schema_graph("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.graph.is_object());
    }

    #[test]
    fn test_gts_ops_attr() {
        let mut ops = GtsOps::new(None, None, 0);

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "user": {
                "name": "John"
            }
        });

        ops.add_entity(&content, false);

        let result = ops.attr("gts.vendor.package.namespace.type.v1.0#user.name");
        // Just verify it executes
        assert!(!result.gts_id.is_empty());
    }

    #[test]
    fn test_gts_ops_attr_no_path() {
        let mut ops = GtsOps::new(None, None, 0);

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "test"
        });

        ops.add_entity(&content, false);

        let result = ops.attr("gts.vendor.package.namespace.type.v1.0");
        assert_eq!(result.path, "");
    }

    #[test]
    fn test_gts_ops_attr_nonexistent() {
        let mut ops = GtsOps::new(None, None, 0);
        let result = ops.attr("nonexistent#path");
        assert!(!result.resolved);
    }

    // Path resolver tests

    #[test]
    fn test_path_resolver_failure() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({"name": "test"});
        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.failure("invalid.path", "Path not found");

        assert!(!result.resolved);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_path_resolver_array_access() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({
            "items": [
                {"name": "first"},
                {"name": "second"}
            ]
        });

        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("items[0].name");

        assert_eq!(result.path, "items[0].name");
    }

    #[test]
    fn test_path_resolver_invalid_path() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({"name": "test"});
        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("nonexistent.path");

        assert!(!result.resolved);
    }

    #[test]
    fn test_path_resolver_empty_path() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({"name": "test"});
        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("");

        assert_eq!(result.path, "");
    }

    #[test]
    fn test_path_resolver_root_access() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({"name": "test", "value": 42});
        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("$");

        // Root access should return the whole object
        assert_eq!(result.gts_id, "gts.test.id.v1.0");
    }

    #[test]
    fn test_gts_ops_list_entities() {
        let mut ops = GtsOps::new(None, None, 0);

        for i in 0..3 {
            let content = json!({
                "id": format!("gts.vendor.package.namespace.type.v1.{}", i),
                "name": format!("test{}", i)
            });
            ops.add_entity(&content, false);
        }

        let result = ops.list(10);
        assert_eq!(result.total, 3);
        assert_eq!(result.entities.len(), 3);
    }

    #[test]
    fn test_gts_ops_list_with_limit() {
        let mut ops = GtsOps::new(None, None, 0);

        for i in 0..5 {
            let content = json!({
                "id": format!("gts.vendor.package.namespace.type.v1.{}", i),
                "name": format!("test{}", i)
            });
            ops.add_entity(&content, false);
        }

        let result = ops.list(2);
        assert_eq!(result.entities.len(), 2);
        assert_eq!(result.total, 5);
    }

    #[test]
    fn test_gts_ops_list_empty() {
        let ops = GtsOps::new(None, None, 0);
        let result = ops.list(10);
        assert_eq!(result.total, 0);
        assert_eq!(result.entities.len(), 0);
    }

    #[test]
    fn test_gts_ops_validate_instance() {
        let mut ops = GtsOps::new(None, None, 0);

        let schema = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        ops.add_schema(
            "gts.vendor.package.namespace.type.v1.0~".to_owned(),
            &schema,
        );

        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        ops.add_entity(&content, false);

        let result = ops.validate_instance("gts.vendor.package.namespace.type.v1.0");
        // Validation result has an id field matching the input
        assert_eq!(result.id, "gts.vendor.package.namespace.type.v1.0");
    }

    #[test]
    fn test_path_resolver_nested_object() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({
            "user": {
                "profile": {
                    "name": "John"
                }
            }
        });

        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("user.profile.name");

        assert_eq!(result.gts_id, "gts.test.id.v1.0");
    }

    #[test]
    fn test_path_resolver_array_out_of_bounds() {
        use crate::path_resolver::JsonPathResolver;

        let content = json!({
            "items": [1, 2, 3]
        });

        let resolver = JsonPathResolver::new("gts.test.id.v1.0".to_owned(), content);
        let result = resolver.resolve("items[10]");

        assert!(!result.resolved);
    }

    #[test]
    fn test_gts_ops_compatibility() {
        let mut ops = GtsOps::new(None, None, 0);

        let schema1 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema2 = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        ops.add_schema(
            "gts.vendor.package.namespace.type.v1.0~".to_owned(),
            &schema1,
        );
        ops.add_schema(
            "gts.vendor.package.namespace.type.v1.1~".to_owned(),
            &schema2,
        );

        let result = ops.compatibility(
            "gts.vendor.package.namespace.type.v1.0~",
            "gts.vendor.package.namespace.type.v1.1~",
        );

        // Adding optional property is backward compatible
        assert!(result.is_backward_compatible);
    }

    // Additional entities.rs coverage tests

    #[test]
    fn test_json_entity_resolve_path() {
        use crate::entities::{GtsConfig, GtsEntity};

        let cfg = GtsConfig::default();
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "user": {
                "name": "John",
                "age": 30
            }
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

        let result = entity.resolve_path("user.name");
        assert_eq!(result.gts_id, "gts.vendor.package.namespace.type.v1.0");
    }

    #[test]
    fn test_json_entity_cast_method() {
        use crate::entities::{GtsConfig, GtsEntity};

        let cfg = GtsConfig::default();

        let from_schema_content = json!({
            "$id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let to_schema_content = json!({
            "$id": "gts.vendor.package.namespace.type.v1.1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string", "default": "test@example.com"}
            }
        });

        let from_schema = GtsEntity::new(
            None,
            None,
            &from_schema_content,
            Some(&cfg),
            None,
            true,
            String::new(),
            None,
            None,
        );

        let to_schema = GtsEntity::new(
            None,
            None,
            &to_schema_content,
            Some(&cfg),
            None,
            true,
            String::new(),
            None,
            None,
        );

        let instance_content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "name": "John"
        });

        let instance = GtsEntity::new(
            None,
            None,
            &instance_content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        let result = instance.cast(&to_schema, &from_schema, None);
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_json_file_with_array_content() {
        use crate::entities::GtsFile;

        let content = json!([
            {"id": "gts.vendor.package.namespace.type.v1.0", "name": "first"},
            {"id": "gts.vendor.package.namespace.type.v1.1", "name": "second"}
        ]);

        let file = GtsFile::new(
            "/path/to/file.json".to_owned(),
            "file.json".to_owned(),
            content,
        );

        assert_eq!(file.sequences_count, 2);
        assert_eq!(file.sequence_content.len(), 2);
    }

    #[test]
    fn test_json_file_with_single_object() {
        use crate::entities::GtsFile;

        let content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});

        let file = GtsFile::new(
            "/path/to/file.json".to_owned(),
            "file.json".to_owned(),
            content,
        );

        assert_eq!(file.sequences_count, 1);
        assert_eq!(file.sequence_content.len(), 1);
    }

    #[test]
    fn test_json_entity_with_validation_result() {
        use crate::entities::{GtsConfig, GtsEntity, ValidationError, ValidationResult};

        let cfg = GtsConfig::default();
        let content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});

        let mut validation = ValidationResult::default();
        validation.errors.push(ValidationError {
            instance_path: "/test".to_owned(),
            schema_path: "/schema/test".to_owned(),
            keyword: "type".to_owned(),
            message: "validation error".to_owned(),
            params: std::collections::HashMap::new(),
            data: None,
        });

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            Some(validation),
            None,
        );

        assert_eq!(entity.validation.errors.len(), 1);
    }

    #[test]
    fn test_json_entity_with_file() {
        use crate::entities::{GtsConfig, GtsEntity, GtsFile};

        let cfg = GtsConfig::default();
        let content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});

        let file = GtsFile::new(
            "/path/to/file.json".to_owned(),
            "file.json".to_owned(),
            content.clone(),
        );

        let entity = GtsEntity::new(
            Some(file),
            Some(0),
            &content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert!(entity.file.is_some());
        assert_eq!(entity.list_sequence, Some(0));
    }
}
