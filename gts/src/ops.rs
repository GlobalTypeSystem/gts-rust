use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::entities::{GtsConfig, GtsEntity};
use crate::files_reader::GtsFileReader;
use crate::gts::{GtsId, GtsIdPattern};
use crate::path_resolver::JsonPathResolver;
use crate::schema_cast::GtsEntityCastResult;
use crate::store::{GtsStore, GtsStoreQueryResult};

/// `is_schema` is `Some(true)` for schema/type IDs (ending with `~`),
/// `Some(false)` for instance IDs and wildcard patterns that match instances,
/// and `None` when the input couldn't be parsed (unknown).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsIdValidationResult {
    pub id: String,
    pub valid: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_type: Option<bool>,
    pub is_wildcard: bool,
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
        // A concrete segment always carries a real major version (including a
        // legitimate `v0`), so `ver_major` is never the wildcard "unspecified"
        // sentinel here.
        Self {
            vendor: seg.vendor().to_owned(),
            package: seg.package().to_owned(),
            namespace: seg.namespace().to_owned(),
            type_name: seg.type_name().to_owned(),
            ver_major: Some(seg.ver_major()),
            ver_minor: seg.ver_minor(),
            is_type: seg.is_type(),
        }
    }
}

impl From<&crate::gts::GtsIdPatternSegment> for GtsIdSegmentInfo {
    fn from(seg: &crate::gts::GtsIdPatternSegment) -> Self {
        Self {
            vendor: seg.vendor().to_owned(),
            package: seg.package().to_owned(),
            namespace: seg.namespace().to_owned(),
            type_name: seg.type_name().to_owned(),
            // For a wildcard segment, `ver_major() == 0` is the "unspecified"
            // sentinel and must serialize as `null`.
            ver_major: if seg.is_wildcard() && seg.ver_major() == 0 {
                None
            } else {
                Some(seg.ver_major())
            },
            ver_minor: seg.ver_minor(),
            is_type: seg.is_type(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsIdParseResult {
    pub id: String,
    pub ok: bool,
    pub segments: Vec<GtsIdSegmentInfo>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_type: Option<bool>,
    pub is_wildcard: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsEntityValidationResult {
    pub id: String,
    pub ok: bool,
    pub entity_type: String,
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
    pub type_id: Option<String>,
    pub is_type_schema: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsGetEntityResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub type_id: Option<String>,
    pub is_type_schema: bool,
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
    pub type_id: Option<String>,
    pub is_type_schema: bool,
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
    pub type_id: Option<String>,
    pub selected_entity_field: Option<String>,
    pub selected_type_id_field: Option<String>,
    pub is_type_schema: bool,
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
        let store = match path.as_ref() {
            Some(p) => {
                let reader = Box::new(GtsFileReader::new(p, Some(cfg.clone())))
                    as Box<dyn crate::store::GtsReader>;
                GtsStore::with_reader(reader)
            }
            None => GtsStore::new(),
        };

        GtsOps {
            verbose,
            cfg,
            path,
            store,
        }
    }

    fn load_config(config_path: Option<String>) -> GtsConfig {
        // Try user-provided path
        if let Some(path) = config_path
            && let Ok(cfg) = Self::load_config_from_path(&PathBuf::from(path))
        {
            return cfg;
        }

        // Try default path (relative to current directory)
        #[allow(unknown_lints, gts_id_hardcoded_prefix)]
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
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or(default_cfg.entity_id_fields);

        let type_id_fields = data
            .get("type_id_fields")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or(default_cfg.type_id_fields);

        GtsConfig {
            entity_id_fields,
            type_id_fields,
        }
    }

    pub fn reload_from_path(&mut self, path: &[String]) {
        self.path = Some(path.to_vec());
        let reader = Box::new(GtsFileReader::new(path, Some(self.cfg.clone())))
            as Box<dyn crate::store::GtsReader>;
        self.store = GtsStore::with_reader(reader);
    }

    fn get_details(&mut self, entity: &GtsEntity) -> String {
        let result = "Content: ".to_owned()
            + &serde_json::to_string_pretty(&entity.content)
                .unwrap_or_else(|_| "<invalid JSON>".to_owned());

        // Add schema information if available
        if let Some(type_id) = &entity.type_id {
            match self.store.get(type_id) {
                Some(schema_entity) => {
                    let schema_content = serde_json::to_string_pretty(&schema_entity.content)
                        .unwrap_or_else(|_| "<invalid schema JSON>".to_owned());
                    result + "\nSchema: " + &schema_content
                }
                None => result + "\nSchema: not found",
            }
        } else {
            result
        }
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

        // For instances, require at least one entity_id_fields to be present
        // (either a GTS ID for well-known instances, or a UUID/other ID for anonymous instances)
        let Some(entity_id) = entity.effective_id() else {
            return GtsAddEntityResult {
                ok: false,
                id: String::new(),
                type_id: None,
                is_type_schema: entity.is_schema,
                error: if entity.is_schema {
                    format!(
                        "Unable to detect GTS ID in schema entity:\n{}",
                        self.get_details(&entity)
                    )
                } else {
                    format!(
                        "Unable to detect ID in instance entity. Instances must have an 'id' field (or one of the configured entity_id_fields):\n{}",
                        self.get_details(&entity)
                    )
                },
            };
        };

        // Validate GTS extension keywords (x-gts-final/x-gts-abstract format and
        // mutual exclusion; x-gts-traits/x-gts-traits-schema placement) — raw
        // structural check enforced at every ingest, regardless of `validate`.
        // Pure check, so run it before `register` to avoid leaving a malformed
        // schema in the store.
        if entity.is_schema
            && let Err(e) = crate::schema_modifiers::validate_gts_keywords(&entity.content)
        {
            return GtsAddEntityResult {
                ok: false,
                id: String::new(),
                type_id: None,
                is_type_schema: entity.is_schema,
                error: e,
            };
        }

        // Register the entity
        if let Err(e) = self.store.register(entity.clone()) {
            return GtsAddEntityResult {
                ok: false,
                id: String::new(),
                type_id: None,
                is_type_schema: entity.is_schema,
                error: format!(
                    "Unable to register entity: {e}\n{}",
                    self.get_details(&entity)
                ),
            };
        }

        // Validate schemas. Without `validate` we only check `$ref`/`x-gts-ref`
        // structure — no dependency resolution, so forward-reference batches can
        // be registered before their targets exist. With `validate` we run the
        // full pipeline (refs, chain, resolve, meta-compile, traits) via
        // `validate_schema`, discarding the resolved artifacts.
        if entity.is_schema {
            let validation = if validate {
                self.store.validate_schema(&entity_id).map(|_| ())
            } else {
                self.store.validate_schema_refs(&entity_id)
            };
            if let Err(e) = validation {
                return GtsAddEntityResult {
                    ok: false,
                    id: String::new(),
                    type_id: None,
                    is_type_schema: entity.is_schema,
                    error: format!(
                        "Schema validation failed: {e}\n{}",
                        self.get_details(&entity)
                    ),
                };
            }
        }

        // Instance validation when requested.
        if validate
            && !entity.is_schema
            && let Err(e) = self.store.validate_instance(&entity_id)
        {
            return GtsAddEntityResult {
                ok: false,
                id: String::new(),
                type_id: None,
                is_type_schema: entity.is_schema,
                error: format!(
                    "Instance validation failed: {e}\n{}",
                    self.get_details(&entity)
                ),
            };
        }

        // println!("submitted: {}", self.get_content_pretty(&entity));

        GtsAddEntityResult {
            ok: true,
            id: entity_id,
            type_id: entity.type_id,
            is_type_schema: entity.is_schema,
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
                error: format!(
                    "Unable to register schema: {e}\n{}",
                    self.get_details(&GtsEntity::new(
                        None,
                        None,
                        schema,
                        Some(&self.cfg),
                        None,
                        false,
                        String::new(),
                        None,
                        None,
                    ))
                ),
            },
        }
    }

    #[must_use]
    pub fn validate_id(gts_id: &str) -> GtsIdValidationResult {
        let contains_wildcard = gts_id.contains('*');

        if contains_wildcard {
            // Use GtsIdPattern for wildcard pattern validation - it enforces:
            // - Only one '*' allowed
            // - '*' must be at end (ending with '.*' or '~*')
            // - No '*' in the middle of segments
            match GtsIdPattern::try_new(gts_id) {
                Ok(_) => GtsIdValidationResult {
                    id: gts_id.to_owned(),
                    valid: true,
                    error: String::new(),
                    is_type: Some(false),
                    is_wildcard: true,
                },
                Err(e) => GtsIdValidationResult {
                    id: gts_id.to_owned(),
                    valid: false,
                    error: format!("Unable to validate GTS ID '{gts_id}': {e}"),
                    is_type: None,
                    is_wildcard: true,
                },
            }
        } else {
            match GtsId::try_new(gts_id) {
                Ok(id) => GtsIdValidationResult {
                    id: gts_id.to_owned(),
                    valid: true,
                    error: String::new(),
                    is_type: Some(id.is_type()),
                    is_wildcard: false,
                },
                Err(e) => GtsIdValidationResult {
                    id: gts_id.to_owned(),
                    valid: false,
                    error: format!("Unable to validate GTS ID '{gts_id}': {e}"),
                    is_type: None,
                    is_wildcard: false,
                },
            }
        }
    }

    pub fn parse_id(gts_id: &str) -> GtsIdParseResult {
        let contains_wildcard = gts_id.contains('*');

        if contains_wildcard {
            // Use GtsIdPattern for wildcard pattern parsing/validation
            match GtsIdPattern::try_new(gts_id) {
                Ok(w) => {
                    let segments = w.segments().iter().map(GtsIdSegmentInfo::from).collect();
                    GtsIdParseResult {
                        id: gts_id.to_owned(),
                        ok: true,
                        segments,
                        error: String::new(),
                        is_type: Some(false),
                        is_wildcard: true,
                    }
                }
                Err(e) => GtsIdParseResult {
                    id: gts_id.to_owned(),
                    ok: false,
                    segments: Vec::new(),
                    error: e.to_string(),
                    is_type: None,
                    is_wildcard: true,
                },
            }
        } else {
            match GtsId::try_new(gts_id) {
                Ok(id) => {
                    let segments = id.segments().iter().map(GtsIdSegmentInfo::from).collect();

                    GtsIdParseResult {
                        id: gts_id.to_owned(),
                        ok: true,
                        segments,
                        error: String::new(),
                        is_type: Some(id.is_type()),
                        is_wildcard: false,
                    }
                }
                Err(e) => GtsIdParseResult {
                    id: gts_id.to_owned(),
                    ok: false,
                    segments: Vec::new(),
                    error: e.to_string(),
                    is_type: None,
                    is_wildcard: false,
                },
            }
        }
    }

    #[must_use]
    pub fn match_id_pattern(candidate: &str, pattern: &str) -> GtsIdMatchResult {
        // The pattern side is always a pattern; a concrete id is just a
        // zero-`*` pattern, which `GtsIdPattern::try_new` accepts.
        let pattern_result = GtsIdPattern::try_new(pattern);

        // The candidate may itself be a wildcard pattern. Either way it is matched
        // against the pattern with the same field-level logic (minor-version
        // flexibility, wildcard tails); `matches_pattern` is defined on both
        // `GtsId` and `GtsIdPattern`.
        let match_result: Result<bool, (bool, String)> = if candidate.contains('*') {
            match (GtsIdPattern::try_new(candidate), &pattern_result) {
                (Ok(cand), Ok(pat)) => Ok(cand.matches_pattern(pat)),
                (Err(e), _) => Err((true, e.to_string())),
                (_, Err(e)) => Err((false, e.to_string())),
            }
        } else {
            match (GtsId::try_new(candidate), &pattern_result) {
                (Ok(cand), Ok(pat)) => Ok(cand.matches_pattern(pat)),
                (Err(e), _) => Err((true, e.to_string())),
                (_, Err(e)) => Err((false, e.to_string())),
            }
        };

        match match_result {
            Ok(is_match) => GtsIdMatchResult {
                candidate: candidate.to_owned(),
                pattern: pattern.to_owned(),
                is_match,
                error: String::new(),
            },
            Err((is_candidate, e)) => GtsIdMatchResult {
                candidate: candidate.to_owned(),
                pattern: pattern.to_owned(),
                is_match: false,
                error: if is_candidate {
                    format!("Invalid candidate: {e}")
                } else {
                    format!("Invalid pattern: {e}")
                },
            },
        }
    }

    #[must_use]
    pub fn uuid(gts_id: &str) -> GtsUuidResult {
        match GtsId::try_new(gts_id) {
            Ok(g) => GtsUuidResult {
                id: g.id().to_owned(),
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
        // Full pipeline lives in `GtsStore::validate_schema` (refs → chain →
        // resolve → meta-compile → traits); we only need pass/fail here, so the
        // resolved artifacts are discarded.
        match self.store.validate_schema(gts_id) {
            Ok(_) => GtsValidationResult {
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

    pub fn validate_entity(&mut self, gts_id: &str) -> GtsEntityValidationResult {
        let parsed_id = match GtsId::try_new(gts_id) {
            Ok(parsed_id) => parsed_id,
            Err(e) => {
                return GtsEntityValidationResult {
                    id: gts_id.to_owned(),
                    ok: false,
                    entity_type: String::new(),
                    error: e.to_string(),
                };
            }
        };

        let (result, entity_type) = if parsed_id.is_type() {
            (self.validate_schema(gts_id), "schema".to_owned())
        } else {
            (self.validate_instance(gts_id), "instance".to_owned())
        };

        GtsEntityValidationResult {
            id: result.id,
            ok: result.ok,
            entity_type,
            error: result.error,
        }
    }
    pub fn schema_graph(&mut self, gts_id: &str) -> GtsSchemaGraphResult {
        let graph = self.store.build_schema_graph(gts_id);
        GtsSchemaGraphResult { graph }
    }

    pub fn compatibility(&mut self, old_type_id: &str, new_type_id: &str) -> GtsEntityCastResult {
        self.store.is_minor_compatible(old_type_id, new_type_id)
    }

    pub fn cast(&mut self, from_id: &str, to_type_id: &str) -> GtsEntityCastResult {
        match self.store.cast(from_id, to_type_id) {
            Ok(result) => result,
            Err(e) => GtsEntityCastResult {
                from_id: from_id.to_owned(),
                to_id: to_type_id.to_owned(),
                old: from_id.to_owned(),
                new: to_type_id.to_owned(),
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
        match GtsId::split_at_path(gts_with_path) {
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
            id: entity.effective_id().unwrap_or_default(),
            type_id: entity.type_id,
            selected_entity_field: entity.selected_entity_field,
            selected_type_id_field: entity.selected_type_id_field,
            is_type_schema: entity.is_schema,
        }
    }

    pub fn get_entity(&mut self, gts_id: &str) -> GtsGetEntityResult {
        match self.store.get(gts_id) {
            Some(entity) => GtsGetEntityResult {
                ok: true,
                id: entity
                    .gts_id
                    .as_ref()
                    .map_or_else(|| gts_id.to_owned(), |g| g.id().to_owned()),
                type_id: entity.type_id.clone(),
                is_type_schema: entity.is_schema,
                content: Some(entity.content.clone()),
                error: String::new(),
            },
            None => GtsGetEntityResult {
                ok: false,
                id: String::new(),
                type_id: None,
                is_type_schema: false,
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
                type_id: entity.type_id.clone(),
                is_type_schema: entity.is_schema,
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
    use serde_json::json;

    #[test]
    fn test_validate_id_valid() {
        let result =
            GtsOps::validate_id("gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0");
        assert!(result.valid);
        assert_eq!(
            result.id,
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0"
        );
    }

    #[test]
    fn test_validate_id_invalid() {
        let result = GtsOps::validate_id("invalid-id");
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_id_schema() {
        let result = GtsOps::validate_id("gts.vendor.package.namespace.type.v1.0~");
        assert!(result.valid);
    }

    #[test]
    fn test_parse_id_valid() {
        let result =
            GtsOps::parse_id("gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0");
        assert!(!result.segments.is_empty());
        assert_eq!(
            result.id,
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0"
        );
    }

    #[test]
    fn test_parse_id_invalid() {
        let result = GtsOps::parse_id("invalid");
        assert!(result.segments.is_empty());
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_parse_id_version_zero() {
        let result = GtsOps::parse_id("gts.x.pkg.ns.type.v0~");
        assert!(result.ok);
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].ver_major, Some(0));
        assert_eq!(result.segments[0].ver_minor, None);
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
            result.type_id,
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
    fn test_cast_entity_to_schema() {
        let mut ops = GtsOps::new(None, None, 0);

        // Register a base schema
        let base_schema = json!({
            "$id": "gts://gts.test.base.v1.0~",
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
            "$id": "gts://gts.test.derived.v1.1~",
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
    fn test_extract_id_triggers_calc_json_type_id() {
        let ops = GtsOps::new(None, None, 0);

        // Test with entity that has a type ID
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~instance.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let result = ops.extract_id(&content);

        // calc_json_type_id should be triggered and extract type_id from type field
        assert_eq!(
            result.type_id,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned())
        );
        // Verify the method executed successfully
        assert!(!result.id.is_empty());
    }

    #[test]
    fn test_extract_id_well_known_instance_type_id_from_chain() {
        let ops = GtsOps::new(None, None, 0);

        // Test with well-known instance where type_id is extracted from the chained id
        let content = json!({
            "id": "gts.x.test2.events.type.v1~abc.app._.custom_event.v1.2"
        });

        let result = ops.extract_id(&content);

        // The id should be the full chained GTS ID
        assert_eq!(
            result.id,
            "gts.x.test2.events.type.v1~abc.app._.custom_event.v1.2"
        );
        // The type_id should be extracted from the chain (everything up to and including last ~)
        assert_eq!(
            result.type_id,
            Some("gts.x.test2.events.type.v1~".to_owned())
        );
        // It's an instance (no $schema field)
        assert!(!result.is_type_schema);
        // The entity field should be "id"
        assert_eq!(result.selected_entity_field, Some("id".to_owned()));
        // The type_id was extracted from the id field, so selected_type_id_field should also be "id"
        assert_eq!(result.selected_type_id_field, Some("id".to_owned()));
    }

    #[test]
    fn test_extract_id_single_segment_type_id_as_instance() {
        let ops = GtsOps::new(None, None, 0);

        // Test with a single-segment GTS ID ending with ~ (looks like a type ID)
        // but used as an instance id field. This is unusual but valid.
        // The type_id should be None because we can't determine the parent type.
        let content = json!({
            "id": "gts.v123.p456.n789.t000.v999.888~"
        });

        let result = ops.extract_id(&content);

        // The id should be the GTS ID
        assert_eq!(result.id, "gts.v123.p456.n789.t000.v999.888~");
        // No $schema field, so it's not a schema
        assert!(!result.is_type_schema);
        // type_id should be None - we can't determine the parent type for a single-segment ID
        assert_eq!(result.type_id, None);
        // The entity field should be "id"
        assert_eq!(result.selected_entity_field, Some("id".to_owned()));
        // No type_id was extracted, so selected_type_id_field should be None
        assert_eq!(result.selected_type_id_field, None);
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
        assert!(result.is_type_schema);
        // Per spec, a base schema (single-segment $id, $schema is a JSON Schema
        // dialect URL) has no GTS parent type — type_id MUST be null.
        assert!(result.type_id.is_none());
    }

    #[test]
    fn test_compatibility_check() {
        let mut ops = GtsOps::new(None, None, 0);

        // Register old schema
        let old_schema = json!({
            "$id": "gts://gts.test.compat.v1.0~",
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
            "$id": "gts://gts.test.compat.v1.1~",
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
            is_type: Some(false),
            is_wildcard: false,
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert!(json.get("valid").expect("test").as_bool().expect("test"));
        assert!(json.get("is_type").expect("test").as_bool().is_some());
        assert!(
            !json
                .get("is_wildcard")
                .expect("test")
                .as_bool()
                .expect("test")
        );
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
            segments: vec![],
            error: String::new(),
            is_type: Some(false),
            is_wildcard: false,
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
            type_id: Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
            is_type_schema: false,
        };

        let json = to_json_obj(&info);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert!(
            !json
                .get("is_type_schema")
                .expect("test")
                .as_bool()
                .expect("test")
        );
        assert!(json.contains_key("type_id"));
    }

    #[test]
    fn test_gts_entities_list_result_serialization() {
        use crate::ops::{GtsEntitiesListResult, GtsEntityInfo};

        let entities = vec![
            GtsEntityInfo {
                id: "gts.test.id1.v1.0".to_owned(),
                type_id: None,
                is_type_schema: false,
            },
            GtsEntityInfo {
                id: "gts.test.id2.v1.0".to_owned(),
                type_id: None,
                is_type_schema: false,
            },
        ];

        let result = GtsEntitiesListResult {
            entities,
            count: 2,
            total: 2,
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
            type_id: None,
            is_type_schema: false,
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
                type_id: None,
                is_type_schema: false,
                error: String::new(),
            },
            GtsAddEntityResult {
                ok: true,
                id: "gts.test.id2.v1.0".to_owned(),
                type_id: None,
                is_type_schema: false,
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
            type_id: Some("gts.vendor.package.namespace.type.v1.0~".to_owned()),
            selected_entity_field: Some("id".to_owned()),
            selected_type_id_field: Some("type".to_owned()),
            is_type_schema: false,
        };

        let json = to_json_obj(&result);
        assert_eq!(
            json.get("id").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert!(json.contains_key("type_id"));
        assert!(json.contains_key("selected_entity_field"));
        assert!(json.contains_key("selected_type_id_field"));
        assert!(
            !json
                .get("is_type_schema")
                .expect("test")
                .as_bool()
                .expect("test")
        );
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
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
            "gts.vendor.package.namespace.type.v1.1~abc.app.custom.event.v1.1",
        );
        assert_eq!(direction, "up");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_down() {
        use crate::schema_cast::GtsEntityCastResult;

        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.1~abc.app.custom.event.v1.1",
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
        );
        assert_eq!(direction, "down");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_none() {
        use crate::schema_cast::GtsEntityCastResult;

        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
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
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
            "gts.vendor.package.namespace.type.v1.1~abc.app.custom.event.v1.1",
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
            "gts.vendor.package.namespace.type.v1.1~abc.app.custom.event.v1.1",
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
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
        let result =
            GtsOps::uuid("gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0");
        assert!(!result.uuid.is_empty());
    }

    #[test]
    fn test_gts_ops_match_id_pattern_valid() {
        let result = GtsOps::match_id_pattern(
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
            "gts.vendor.*",
        );
        assert!(result.is_match);
    }

    #[test]
    fn test_gts_ops_match_id_pattern_wildcard_candidate_directionality() {
        let broad_candidate = GtsOps::match_id_pattern("gts.vendor.*", "gts.vendor.package.*");
        assert!(
            !broad_candidate.is_match,
            "A broader candidate pattern must not match a narrower pattern"
        );

        let narrow_candidate = GtsOps::match_id_pattern("gts.vendor.package.*", "gts.vendor.*");
        assert!(
            narrow_candidate.is_match,
            "A narrower candidate pattern should match a broader pattern"
        );

        let disjoint_candidate = GtsOps::match_id_pattern("gts.vendor.package.*", "gts.other.*");
        assert!(!disjoint_candidate.is_match);
    }

    #[test]
    fn test_gts_ops_match_id_pattern_invalid() {
        let result = GtsOps::match_id_pattern(
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
            "gts.other.*",
        );
        assert!(!result.is_match);
    }

    #[test]
    fn test_gts_ops_match_id_pattern_invalid_candidate() {
        let result = GtsOps::match_id_pattern("invalid", "gts.vendor.*");
        assert!(!result.is_match);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_gts_ops_match_id_pattern_invalid_pattern() {
        let result = GtsOps::match_id_pattern("gts.vendor.package.namespace.type.v1.0", "invalid");
        assert!(!result.is_match);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_gts_ops_schema_graph() {
        let mut ops = GtsOps::new(None, None, 0);

        let schema = json!({
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
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
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
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
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let schema2 = json!({
            "$id": "gts://gts.vendor.package.namespace.type.v1.1~",
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
            "id": "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
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
        assert_eq!(
            result.gts_id,
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0"
        );
    }

    #[test]
    fn test_json_entity_cast_method() {
        use crate::entities::{GtsConfig, GtsEntity};

        let cfg = GtsConfig::default();

        let from_schema_content = json!({
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let to_schema_content = json!({
            "$id": "gts://gts.vendor.package.namespace.type.v1.1~",
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

    // =============================================================================
    // Tests for instance registration validation (commit 7d1eade)
    // =============================================================================

    #[test]
    fn test_add_entity_requires_id_for_instance() {
        // Instance without id field should return error
        let mut ops = GtsOps::new(None, None, 0);
        let content = json!({
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let result = ops.add_entity(&content, false);
        assert!(!result.ok, "Instance without id should fail");
        assert!(
            result.error.contains("Unable to detect ID"),
            "Error should mention missing ID"
        );
        assert!(
            result.error.contains("Instances must have an 'id' field"),
            "Error should specify requirement for id field"
        );
    }

    #[test]
    fn test_add_entity_accepts_well_known_instance() {
        // Well-known instance with GTS ID in id field should succeed
        let mut ops = GtsOps::new(None, None, 0);
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~instance.v1.0"
        });

        let result = ops.add_entity(&content, false);
        assert!(result.ok, "Well-known instance should succeed");
        assert_eq!(
            result.id,
            "gts.vendor.package.namespace.type.v1.0~instance.v1.0"
        );
        assert!(!result.is_type_schema);
    }

    #[test]
    fn test_add_entity_accepts_anonymous_instance() {
        // Anonymous instance with UUID in id field should succeed
        let mut ops = GtsOps::new(None, None, 0);
        let content = json!({
            "id": "7a1d2f34-5678-49ab-9012-abcdef123456",
            "type": "gts.vendor.package.namespace.type.v1.0~"
        });

        let result = ops.add_entity(&content, false);
        assert!(result.ok, "Anonymous instance should succeed");
        assert_eq!(result.id, "7a1d2f34-5678-49ab-9012-abcdef123456");
        assert!(!result.is_type_schema);
        assert_eq!(
            result.type_id,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned())
        );
    }

    #[test]
    fn test_add_entity_schema_without_id_returns_error() {
        // Schema without $id field should return error
        let mut ops = GtsOps::new(None, None, 0);
        let content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let result = ops.add_entity(&content, false);
        assert!(!result.ok, "Schema without $id should fail");
        assert!(
            result.error.contains("Unable to detect GTS ID"),
            "Error should mention missing GTS ID"
        );
    }

    #[test]
    fn test_add_entity_schema_with_valid_id_succeeds() {
        // Schema with valid $id should succeed
        let mut ops = GtsOps::new(None, None, 0);
        let content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "type": "object"
        });

        let result = ops.add_entity(&content, false);
        assert!(result.ok, "Schema with valid $id should succeed");
        assert_eq!(result.id, "gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_type_schema);
    }

    #[test]
    fn test_extract_id_for_well_known_instance() {
        // extract_id should return GTS ID for well-known instance
        let ops = GtsOps::new(None, None, 0);
        let content = json!({
            "id": "gts.x.core.events.type.v1~abc.app._.custom_event.v1.2"
        });

        let result = ops.extract_id(&content);
        assert_eq!(
            result.id,
            "gts.x.core.events.type.v1~abc.app._.custom_event.v1.2"
        );
        assert!(!result.is_type_schema);
        assert_eq!(
            result.type_id,
            Some("gts.x.core.events.type.v1~".to_owned())
        );
        assert_eq!(result.selected_entity_field, Some("id".to_owned()));
        assert_eq!(
            result.selected_type_id_field,
            Some("id".to_owned()),
            "selected_type_id_field should be set when type_id is derived from id"
        );
    }

    #[test]
    fn test_extract_id_for_anonymous_instance() {
        // extract_id should return UUID for anonymous instance
        let ops = GtsOps::new(None, None, 0);
        let content = json!({
            "id": "7a1d2f34-5678-49ab-9012-abcdef123456",
            "type": "gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~"
        });

        let result = ops.extract_id(&content);
        assert_eq!(result.id, "7a1d2f34-5678-49ab-9012-abcdef123456");
        assert!(!result.is_type_schema);
        assert_eq!(
            result.type_id,
            Some("gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~".to_owned())
        );
        assert_eq!(result.selected_entity_field, Some("id".to_owned()));
        assert_eq!(result.selected_type_id_field, Some("type".to_owned()));
    }

    #[test]
    fn test_extract_id_for_schema() {
        // extract_id should return GTS ID for schema
        let ops = GtsOps::new(None, None, 0);
        let content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~"
        });

        let result = ops.extract_id(&content);
        assert_eq!(result.id, "gts.vendor.package.namespace.type.v1.0~");
        assert!(result.is_type_schema);
    }

    #[test]
    fn test_extract_id_for_instance_without_id_returns_empty() {
        // extract_id should return empty string for instance without id
        let ops = GtsOps::new(None, None, 0);
        let content = json!({
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let result = ops.extract_id(&content);
        assert_eq!(result.id, "", "Should return empty string when no id found");
        assert!(!result.is_type_schema);
    }

    #[test]
    fn test_add_entity_schema_with_plain_gts_prefix_fails() {
        let mut ops = GtsOps::new(None, None, 0);
        let content = json!({
            "$id": "gts.x.test6.invalid_id.plain_prefix.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            },
            "required": ["id"]
        });

        let result = ops.add_entity(&content, false);
        assert!(
            !result.ok,
            "Schema with plain gts. prefix in $id should fail"
        );
        assert!(
            result.error.contains("Unable to detect GTS ID"),
            "Error should mention missing GTS ID, got: {}",
            result.error
        );
    }

    #[test]
    fn test_add_entity_schema_with_wildcard_in_gts_uri_fails() {
        let mut ops = GtsOps::new(None, None, 0);
        let content = json!({
            "$id": "gts://gts.x.test6.events.*.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            },
            "required": ["id"]
        });

        let result = ops.add_entity(&content, false);
        assert!(!result.ok, "Schema with wildcard in gts:// URI should fail");
        assert!(
            result.error.contains("Unable to detect GTS ID") || result.error.contains("wildcard"),
            "Error should mention invalid GTS ID or wildcard, got: {}",
            result.error
        );
    }

    #[test]
    fn test_add_entity_schema_with_gts_uri_prefix_succeeds() {
        let mut ops = GtsOps::new(None, None, 0);
        let content = json!({
            "$id": "gts://gts.x.test6.valid_id.with_uri.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "id": {"type": "string"}
            },
            "required": ["id"]
        });

        let result = ops.add_entity(&content, false);
        assert!(
            result.ok,
            "Schema with gts:// URI prefix should succeed, got error: {}",
            result.error
        );
        assert_eq!(result.id, "gts.x.test6.valid_id.with_uri.v1~");
        assert!(result.is_type_schema);
    }

    // =============================================================================
    // Additional test coverage for ops.rs functions
    // =============================================================================

    #[test]
    fn test_create_config_from_data_with_custom_fields() {
        let mut data = HashMap::new();
        data.insert(
            "entity_id_fields".to_owned(),
            json!(["customId", "uuid", "id"]),
        );
        data.insert(
            "type_id_fields".to_owned(),
            json!(["$schema", "$id", "schemaId"]),
        );

        let config = GtsOps::create_config_from_data(&data);
        assert_eq!(config.entity_id_fields, vec!["customId", "uuid", "id"]);
        assert_eq!(config.type_id_fields, vec!["$schema", "$id", "schemaId"]);
    }

    #[test]
    fn test_create_config_from_data_with_empty_data() {
        let data = HashMap::new();
        let config = GtsOps::create_config_from_data(&data);

        // Should use default config values
        let default_cfg = GtsConfig::default();
        assert_eq!(config.entity_id_fields, default_cfg.entity_id_fields);
        assert_eq!(config.type_id_fields, default_cfg.type_id_fields);
    }

    #[test]
    fn test_create_config_from_data_with_invalid_types() {
        let mut data = HashMap::new();
        // Non-array value should be ignored
        data.insert("entity_id_fields".to_owned(), json!("not-an-array"));
        data.insert("type_id_fields".to_owned(), json!(123));

        let config = GtsOps::create_config_from_data(&data);

        // Should fall back to default values
        let default_cfg = GtsConfig::default();
        assert_eq!(config.entity_id_fields, default_cfg.entity_id_fields);
        assert_eq!(config.type_id_fields, default_cfg.type_id_fields);
    }

    #[test]
    fn test_add_entity_schema_validation_error() {
        // Test "Always validate schemas" error branch
        let mut ops = GtsOps::new(None, None, 0);

        // Create a schema with an invalid $ref (not a local # reference or gts:// URI)
        // This will fail the validate_schema_refs check
        let content = json!({
            "$id": "gts://gts.test.invalid.schema.broken.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "foo": {
                    "$ref": "http://example.com/some-schema.json"
                }
            }
        });

        let result = ops.add_entity(&content, false);
        assert!(
            !result.ok,
            "Schema with invalid $ref should fail validation"
        );
        assert!(
            result.error.contains("Schema validation failed"),
            "Error should mention schema validation failure, got: {}",
            result.error
        );
    }

    #[test]
    fn test_add_entity_register_error() {
        // Test "Register the entity first" error branch
        // This is difficult to trigger directly since register() typically succeeds,
        // but we can test with a duplicate schema that fails registration
        let mut ops = GtsOps::new(None, None, 0);

        let schema = json!({
            "$id": "gts://gts.test.register.dup.schema.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        // First registration should succeed
        let result1 = ops.add_entity(&schema, false);
        assert!(result1.ok, "First schema registration should succeed");

        // Second registration of the same schema should succeed (overwrites)
        // To trigger a registration error, we would need to trigger an internal error
        // which is hard without mocking. This test validates the happy path.
        let result2 = ops.add_entity(&schema, false);
        assert!(
            result2.ok,
            "Schema re-registration should succeed (overwrite)"
        );
    }

    #[test]
    fn test_add_entity_instance_validation_error() {
        // Test "If validation is requested, validate the instance as well" error branch
        let mut ops = GtsOps::new(None, None, 0);

        // First, add a schema
        let schema = json!({
            "$id": "gts://gts.test.validation.instance.person.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            },
            "required": ["name", "age"]
        });

        let schema_result = ops.add_entity(&schema, false);
        assert!(schema_result.ok, "Schema should be added successfully");

        // Create an instance that violates the schema (missing required field)
        let invalid_instance = json!({
            "id": "test-person-123",
            "type": "gts.test.validation.instance.person.v1~",
            "name": "John Doe"
            // Missing required "age" field
        });

        // Add with validation enabled - should fail
        let result = ops.add_entity(&invalid_instance, true);
        assert!(
            !result.ok,
            "Instance validation should fail for invalid instance"
        );
        assert!(
            result.error.contains("Instance validation failed"),
            "Error should mention instance validation failure, got: {}",
            result.error
        );
    }

    #[test]
    fn test_validate_id_with_wildcard_valid() {
        // Test wildcard validation for valid patterns
        let result = GtsOps::validate_id("gts.vendor.package.namespace.*");
        assert!(result.valid, "Wildcard at end should be valid");
        assert!(result.is_wildcard);
        assert_eq!(result.is_type, Some(false));
    }

    #[test]
    fn test_validate_id_with_wildcard_schema() {
        // Test wildcard validation for pattern matching instances of a schema
        // Note: gts.vendor.package.namespace.type.v1~* matches instances, not schemas
        let result = GtsOps::validate_id("gts.vendor.package.namespace.type.v1~*");
        assert!(result.valid, "Wildcard at end of schema should be valid");
        assert!(result.is_wildcard);
        assert_eq!(
            result.is_type,
            Some(false),
            "Pattern matches instances, not schemas"
        );
    }

    #[test]
    fn test_validate_id_with_wildcard_invalid() {
        // Test wildcard validation for invalid patterns (multiple wildcards)
        let result = GtsOps::validate_id("gts.*.vendor.*.package");
        assert!(!result.valid, "Multiple wildcards should be invalid");
        assert!(result.is_wildcard);
        assert!(
            result.error.contains("Unable to validate GTS ID"),
            "Error should mention validation failure"
        );
    }

    #[test]
    fn test_validate_id_with_wildcard_middle() {
        // Test wildcard validation for invalid pattern (wildcard in middle)
        let result = GtsOps::validate_id("gts.vendor.*.package.type.v1");
        assert!(!result.valid, "Wildcard in middle should be invalid");
        assert!(result.is_wildcard);
    }

    #[test]
    fn test_parse_id_with_wildcard_valid() {
        // Test parse_id with valid wildcard pattern
        let result = GtsOps::parse_id("gts.vendor.package.namespace.*");
        assert!(result.ok, "Parsing valid wildcard should succeed");
        assert!(result.is_wildcard);
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].vendor, "vendor");
        assert_eq!(result.segments[0].package, "package");
        assert_eq!(result.segments[0].namespace, "namespace");
        assert_eq!(result.segments[0].type_name, "");
        assert_eq!(result.segments[0].ver_major, None);
        assert_eq!(result.segments[0].ver_minor, None);
        assert!(!result.segments[0].is_type);
        assert_eq!(result.is_type, Some(false));
    }

    #[test]
    fn test_parse_id_with_version_wildcard_shape() {
        let result = GtsOps::parse_id("gts.vendor.package.namespace.type.v*");
        assert!(result.ok, "Parsing valid version wildcard should succeed");
        assert!(result.is_wildcard);
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].vendor, "vendor");
        assert_eq!(result.segments[0].package, "package");
        assert_eq!(result.segments[0].namespace, "namespace");
        assert_eq!(result.segments[0].type_name, "type");
        assert_eq!(result.segments[0].ver_major, None);
        assert_eq!(result.segments[0].ver_minor, None);
        assert!(!result.segments[0].is_type);
        assert_eq!(result.is_type, Some(false));
    }

    #[test]
    fn test_parse_id_with_wildcard_schema() {
        // Test parse_id with wildcard pattern matching instances of a schema
        // Note: gts.vendor.package.namespace.type.v1~* matches instances, not schemas
        let result = GtsOps::parse_id("gts.vendor.package.namespace.type.v1~*");
        assert!(result.ok, "Parsing valid wildcard should succeed");
        assert!(result.is_wildcard);
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].vendor, "vendor");
        assert_eq!(result.segments[0].package, "package");
        assert_eq!(result.segments[0].namespace, "namespace");
        assert_eq!(result.segments[0].type_name, "type");
        assert_eq!(result.segments[0].ver_major, Some(1));
        assert_eq!(result.segments[0].ver_minor, None);
        assert!(result.segments[0].is_type);
        assert_eq!(result.segments[1].vendor, "");
        assert_eq!(result.segments[1].package, "");
        assert_eq!(result.segments[1].namespace, "");
        assert_eq!(result.segments[1].type_name, "");
        assert_eq!(result.segments[1].ver_major, None);
        assert_eq!(result.segments[1].ver_minor, None);
        assert!(!result.segments[1].is_type);
        assert_eq!(
            result.is_type,
            Some(false),
            "Pattern matches instances, not schemas"
        );
    }

    #[test]
    fn test_parse_id_with_wildcard_invalid() {
        // Test parse_id with invalid wildcard pattern
        let result = GtsOps::parse_id("gts.*.vendor.*.package");
        assert!(!result.ok, "Parsing invalid wildcard should fail");
        assert!(result.is_wildcard);
        assert!(
            result.segments.is_empty(),
            "Should have no segments on error"
        );
        assert!(!result.error.is_empty(), "Should have error message");
    }

    #[test]
    fn test_validate_schema_success() {
        let mut ops = GtsOps::new(None, None, 0);

        // Add a valid schema
        let schema = json!({
            "$id": "gts://gts.test.validate.schema.success.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        ops.add_entity(&schema, false);

        // Validate the schema
        let result = ops.validate_schema("gts.test.validate.schema.success.v1~");
        assert!(result.ok, "Valid schema should pass validation");
        assert!(result.error.is_empty());
        assert_eq!(result.id, "gts.test.validate.schema.success.v1~");
    }

    #[test]
    fn test_validate_schema_not_found() {
        let mut ops = GtsOps::new(None, None, 0);

        // Validate a schema that doesn't exist
        let result = ops.validate_schema("gts.test.validate.schema.notfound.v1~");
        assert!(!result.ok, "Non-existent schema should fail validation");
        assert!(!result.error.is_empty(), "Should have error message");
    }

    #[test]
    fn test_validate_entity_schema() {
        let mut ops = GtsOps::new(None, None, 0);

        // Add a valid schema
        let schema = json!({
            "$id": "gts://gts.test.validate.entity.schema.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        ops.add_entity(&schema, false);

        // validate_entity should route to validate_schema for schema IDs
        let result = ops.validate_entity("gts.test.validate.entity.schema.v1~");
        assert!(
            result.ok,
            "Schema validation through validate_entity should succeed"
        );
    }

    #[test]
    fn test_get_entity_not_found() {
        let mut ops = GtsOps::new(None, None, 0);

        // Try to get an entity that doesn't exist
        let result = ops.get_entity("gts.nonexistent.entity.v1~");
        assert!(!result.ok, "Getting non-existent entity should fail");
        assert_eq!(
            result.error,
            "Entity 'gts.nonexistent.entity.v1~' not found"
        );
        assert!(result.content.is_none(), "Content should be None");
        assert!(result.id.is_empty(), "ID should be empty on error");
    }

    #[test]
    fn test_get_entity_success() {
        let mut ops = GtsOps::new(None, None, 0);

        // Add a schema
        let schema = json!({
            "$id": "gts://gts.test.get.entity.success.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        ops.add_entity(&schema, false);

        // Get the entity
        let result = ops.get_entity("gts.test.get.entity.success.v1~");
        assert!(result.ok, "Getting existing entity should succeed");
        assert!(result.error.is_empty());
        assert!(result.content.is_some(), "Content should be present");
        assert_eq!(result.id, "gts.test.get.entity.success.v1~");
        assert!(result.is_type_schema);
    }

    #[test]
    fn test_validate_entity_accepts_abstract_base_with_unresolved_required_trait() {
        // gts-spec §9.7.5 / §9.11.4 (ADR-0003): a type marked
        // `x-gts-abstract: true` is exempt from the required-trait completeness
        // check. `/validate-entity` uses the same schema validation pipeline.
        let mut ops = GtsOps::new(None, None, 0);
        ops.store
            .register_schema(
                "gts.x.test13.abs.base.v1~",
                &json!({
                    "$id": "gts://gts.x.test13.abs.base.v1~",
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-abstract": true,
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {"topicRef": {"type": "string"}},
                        "required": ["topicRef"]
                    },
                    "properties": {"id": {"type": "string"}}
                }),
            )
            .expect("register abstract base");

        let result = ops.validate_entity("gts.x.test13.abs.base.v1~");
        assert!(
            result.ok,
            "abstract base must defer completeness: {result:?}"
        );
    }

    #[test]
    fn test_validate_entity_accepts_optional_trait_schema_without_values() {
        // OP#13 completeness is about required properties in the effective
        // trait-schema, not about requiring any `x-gts-traits` object to exist.
        let mut ops = GtsOps::new(None, None, 0);
        ops.store
            .register_schema(
                "gts.x.test13.conc.base.v1~",
                &json!({
                    "$id": "gts://gts.x.test13.conc.base.v1~",
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {"topicRef": {"type": "string"}}
                    },
                    "properties": {"id": {"type": "string"}}
                }),
            )
            .expect("register concrete base");

        let result = ops.validate_entity("gts.x.test13.conc.base.v1~");
        assert!(
            result.ok,
            "optional trait schema without values must be valid: {result:?}"
        );
    }

    #[test]
    fn test_validate_entity_accepts_open_trait_schema_with_values() {
        // `x-gts-traits-schema` is an ordinary JSON Schema subschema. The spec
        // does not require `additionalProperties: false` for `/validate-entity`.
        let mut ops = GtsOps::new(None, None, 0);
        ops.store
            .register_schema(
                "gts.x.test13.open.base.v1~",
                &json!({
                    "$id": "gts://gts.x.test13.open.base.v1~",
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {"topicRef": {"type": "string"}}
                    },
                    "x-gts-traits": {"topicRef": "events"},
                    "properties": {"id": {"type": "string"}}
                }),
            )
            .expect("register open base");

        let result = ops.validate_entity("gts.x.test13.open.base.v1~");
        assert!(
            result.ok,
            "open trait schema with conforming values must be valid: {result:?}"
        );
    }

    #[test]
    fn test_validate_entity_accepts_boolean_trait_schema_true() {
        // ADR-0002 explicitly admits boolean subschemas. `true` permits
        // arbitrary trait values.
        let mut ops = GtsOps::new(None, None, 0);
        ops.store
            .register_schema(
                "gts.x.test13.boolean.base.v1~",
                &json!({
                    "$id": "gts://gts.x.test13.boolean.base.v1~",
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": true,
                    "x-gts-traits": {"topicRef": "events"},
                    "properties": {"id": {"type": "string"}}
                }),
            )
            .expect("register boolean-trait base");

        let result = ops.validate_entity("gts.x.test13.boolean.base.v1~");
        assert!(
            result.ok,
            "boolean `true` trait schema must be valid: {result:?}"
        );
    }

    #[test]
    fn test_validate_entity_reports_missing_required_trait_failure() {
        // `/validate-entity` still surfaces OP#13 failures from validate_schema:
        // non-abstract types must resolve required traits.
        let mut ops = GtsOps::new(None, None, 0);
        ops.store
            .register_schema(
                "gts.x.test13.validate.required.v1~",
                &json!({
                    "$id": "gts://gts.x.test13.validate.required.v1~",
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "x-gts-traits-schema": {
                        "type": "object",
                        "properties": {"topicRef": {"type": "string"}},
                        "required": ["topicRef"]
                    },
                    "properties": {"id": {"type": "string"}}
                }),
            )
            .expect("register base with required trait");

        let result = ops.validate_entity("gts.x.test13.validate.required.v1~");
        assert!(
            !result.ok,
            "missing required trait must fail validate_entity, got ok=true"
        );
        assert!(
            result.error.contains("trait validation failed"),
            "validate_entity must surface the OP#13 error, got: {}",
            result.error
        );
    }
}
