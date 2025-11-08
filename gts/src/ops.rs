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
    pub error: String,
}

impl GtsIdValidationResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("id".to_string(), Value::String(self.id.clone()));
        map.insert("valid".to_string(), Value::Bool(self.valid));
        map.insert("error".to_string(), Value::String(self.error.clone()));
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsIdSegment {
    pub vendor: String,
    pub package: String,
    pub namespace: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub ver_major: Option<u32>,
    pub ver_minor: Option<u32>,
    pub is_type: bool,
}

impl GtsIdSegment {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("vendor".to_string(), Value::String(self.vendor.clone()));
        map.insert("package".to_string(), Value::String(self.package.clone()));
        map.insert(
            "namespace".to_string(),
            Value::String(self.namespace.clone()),
        );
        map.insert("type".to_string(), Value::String(self.type_name.clone()));
        map.insert(
            "ver_major".to_string(),
            self.ver_major
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
        );
        map.insert(
            "ver_minor".to_string(),
            self.ver_minor
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
        );
        map.insert("is_type".to_string(), Value::Bool(self.is_type));
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsIdParseResult {
    pub id: String,
    pub ok: bool,
    pub segments: Vec<GtsIdSegment>,
    pub error: String,
}

impl GtsIdParseResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("id".to_string(), Value::String(self.id.clone()));
        map.insert("ok".to_string(), Value::Bool(self.ok));
        map.insert(
            "segments".to_string(),
            Value::Array(
                self.segments
                    .iter()
                    .map(|s| Value::Object(s.to_dict()))
                    .collect(),
            ),
        );
        map.insert("error".to_string(), Value::String(self.error.clone()));
        map
    }
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

impl GtsIdMatchResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert(
            "candidate".to_string(),
            Value::String(self.candidate.clone()),
        );
        map.insert("pattern".to_string(), Value::String(self.pattern.clone()));
        map.insert("match".to_string(), Value::Bool(self.is_match));
        if !self.error.is_empty() {
            map.insert("error".to_string(), Value::String(self.error.clone()));
        }
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsUuidResult {
    pub id: String,
    pub uuid: String,
}

impl GtsUuidResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("id".to_string(), Value::String(self.id.clone()));
        map.insert("uuid".to_string(), Value::String(self.uuid.clone()));
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsValidationResult {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

impl GtsValidationResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("id".to_string(), Value::String(self.id.clone()));
        map.insert("ok".to_string(), Value::Bool(self.ok));
        if !self.error.is_empty() {
            map.insert("error".to_string(), Value::String(self.error.clone()));
        }
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsSchemaGraphResult {
    pub graph: Value,
}

impl GtsSchemaGraphResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        if let Value::Object(map) = &self.graph {
            map.clone()
        } else {
            serde_json::Map::new()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsEntityInfo {
    pub id: String,
    pub schema_id: Option<String>,
    pub is_schema: bool,
}

impl GtsEntityInfo {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("id".to_string(), Value::String(self.id.clone()));
        map.insert(
            "schema_id".to_string(),
            self.schema_id
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
        );
        map.insert("is_schema".to_string(), Value::Bool(self.is_schema));
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsEntitiesListResult {
    pub entities: Vec<GtsEntityInfo>,
    pub count: usize,
    pub total: usize,
}

impl GtsEntitiesListResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert(
            "entities".to_string(),
            Value::Array(
                self.entities
                    .iter()
                    .map(|e| Value::Object(e.to_dict()))
                    .collect(),
            ),
        );
        map.insert("count".to_string(), Value::Number(self.count.into()));
        map.insert("total".to_string(), Value::Number(self.total.into()));
        map
    }
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

impl GtsAddEntityResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("ok".to_string(), Value::Bool(self.ok));
        if self.ok {
            map.insert("id".to_string(), Value::String(self.id.clone()));
            map.insert(
                "schema_id".to_string(),
                self.schema_id
                    .as_ref()
                    .map(|s| Value::String(s.clone()))
                    .unwrap_or(Value::Null),
            );
            map.insert("is_schema".to_string(), Value::Bool(self.is_schema));
        } else {
            map.insert("error".to_string(), Value::String(self.error.clone()));
        }
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsAddEntitiesResult {
    pub ok: bool,
    pub results: Vec<GtsAddEntityResult>,
}

impl GtsAddEntitiesResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("ok".to_string(), Value::Bool(self.ok));
        map.insert(
            "results".to_string(),
            Value::Array(
                self.results
                    .iter()
                    .map(|r| Value::Object(r.to_dict()))
                    .collect(),
            ),
        );
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsAddSchemaResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

impl GtsAddSchemaResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("ok".to_string(), Value::Bool(self.ok));
        if self.ok {
            map.insert("id".to_string(), Value::String(self.id.clone()));
        } else {
            map.insert("error".to_string(), Value::String(self.error.clone()));
        }
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsExtractIdResult {
    pub id: String,
    pub schema_id: Option<String>,
    pub selected_entity_field: Option<String>,
    pub selected_schema_id_field: Option<String>,
    pub is_schema: bool,
}

impl GtsExtractIdResult {
    pub fn to_dict(&self) -> serde_json::Map<String, Value> {
        let mut map = serde_json::Map::new();
        map.insert("id".to_string(), Value::String(self.id.clone()));
        map.insert(
            "schema_id".to_string(),
            self.schema_id
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
        );
        map.insert(
            "selected_entity_field".to_string(),
            self.selected_entity_field
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
        );
        map.insert(
            "selected_schema_id_field".to_string(),
            self.selected_schema_id_field
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
        );
        map.insert("is_schema".to_string(), Value::Bool(self.is_schema));
        map
    }
}

pub struct GtsOps {
    pub verbose: usize,
    pub cfg: GtsConfig,
    pub path: Option<Vec<String>>,
    pub store: GtsStore,
}

impl GtsOps {
    pub fn new(path: Option<Vec<String>>, config: Option<String>, verbose: usize) -> Self {
        let cfg = Self::load_config(config);
        let reader: Option<Box<dyn crate::store::GtsReader>> = path.as_ref().map(|p| {
            Box::new(GtsFileReader::new(p.clone(), Some(cfg.clone())))
                as Box<dyn crate::store::GtsReader>
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
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or(default_cfg.entity_id_fields);

        let schema_id_fields = data
            .get("schema_id_fields")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or(default_cfg.schema_id_fields);

        GtsConfig {
            entity_id_fields,
            schema_id_fields,
        }
    }

    pub fn reload_from_path(&mut self, path: Vec<String>) {
        self.path = Some(path.clone());
        let reader = Box::new(GtsFileReader::new(path, Some(self.cfg.clone())))
            as Box<dyn crate::store::GtsReader>;
        self.store = GtsStore::new(Some(reader));
    }

    pub fn add_entity(&mut self, content: Value) -> GtsAddEntityResult {
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

        if entity.gts_id.is_none() {
            return GtsAddEntityResult {
                ok: false,
                id: String::new(),
                schema_id: None,
                is_schema: false,
                error: "Unable to detect GTS ID in entity".to_string(),
            };
        }

        if let Err(e) = self.store.register(entity.clone()) {
            return GtsAddEntityResult {
                ok: false,
                id: String::new(),
                schema_id: None,
                is_schema: false,
                error: e.to_string(),
            };
        }

        GtsAddEntityResult {
            ok: true,
            id: entity.gts_id.as_ref().unwrap().id.clone(),
            schema_id: entity.schema_id,
            is_schema: entity.is_schema,
            error: String::new(),
        }
    }

    pub fn add_entities(&mut self, items: Vec<Value>) -> GtsAddEntitiesResult {
        let results: Vec<GtsAddEntityResult> =
            items.into_iter().map(|it| self.add_entity(it)).collect();
        let ok = results.iter().all(|r| r.ok);
        GtsAddEntitiesResult { ok, results }
    }

    pub fn add_schema(&mut self, type_id: String, schema: Value) -> GtsAddSchemaResult {
        match self.store.register_schema(&type_id, schema) {
            Ok(_) => GtsAddSchemaResult {
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

    pub fn validate_id(&self, gts_id: &str) -> GtsIdValidationResult {
        match GtsID::new(gts_id) {
            Ok(_) => GtsIdValidationResult {
                id: gts_id.to_string(),
                valid: true,
                error: String::new(),
            },
            Err(e) => GtsIdValidationResult {
                id: gts_id.to_string(),
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
                    .map(|s| GtsIdSegment {
                        vendor: s.vendor.clone(),
                        package: s.package.clone(),
                        namespace: s.namespace.clone(),
                        type_name: s.type_name.clone(),
                        ver_major: Some(s.ver_major),
                        ver_minor: s.ver_minor,
                        is_type: s.is_type,
                    })
                    .collect();

                GtsIdParseResult {
                    id: gts_id.to_string(),
                    ok: true,
                    segments,
                    error: String::new(),
                }
            }
            Err(e) => GtsIdParseResult {
                id: gts_id.to_string(),
                ok: false,
                segments: Vec::new(),
                error: e.to_string(),
            },
        }
    }

    pub fn match_id_pattern(&self, candidate: &str, pattern: &str) -> GtsIdMatchResult {
        match (GtsID::new(candidate), GtsWildcard::new(pattern)) {
            (Ok(c), Ok(p)) => {
                let is_match = c.wildcard_match(&p);
                GtsIdMatchResult {
                    candidate: candidate.to_string(),
                    pattern: pattern.to_string(),
                    is_match,
                    error: String::new(),
                }
            }
            (Err(e), _) | (_, Err(e)) => GtsIdMatchResult {
                candidate: candidate.to_string(),
                pattern: pattern.to_string(),
                is_match: false,
                error: e.to_string(),
            },
        }
    }

    pub fn uuid(&self, gts_id: &str) -> GtsUuidResult {
        let g = GtsID::new(gts_id).unwrap();
        GtsUuidResult {
            id: g.id.clone(),
            uuid: g.to_uuid().to_string(),
        }
    }

    pub fn validate_instance(&mut self, gts_id: &str) -> GtsValidationResult {
        match self.store.validate_instance(gts_id) {
            Ok(_) => GtsValidationResult {
                id: gts_id.to_string(),
                ok: true,
                error: String::new(),
            },
            Err(e) => GtsValidationResult {
                id: gts_id.to_string(),
                ok: false,
                error: e.to_string(),
            },
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
                from_id: from_id.to_string(),
                to_id: to_schema_id.to_string(),
                old: from_id.to_string(),
                new: to_schema_id.to_string(),
                direction: "unknown".to_string(),
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
                        .failure(&path, &format!("Entity not found: {}", gts))
                }
            }
            Ok((gts, None)) => JsonPathResolver::new(gts, Value::Null)
                .failure("", "Attribute selector requires '@path' in the identifier"),
            Err(e) => JsonPathResolver::new(String::new(), Value::Null).failure("", &e.to_string()),
        }
    }

    pub fn extract_id(&self, content: Value) -> GtsExtractIdResult {
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

    pub fn list(&self, limit: usize) -> GtsEntitiesListResult {
        self.get_entities(limit)
    }
}
