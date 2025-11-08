use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::gts::GtsID;
use crate::path_resolver::JsonPathResolver;
use crate::schema_cast::{GtsEntityCastResult, SchemaCastError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    #[serde(rename = "instancePath")]
    pub instance_path: String,
    #[serde(rename = "schemaPath")]
    pub schema_path: String,
    pub keyword: String,
    pub message: String,
    pub params: HashMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
}

#[derive(Debug, Clone)]
pub struct GtsFile {
    pub path: String,
    pub name: String,
    pub content: Value,
    pub sequences_count: usize,
    pub sequence_content: HashMap<usize, Value>,
    pub validation: ValidationResult,
}

impl GtsFile {
    pub fn new(path: String, name: String, content: Value) -> Self {
        let mut sequences_count = 0;
        let mut sequence_content = HashMap::new();

        let items = if content.is_array() {
            content.as_array().unwrap().clone()
        } else {
            vec![content.clone()]
        };

        for (i, item) in items.iter().enumerate() {
            sequences_count += 1;
            sequence_content.insert(i, item.clone());
        }

        GtsFile {
            path,
            name,
            content,
            sequences_count,
            sequence_content,
            validation: ValidationResult::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsConfig {
    pub entity_id_fields: Vec<String>,
    pub schema_id_fields: Vec<String>,
}

impl Default for GtsConfig {
    fn default() -> Self {
        GtsConfig {
            entity_id_fields: vec![
                "$id".to_string(),
                "gtsId".to_string(),
                "gtsIid".to_string(),
                "gtsOid".to_string(),
                "gtsI".to_string(),
                "gts_id".to_string(),
                "gts_oid".to_string(),
                "gts_iid".to_string(),
                "id".to_string(),
            ],
            schema_id_fields: vec![
                "$schema".to_string(),
                "gtsTid".to_string(),
                "gtsType".to_string(),
                "gtsT".to_string(),
                "gts_t".to_string(),
                "gts_tid".to_string(),
                "gts_type".to_string(),
                "type".to_string(),
                "schema".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct GtsRef {
    pub id: String,
    pub source_path: String,
}

#[derive(Debug, Clone)]
pub struct GtsEntity {
    pub gts_id: Option<GtsID>,
    pub is_schema: bool,
    pub file: Option<GtsFile>,
    pub list_sequence: Option<usize>,
    pub label: String,
    pub content: Value,
    pub gts_refs: Vec<GtsRef>,
    pub validation: ValidationResult,
    pub schema_id: Option<String>,
    pub selected_entity_field: Option<String>,
    pub selected_schema_id_field: Option<String>,
    pub description: String,
    pub schema_refs: Vec<GtsRef>,
}

impl GtsEntity {
    pub fn new(
        file: Option<GtsFile>,
        list_sequence: Option<usize>,
        content: Value,
        cfg: Option<&GtsConfig>,
        gts_id: Option<GtsID>,
        is_schema: bool,
        label: String,
        validation: Option<ValidationResult>,
        schema_id: Option<String>,
    ) -> Self {
        let mut entity = GtsEntity {
            file,
            list_sequence,
            content: content.clone(),
            gts_id,
            is_schema,
            label,
            validation: validation.unwrap_or_default(),
            schema_id,
            selected_entity_field: None,
            selected_schema_id_field: None,
            gts_refs: Vec::new(),
            schema_refs: Vec::new(),
            description: String::new(),
        };

        // Auto-detect if this is a schema
        if entity.is_json_schema_entity() {
            entity.is_schema = true;
        }

        // Calculate IDs if config provided
        if let Some(cfg) = cfg {
            let idv = entity.calc_json_entity_id(cfg);
            entity.schema_id = entity.calc_json_schema_id(cfg);

            // If no valid GTS ID found in entity fields, use schema ID as fallback
            let mut final_id = idv;
            if final_id.is_none() || !GtsID::is_valid(final_id.as_ref().unwrap()) {
                if let Some(ref sid) = entity.schema_id {
                    if GtsID::is_valid(sid) {
                        final_id = Some(sid.clone());
                    }
                }
            }

            entity.gts_id = final_id.and_then(|id| GtsID::new(&id).ok());
        }

        // Set label
        if let Some(ref file) = entity.file {
            if entity.list_sequence.is_some() {
                entity.label = format!("{}#{}", file.name, entity.list_sequence.unwrap());
            } else {
                entity.label = file.name.clone();
            }
        } else if let Some(ref gts_id) = entity.gts_id {
            entity.label = gts_id.id.clone();
        } else if entity.label.is_empty() {
            entity.label = String::new();
        }

        // Extract description
        if let Some(obj) = content.as_object() {
            if let Some(desc) = obj.get("description") {
                if let Some(s) = desc.as_str() {
                    entity.description = s.to_string();
                }
            }
        }

        // Extract references
        entity.gts_refs = entity.extract_gts_ids_with_paths();
        if entity.is_schema {
            entity.schema_refs = entity.extract_ref_strings_with_paths();
        }

        entity
    }

    fn is_json_schema_entity(&self) -> bool {
        if let Some(obj) = self.content.as_object() {
            if let Some(url) = obj.get("$schema") {
                if let Some(url_str) = url.as_str() {
                    return url_str.starts_with("http://json-schema.org/")
                        || url_str.starts_with("https://json-schema.org/")
                        || url_str.starts_with("gts://")
                        || url_str.starts_with("gts.");
                }
            }
        }
        false
    }

    pub fn resolve_path(&self, path: &str) -> JsonPathResolver {
        let gts_id = self
            .gts_id
            .as_ref()
            .map(|g| g.id.clone())
            .unwrap_or_default();
        JsonPathResolver::new(gts_id, self.content.clone()).resolve(path)
    }

    pub fn cast(
        &self,
        to_schema: &GtsEntity,
        from_schema: &GtsEntity,
        resolver: Option<&()>,
    ) -> Result<GtsEntityCastResult, SchemaCastError> {
        if self.is_schema {
            // When casting a schema, from_schema might be a standard JSON Schema (no gts_id)
            if let (Some(ref self_id), Some(ref from_id)) = (&self.gts_id, &from_schema.gts_id) {
                if self_id.id != from_id.id {
                    return Err(SchemaCastError::InternalError(format!(
                        "Internal error: {} != {}",
                        self_id.id, from_id.id
                    )));
                }
            }
        }

        if !to_schema.is_schema {
            return Err(SchemaCastError::TargetMustBeSchema);
        }

        if !from_schema.is_schema {
            return Err(SchemaCastError::SourceMustBeSchema);
        }

        let from_id = self
            .gts_id
            .as_ref()
            .map(|g| g.id.clone())
            .unwrap_or_default();
        let to_id = to_schema
            .gts_id
            .as_ref()
            .map(|g| g.id.clone())
            .unwrap_or_default();

        GtsEntityCastResult::cast(
            &from_id,
            &to_id,
            &self.content,
            &from_schema.content,
            &to_schema.content,
            resolver,
        )
    }

    fn walk_and_collect<F>(&self, content: &Value, collector: &mut Vec<GtsRef>, matcher: F)
    where
        F: Fn(&Value, &str) -> Option<GtsRef> + Copy,
    {
        fn walk<F>(node: &Value, current_path: &str, collector: &mut Vec<GtsRef>, matcher: F)
        where
            F: Fn(&Value, &str) -> Option<GtsRef> + Copy,
        {
            // Try to match current node
            if let Some(match_result) = matcher(node, current_path) {
                collector.push(match_result);
            }

            // Recurse into structures
            match node {
                Value::Object(map) => {
                    for (k, v) in map {
                        let next_path = if current_path.is_empty() {
                            k.clone()
                        } else {
                            format!("{}.{}", current_path, k)
                        };
                        walk(v, &next_path, collector, matcher);
                    }
                }
                Value::Array(arr) => {
                    for (idx, item) in arr.iter().enumerate() {
                        let next_path = format!("{}[{}]", current_path, idx);
                        walk(item, &next_path, collector, matcher);
                    }
                }
                _ => {}
            }
        }

        walk(content, "", collector, matcher);
    }

    fn deduplicate_by_id_and_path(&self, items: Vec<GtsRef>) -> Vec<GtsRef> {
        let mut seen = HashMap::new();
        let mut result = Vec::new();

        for item in items {
            let key = format!("{}|{}", item.id, item.source_path);
            if !seen.contains_key(&key) {
                seen.insert(key, true);
                result.push(item);
            }
        }

        result
    }

    fn extract_gts_ids_with_paths(&self) -> Vec<GtsRef> {
        let mut found = Vec::new();

        let gts_id_matcher = |node: &Value, path: &str| -> Option<GtsRef> {
            if let Some(s) = node.as_str() {
                if GtsID::is_valid(s) {
                    return Some(GtsRef {
                        id: s.to_string(),
                        source_path: if path.is_empty() {
                            "root".to_string()
                        } else {
                            path.to_string()
                        },
                    });
                }
            }
            None
        };

        self.walk_and_collect(&self.content, &mut found, gts_id_matcher);
        self.deduplicate_by_id_and_path(found)
    }

    fn extract_ref_strings_with_paths(&self) -> Vec<GtsRef> {
        let mut refs = Vec::new();

        let ref_matcher = |node: &Value, path: &str| -> Option<GtsRef> {
            if let Some(obj) = node.as_object() {
                if let Some(ref_val) = obj.get("$ref") {
                    if let Some(ref_str) = ref_val.as_str() {
                        let ref_path = if path.is_empty() {
                            "$ref".to_string()
                        } else {
                            format!("{}.$ref", path)
                        };
                        return Some(GtsRef {
                            id: ref_str.to_string(),
                            source_path: ref_path,
                        });
                    }
                }
            }
            None
        };

        self.walk_and_collect(&self.content, &mut refs, ref_matcher);
        self.deduplicate_by_id_and_path(refs)
    }

    fn get_field_value(&self, field: &str) -> Option<String> {
        if let Some(obj) = self.content.as_object() {
            if let Some(v) = obj.get(field) {
                if let Some(s) = v.as_str() {
                    if !s.trim().is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
        None
    }

    fn first_non_empty_field(&mut self, fields: &[String]) -> Option<String> {
        // First pass: look for valid GTS IDs
        for f in fields {
            if let Some(v) = self.get_field_value(f) {
                if GtsID::is_valid(&v) {
                    self.selected_entity_field = Some(f.clone());
                    return Some(v);
                }
            }
        }

        // Second pass: any non-empty string
        for f in fields {
            if let Some(v) = self.get_field_value(f) {
                self.selected_entity_field = Some(f.clone());
                return Some(v);
            }
        }

        None
    }

    fn calc_json_entity_id(&mut self, cfg: &GtsConfig) -> Option<String> {
        if let Some(id) = self.first_non_empty_field(&cfg.entity_id_fields) {
            return Some(id);
        }

        if let Some(ref file) = self.file {
            if let Some(seq) = self.list_sequence {
                return Some(format!("{}#{}", file.path, seq));
            }
            return Some(file.path.clone());
        }

        None
    }

    fn calc_json_schema_id(&mut self, cfg: &GtsConfig) -> Option<String> {
        // First try schema-specific fields
        for f in &cfg.schema_id_fields {
            if let Some(v) = self.get_field_value(f) {
                self.selected_schema_id_field = Some(f.clone());
                return Some(v);
            }
        }

        // Fallback to entity ID logic
        let idv = self.first_non_empty_field(&cfg.entity_id_fields);
        if let Some(ref id) = idv {
            if GtsID::is_valid(id) {
                if id.ends_with('~') {
                    // Don't set selected_schema_id_field when the entity ID itself is a schema ID
                    return Some(id.clone());
                }
                if let Some(last) = id.rfind('~') {
                    // Only set selected_schema_id_field when extracting a substring
                    self.selected_schema_id_field = self.selected_entity_field.clone();
                    return Some(id[..=last].to_string());
                }
            }
        }

        if let Some(ref file) = self.file {
            if let Some(seq) = self.list_sequence {
                return Some(format!("{}#{}", file.path, seq));
            }
            return Some(file.path.clone());
        }

        None
    }
}
