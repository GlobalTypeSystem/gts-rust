use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::gts::{GtsID, GTS_URI_PREFIX};
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
    #[must_use]
    pub fn new(path: String, name: String, content: Value) -> Self {
        let sequence_content: HashMap<usize, Value> = if let Some(arr) = content.as_array() {
            arr.iter()
                .enumerate()
                .map(|(i, v)| (i, v.clone()))
                .collect()
        } else {
            [(0, content.clone())].into_iter().collect()
        };
        let sequences_count = sequence_content.len();

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
                "$id".to_owned(),
                "gtsId".to_owned(),
                "gtsIid".to_owned(),
                "gtsOid".to_owned(),
                "gtsI".to_owned(),
                "gts_id".to_owned(),
                "gts_oid".to_owned(),
                "gts_iid".to_owned(),
                "id".to_owned(),
            ],
            schema_id_fields: vec![
                "gtsTid".to_owned(),
                "gtsType".to_owned(),
                "gtsT".to_owned(),
                "gts_t".to_owned(),
                "gts_tid".to_owned(),
                "gts_type".to_owned(),
                "type".to_owned(),
                "schema".to_owned(),
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
    /// The GTS ID if the entity has one (either from `id` field for well-known instances,
    /// or from `$id` field for schemas). None for anonymous instances.
    pub gts_id: Option<GtsID>,
    /// The instance ID - for anonymous instances this is the UUID from `id` field,
    /// for well-known instances this equals `gts_id.id`, for schemas this equals `gts_id.id`.
    pub instance_id: Option<String>,
    /// True if this is a JSON Schema (has `$schema` field), false if it's an instance.
    pub is_schema: bool,
    pub file: Option<GtsFile>,
    pub list_sequence: Option<usize>,
    pub label: String,
    pub content: Value,
    pub gts_refs: Vec<GtsRef>,
    pub validation: ValidationResult,
    /// The schema ID that this entity conforms to:
    /// - For schemas: the `$schema` field value (e.g., `http://json-schema.org/draft-07/schema#`)
    ///   OR for GTS schemas, the parent schema from the chain
    /// - For instances: the `type` field value (the GTS type ID ending with `~`)
    pub schema_id: Option<String>,
    pub selected_entity_field: Option<String>,
    pub selected_schema_id_field: Option<String>,
    pub description: String,
    pub schema_refs: Vec<GtsRef>,
}

impl GtsEntity {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        file: Option<GtsFile>,
        list_sequence: Option<usize>,
        content: &Value,
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
            instance_id: None,
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

        // RULE: A JSON is a schema if and only if it has a "$schema" field
        // This is the PRIMARY check - $schema presence is the definitive marker
        entity.is_schema = entity.has_schema_field();

        // Calculate IDs if config provided
        if let Some(cfg) = cfg {
            if entity.is_schema {
                // For schemas: extract GTS ID from $id field
                entity.extract_schema_ids(cfg);
            } else {
                // For instances: extract instance_id and schema_id separately
                entity.extract_instance_ids(cfg);
            }
        }

        // Set label
        if let Some(ref file) = entity.file {
            if let Some(seq) = entity.list_sequence {
                entity.label = format!("{}#{seq}", file.name);
            } else {
                entity.label = file.name.clone();
            }
        } else if let Some(ref instance_id) = entity.instance_id {
            entity.label = instance_id.clone();
        } else if let Some(ref gts_id) = entity.gts_id {
            entity.label = gts_id.id.clone();
        } else if entity.label.is_empty() {
            entity.label = String::new();
        }

        // Extract description
        if let Some(obj) = content.as_object() {
            if let Some(desc) = obj.get("description") {
                if let Some(s) = desc.as_str() {
                    s.clone_into(&mut entity.description);
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

    /// Check if the JSON has a "$schema" field - this is the ONLY way to determine if it's a schema.
    /// Per GTS spec: "if json has "$schema" - it's a schema, always. Otherwise, it's instance, always!"
    fn has_schema_field(&self) -> bool {
        if let Some(obj) = self.content.as_object() {
            if let Some(schema_val) = obj.get("$schema") {
                if let Some(schema_str) = schema_val.as_str() {
                    return !schema_str.is_empty();
                }
            }
        }
        false
    }

    /// Extract IDs for a schema entity.
    /// - `gts_id`: from `$id` field (must be `gts://` URI with GTS ID)
    /// - `schema_id`: the parent schema (from `$schema` field or extracted from chain)
    /// - `instance_id`: same as `gts_id` for schemas
    fn extract_schema_ids(&mut self, cfg: &GtsConfig) {
        // Extract GTS ID from $id field
        if let Some(obj) = self.content.as_object() {
            if let Some(id_val) = obj.get("$id") {
                if let Some(id_str) = id_val.as_str() {
                    let normalized = id_str.strip_prefix(GTS_URI_PREFIX).unwrap_or(id_str).trim();
                    if GtsID::is_valid(normalized) {
                        self.gts_id = GtsID::new(normalized).ok();
                        self.instance_id = Some(normalized.to_owned());
                        self.selected_entity_field = Some("$id".to_owned());
                    }
                }
            }

            // For schemas, schema_id is the $schema field value
            // OR for GTS schemas with chains, it's the parent type
            if let Some(schema_val) = obj.get("$schema") {
                if let Some(schema_str) = schema_val.as_str() {
                    self.schema_id = Some(schema_str.to_owned());
                    self.selected_schema_id_field = Some("$schema".to_owned());
                }
            }

            // For chained GTS IDs, extract the parent schema from the chain
            if let Some(ref gts_id) = self.gts_id {
                if gts_id.gts_id_segments.len() > 1 {
                    // Build parent schema ID from all segments except the last
                    // Each segment.segment already includes the ~ suffix if it's a type
                    let parent_segments: Vec<&str> = gts_id
                        .gts_id_segments
                        .iter()
                        .take(gts_id.gts_id_segments.len() - 1)
                        .map(|seg| seg.segment.as_str())
                        .collect();
                    if !parent_segments.is_empty() {
                        // Join segments - they already have ~ at the end if they're types
                        // The full chain format is: gts.seg1~seg2~seg3~
                        // For parent, we want: gts.seg1~ (if only one parent segment)
                        // or gts.seg1~seg2~ (if multiple parent segments)
                        let parent_id = format!("gts.{}", parent_segments.join("~"));
                        // Ensure it ends with ~ (parent is always a schema)
                        let parent_id = if parent_id.ends_with('~') {
                            parent_id
                        } else {
                            format!("{parent_id}~")
                        };
                        // Use parent as schema_id if $schema is a standard JSON Schema URL
                        if self
                            .schema_id
                            .as_ref()
                            .is_some_and(|s| s.starts_with("http"))
                        {
                            self.schema_id = Some(parent_id);
                        }
                    }
                }
            }
        }

        // Fallback to old logic for entity_id_fields if $id not found
        if self.gts_id.is_none() {
            let idv = self.calc_json_entity_id_legacy(cfg);
            if let Some(ref id) = idv {
                if GtsID::is_valid(id) {
                    self.gts_id = GtsID::new(id).ok();
                    self.instance_id = Some(id.clone());
                }
            }
        }
    }

    /// Extract IDs for an instance entity.
    /// There are two types of instances:
    /// 1. Well-known instances: id field contains a GTS ID (e.g., gts.x.core.events.topic.v1~x.commerce._.orders.v1.0)
    /// 2. Anonymous instances: id field contains a UUID, type field contains the GTS schema ID
    ///
    /// For `schema_id` resolution, explicit `type` field takes priority over the chain-derived schema.
    /// This allows overriding the implicit parent schema from a chained ID.
    fn extract_instance_ids(&mut self, cfg: &GtsConfig) {
        // Only process if content is an object
        if self.content.as_object().is_none() {
            return;
        }

        // First, try to get the id field value (could be UUID or GTS ID)
        let id_value = self.get_id_field_value(cfg);

        // Check if id is a valid GTS ID (well-known instance)
        if let Some(ref id) = id_value {
            if GtsID::is_valid(id) {
                // Well-known instance: id IS the GTS ID
                self.gts_id = GtsID::new(id).ok();
                self.instance_id = Some(id.clone());

                // PRIORITY 1: Extract schema from chained ID (always takes priority)
                // For well-known instances with CHAINED IDs (multiple segments),
                // extract schema from the chain. A chained ID has more than one
                // segment.
                // Example: gts.x.core.events.type.v1~abc.app._.custom_event.v1.2
                //          has 2 segments, so schema_id = gts.x.core.events.type.v1~
                // But: gts.v123.p456.n789.t000.v999.888~ has only 1 segment,
                //      so we can't determine its schema (it IS a schema ID)
                if let Some(ref gts_id) = self.gts_id {
                    // Only extract schema_id if there are multiple segments
                    if gts_id.gts_id_segments.len() > 1 {
                        // Extract schema ID: everything up to and including last ~
                        // For a 2-segment chain, this gives first segment (parent)
                        if let Some(last_tilde) = gts_id.id.rfind('~') {
                            self.schema_id = Some(gts_id.id[..=last_tilde].to_string());
                            // Mark that schema_id was extracted from the id field
                            self.selected_schema_id_field = self.selected_entity_field.clone();
                        }
                    }
                }
            } else {
                // Anonymous instance: id is a UUID or other non-GTS identifier
                self.instance_id = Some(id.clone());
                self.gts_id = None; // Anonymous instances don't have a GTS ID
            }
        }

        // PRIORITY 2: Fall back to explicit type field (only if no chain-derived)
        // For anonymous instances or well-known instances without chained IDs,
        // check for explicit type/gtsTid fields.
        if self.schema_id.is_none() {
            self.schema_id = self.get_type_field_value(cfg);
        }

        // If still no instance_id, fall back to file path
        if self.instance_id.is_none() {
            if let Some(ref file) = self.file {
                if let Some(seq) = self.list_sequence {
                    self.instance_id = Some(format!("{}#{}", file.path, seq));
                } else {
                    self.instance_id = Some(file.path.clone());
                }
            }
        }
    }

    /// Get the id field value from `entity_id_fields` config
    fn get_id_field_value(&mut self, cfg: &GtsConfig) -> Option<String> {
        for f in &cfg.entity_id_fields {
            // Skip $schema and type fields - they're not entity IDs
            if f == "$schema" || f == "type" {
                continue;
            }
            if let Some(v) = self.get_field_value(f) {
                self.selected_entity_field = Some(f.clone());
                return Some(v);
            }
        }
        None
    }

    /// Get the type/schema field value from `schema_id_fields` config
    fn get_type_field_value(&mut self, cfg: &GtsConfig) -> Option<String> {
        for f in &cfg.schema_id_fields {
            // Skip $schema for instances - it's not a valid field for instances
            if f == "$schema" {
                continue;
            }
            if let Some(v) = self.get_field_value(f) {
                // Only accept valid GTS type IDs (ending with ~)
                if GtsID::is_valid(&v) && v.ends_with('~') {
                    self.selected_schema_id_field = Some(f.clone());
                    return Some(v);
                }
            }
        }
        None
    }

    /// Legacy method for backwards compatibility
    fn calc_json_entity_id_legacy(&mut self, cfg: &GtsConfig) -> Option<String> {
        self.first_non_empty_field(&cfg.entity_id_fields)
    }

    #[must_use]
    pub fn resolve_path(&self, path: &str) -> JsonPathResolver {
        let gts_id = self
            .gts_id
            .as_ref()
            .map(|g| g.id.clone())
            .unwrap_or_default();
        JsonPathResolver::new(gts_id, self.content.clone()).resolve(path)
    }

    /// Casts this entity to a different schema.
    ///
    /// # Errors
    /// Returns `SchemaCastError` if the cast fails.
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

    fn walk_and_collect<F>(content: &Value, collector: &mut Vec<GtsRef>, matcher: F)
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
                            format!("{current_path}.{k}")
                        };
                        walk(v, &next_path, collector, matcher);
                    }
                }
                Value::Array(arr) => {
                    for (idx, item) in arr.iter().enumerate() {
                        let next_path = format!("{current_path}[{idx}]");
                        walk(item, &next_path, collector, matcher);
                    }
                }
                _ => {}
            }
        }

        walk(content, "", collector, matcher);
    }

    fn deduplicate_by_id_and_path(items: Vec<GtsRef>) -> Vec<GtsRef> {
        let mut seen = HashMap::new();
        let mut result = Vec::new();

        for item in items {
            let key = format!("{}|{}", item.id, item.source_path);
            if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                e.insert(true);
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
                        id: s.to_owned(),
                        source_path: if path.is_empty() {
                            "root".to_owned()
                        } else {
                            path.to_owned()
                        },
                    });
                }
            }
            None
        };

        Self::walk_and_collect(&self.content, &mut found, gts_id_matcher);
        Self::deduplicate_by_id_and_path(found)
    }

    fn extract_ref_strings_with_paths(&self) -> Vec<GtsRef> {
        let mut refs = Vec::new();

        let ref_matcher = |node: &Value, path: &str| -> Option<GtsRef> {
            if let Some(obj) = node.as_object() {
                if let Some(ref_val) = obj.get("$ref") {
                    if let Some(ref_str) = ref_val.as_str() {
                        let ref_path = if path.is_empty() {
                            "$ref".to_owned()
                        } else {
                            format!("{path}.$ref")
                        };
                        // Normalize: strip gts:// prefix for canonical GTS ID storage
                        let normalized_ref = ref_str
                            .strip_prefix(GTS_URI_PREFIX)
                            .unwrap_or(ref_str)
                            .to_owned();
                        return Some(GtsRef {
                            id: normalized_ref,
                            source_path: ref_path,
                        });
                    }
                }
            }
            None
        };

        Self::walk_and_collect(&self.content, &mut refs, ref_matcher);
        Self::deduplicate_by_id_and_path(refs)
    }

    fn get_field_value(&self, field: &str) -> Option<String> {
        if let Some(obj) = self.content.as_object() {
            if let Some(v) = obj.get(field) {
                if let Some(s) = v.as_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        // Strip the "gts://" URI prefix ONLY for $id field (JSON Schema compatibility)
                        // The gts:// prefix is ONLY valid in the $id field of JSON Schema
                        let normalized = if field == "$id" {
                            trimmed.strip_prefix(GTS_URI_PREFIX).unwrap_or(trimmed)
                        } else {
                            trimmed
                        };
                        return Some(normalized.to_owned());
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

    /// Returns the effective ID for this entity (for store indexing and CLI output).
    /// - For schemas: the GTS ID from `$id` field
    /// - For well-known instances: the GTS ID from `id` field
    /// - For anonymous instances: the `instance_id` (UUID or other non-GTS identifier)
    #[must_use]
    pub fn effective_id(&self) -> Option<String> {
        // Prefer GTS ID if available
        if let Some(ref gts_id) = self.gts_id {
            return Some(gts_id.id.clone());
        }
        // Fall back to instance_id for anonymous instances
        self.instance_id.clone()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_file_with_description() {
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0",
            "description": "Test description"
        });

        let cfg = GtsConfig::default();
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

        assert_eq!(entity.description, "Test description");
    }

    #[test]
    fn test_json_entity_with_file_and_sequence() {
        let file_content = json!([
            {"id": "gts.vendor.package.namespace.type.v1.0"},
            {"id": "gts.vendor.package.namespace.type.v1.1"}
        ]);

        let file = GtsFile::new(
            "/path/to/file.json".to_owned(),
            "file.json".to_owned(),
            file_content,
        );

        let entity_content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});
        let cfg = GtsConfig::default();

        let entity = GtsEntity::new(
            Some(file),
            Some(0),
            &entity_content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert_eq!(entity.label, "file.json#0");
    }

    #[test]
    fn test_json_entity_with_file_no_sequence() {
        let file_content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});

        let file = GtsFile::new(
            "/path/to/file.json".to_owned(),
            "file.json".to_owned(),
            file_content,
        );

        let entity_content = json!({"id": "gts.vendor.package.namespace.type.v1.0"});
        let cfg = GtsConfig::default();

        let entity = GtsEntity::new(
            Some(file),
            None,
            &entity_content,
            Some(&cfg),
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert_eq!(entity.label, "file.json");
    }

    #[test]
    fn test_json_entity_extract_gts_ids() {
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~a.b.c.d.v1",
            "nested": {
                "ref": "gts.other.package.namespace.type.v2.0~e.f.g.h.v2"
            }
        });

        let cfg = GtsConfig::default();
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

        // gts_refs is populated during entity construction
        assert!(!entity.gts_refs.is_empty());
    }

    #[test]
    fn test_json_entity_extract_ref_strings() {
        let content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$ref": "gts://gts.vendor.package.namespace.type.v1.0~",
            "properties": {
                "user": {
                    "$ref": "gts://gts.other.package.namespace.type.v2.0~"
                }
            }
        });

        let cfg = GtsConfig::default();
        let entity = GtsEntity::new(
            None,
            None,
            &content,
            Some(&cfg),
            None,
            false, // Will be auto-detected as schema due to $schema field
            String::new(),
            None,
            None,
        );

        // Entity should be detected as schema due to $schema field
        assert!(entity.is_schema);
        // schema_refs is populated during entity construction for schemas
        assert!(!entity.schema_refs.is_empty());
    }

    #[test]
    fn test_json_entity_is_json_schema_entity() {
        let schema_content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let entity = GtsEntity::new(
            None,
            None,
            &schema_content,
            None,
            None,
            false,
            String::new(),
            None,
            None,
        );

        assert!(entity.is_schema);
    }

    #[test]
    fn test_json_entity_instance_with_type_field() {
        // An instance with only a "type" field (no "id" field) should:
        // - Have schema_id set from the type field
        // - NOT have gts_id (because there's no entity ID)
        // - Be marked as instance (not schema)
        let content = json!({
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let cfg = GtsConfig::default();
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

        // No $schema field means it's an instance
        assert!(!entity.is_schema);
        // The type field provides the schema_id
        assert_eq!(
            entity.schema_id,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned())
        );
        // No id field means no gts_id (this is an anonymous instance without an id)
        assert!(entity.gts_id.is_none());
        assert!(entity.instance_id.is_none());
    }

    #[test]
    fn test_json_entity_with_custom_label() {
        let content = json!({"name": "test"});

        let entity = GtsEntity::new(
            None,
            None,
            &content,
            None,
            None,
            false,
            "custom_label".to_owned(),
            None,
            None,
        );

        assert_eq!(entity.label, "custom_label");
    }

    #[test]
    fn test_json_entity_empty_label_fallback() {
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

        assert_eq!(entity.label, "");
    }

    #[test]
    fn test_validation_result_default() {
        let result = ValidationResult::default();
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validation_error_creation() {
        let mut params = std::collections::HashMap::new();
        params.insert("key".to_owned(), json!("value"));

        let error = ValidationError {
            instance_path: "/path".to_owned(),
            schema_path: "/schema".to_owned(),
            keyword: "required".to_owned(),
            message: "test error".to_owned(),
            params,
            data: Some(json!({"test": "data"})),
        };

        assert_eq!(error.instance_path, "/path");
        assert_eq!(error.message, "test error");
        assert!(error.data.is_some());
    }

    #[test]
    fn test_gts_config_entity_id_fields() {
        let cfg = GtsConfig::default();
        assert!(cfg.entity_id_fields.contains(&"id".to_owned()));
        assert!(cfg.entity_id_fields.contains(&"$id".to_owned()));
        assert!(cfg.entity_id_fields.contains(&"gtsId".to_owned()));
    }

    #[test]
    fn test_gts_config_schema_id_fields() {
        let cfg = GtsConfig::default();
        assert!(cfg.schema_id_fields.contains(&"type".to_owned()));
        assert!(cfg.schema_id_fields.contains(&"schema".to_owned()));
        assert!(cfg.schema_id_fields.contains(&"gtsTid".to_owned()));
    }

    #[test]
    fn test_json_entity_with_validation_result() {
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
            None,
            None,
            false,
            String::new(),
            Some(validation.clone()),
            None,
        );

        assert_eq!(entity.validation.errors.len(), 1);
    }

    #[test]
    fn test_json_entity_schema_id_field_selection() {
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~instance.v1.0",
            "type": "gts.vendor.package.namespace.type.v1.0~"
        });

        let cfg = GtsConfig::default();
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

        assert!(entity.selected_schema_id_field.is_some());
    }

    #[test]
    fn test_json_entity_when_id_is_schema() {
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#"
        });

        let cfg = GtsConfig::default();
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

        // When entity ID itself is a schema, selected_schema_id_field should be set to $schema
        assert_eq!(entity.selected_schema_id_field, Some("$schema".to_owned()));
    }

    // =============================================================================
    // Tests for URI prefix "gts:" in JSON Schema $id field
    // The gts: prefix is used in JSON Schema for URI compatibility.
    // GtsEntity strips it when parsing so the GtsID works with normal "gts." format.
    // =============================================================================

    #[test]
    fn test_entity_with_gts_uri_prefix_in_id() {
        // Test that the "gts://" prefix is stripped from JSON Schema $id field
        let content = json!({
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        });

        let cfg = GtsConfig::default();
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

        // The gts_id should have the prefix stripped
        let gts_id = entity.gts_id.as_ref().expect("Entity should have a GTS ID");
        assert_eq!(gts_id.id, "gts.vendor.package.namespace.type.v1.0~");
        assert!(entity.is_schema, "Entity should be detected as a schema");
    }

    #[test]
    fn test_entity_schema_id_extraction() {
        // Test that schema_id is correctly extracted from the "type" field
        // Note: The instance segment must be a valid GTS segment (vendor.package.namespace.type.version)
        // The gts: prefix is ONLY used in $id field, NOT in id/type fields
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1~other.app.data.item.v1.0",
            "type": "gts.vendor.package.namespace.type.v1~"
        });

        let cfg = GtsConfig::default();
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

        let gts_id = entity.gts_id.as_ref().expect("Entity should have a GTS ID");
        assert_eq!(
            gts_id.id,
            "gts.vendor.package.namespace.type.v1~other.app.data.item.v1.0"
        );

        let schema_id = entity
            .schema_id
            .as_ref()
            .expect("Entity should have a schema ID");
        assert_eq!(schema_id, "gts.vendor.package.namespace.type.v1~");
    }

    #[test]
    fn test_is_json_schema_with_standard_schema() {
        // Test that entities with standard $schema URLs are detected as schemas
        let content = json!({
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#"
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

        assert!(
            entity.is_schema,
            "Entity with $schema should be detected as schema"
        );
    }

    #[test]
    fn test_gts_colon_prefix_not_valid_in_id_field() {
        // "gts:" (without //) is NOT a valid prefix - only "gts://" is valid
        // When $id has "gts:" prefix (not "gts://"), it should NOT be stripped
        let content = json!({
            "$id": "gts:gts.vendor.package.namespace.type.v1.0~",
            "$schema": "http://json-schema.org/draft-07/schema#"
        });

        let cfg = GtsConfig::default();
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

        // With "gts:" prefix (not "gts://"), the ID is not stripped and won't be valid
        // The entity should NOT have a valid GTS ID
        assert!(
            entity.gts_id.is_none(),
            "gts: prefix (without //) should not be stripped, resulting in invalid GTS ID"
        );
    }

    #[test]
    fn test_gts_colon_prefix_not_valid_in_other_fields() {
        // "gts:" prefix should never appear in fields other than $id
        // These values should be treated as-is (not stripped) and won't be valid GTS IDs
        let content = json!({
            "id": "gts:gts.vendor.package.namespace.type.v1.0",
            "type": "gts:gts.vendor.package.namespace.type.v1~"
        });

        let cfg = GtsConfig::default();
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

        // The entity should NOT have a valid GTS ID since "gts:" prefix is not stripped
        assert!(
            entity.gts_id.is_none(),
            "gts: prefix in 'id' field should not be valid"
        );
    }

    #[test]
    fn test_gts_uri_prefix_only_stripped_from_dollar_id() {
        // Only $id field should have gts:// prefix stripped
        // The "id" field should NOT have the prefix stripped
        let content = json!({
            "id": "gts://gts.vendor.package.namespace.type.v1.0"
        });

        let cfg = GtsConfig::default();
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

        // The "id" field is not $id, so the gts:// prefix is NOT stripped
        // The value "gts://gts.vendor..." is not a valid GTS ID
        assert!(
            entity.gts_id.is_none(),
            "gts:// prefix in 'id' field (not $id) should not be stripped"
        );
    }

    // =============================================================================
    // Tests for strict schema/instance distinction (commit 1b536ea)
    // =============================================================================

    #[test]
    fn test_strict_schema_detection_requires_dollar_schema() {
        // A document is a schema ONLY if it has $schema field
        let content_with_schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "type": "object"
        });

        let entity_with_schema = GtsEntity::new(
            None,
            None,
            &content_with_schema,
            None,
            None,
            false,
            String::new(),
            None,
            None,
        );
        assert!(
            entity_with_schema.is_schema,
            "Document with $schema should be a schema"
        );

        // Same content without $schema should be an instance
        let content_without_schema = json!({
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "type": "object"
        });

        let entity_without_schema = GtsEntity::new(
            None,
            None,
            &content_without_schema,
            None,
            None,
            false,
            String::new(),
            None,
            None,
        );
        assert!(
            !entity_without_schema.is_schema,
            "Document without $schema should be an instance"
        );
    }

    #[test]
    fn test_well_known_instance_with_chained_gts_id() {
        // Well-known instance: id field contains a GTS ID (possibly chained)
        let content = json!({
            "id": "gts.x.core.events.type.v1~abc.app._.custom_event.v1.2"
        });

        let cfg = GtsConfig::default();
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

        assert!(!entity.is_schema, "Should be an instance");
        assert!(
            entity.gts_id.is_some(),
            "Well-known instance should have gts_id"
        );
        assert_eq!(
            entity.gts_id.as_ref().unwrap().id,
            "gts.x.core.events.type.v1~abc.app._.custom_event.v1.2"
        );
        assert_eq!(
            entity.instance_id,
            Some("gts.x.core.events.type.v1~abc.app._.custom_event.v1.2".to_owned())
        );
        // Schema ID should be extracted from chain (parent segment)
        assert_eq!(
            entity.schema_id,
            Some("gts.x.core.events.type.v1~".to_owned())
        );
        assert_eq!(entity.selected_entity_field, Some("id".to_owned()));
        assert_eq!(
            entity.selected_schema_id_field,
            Some("id".to_owned()),
            "selected_schema_id_field should be set when schema_id is derived from id field"
        );
    }

    #[test]
    fn test_anonymous_instance_with_uuid_id() {
        // Anonymous instance: id field contains UUID, type field has GTS schema ID
        let content = json!({
            "id": "7a1d2f34-5678-49ab-9012-abcdef123456",
            "type": "gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~"
        });

        let cfg = GtsConfig::default();
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

        assert!(!entity.is_schema, "Should be an instance");
        assert!(
            entity.gts_id.is_none(),
            "Anonymous instance should not have gts_id"
        );
        assert_eq!(
            entity.instance_id,
            Some("7a1d2f34-5678-49ab-9012-abcdef123456".to_owned())
        );
        assert_eq!(
            entity.schema_id,
            Some("gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~".to_owned())
        );
        assert_eq!(entity.selected_entity_field, Some("id".to_owned()));
        assert_eq!(entity.selected_schema_id_field, Some("type".to_owned()));
    }

    #[test]
    fn test_effective_id_for_schema() {
        // For schemas, effective_id should return the GTS ID
        let content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~"
        });

        let cfg = GtsConfig::default();
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

        assert_eq!(
            entity.effective_id(),
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned())
        );
    }

    #[test]
    fn test_effective_id_for_well_known_instance() {
        // For well-known instances, effective_id should return the GTS ID
        let content = json!({
            "id": "gts.x.core.events.type.v1~abc.app._.custom_event.v1.2"
        });

        let cfg = GtsConfig::default();
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

        assert_eq!(
            entity.effective_id(),
            Some("gts.x.core.events.type.v1~abc.app._.custom_event.v1.2".to_owned())
        );
    }

    #[test]
    fn test_effective_id_for_anonymous_instance() {
        // For anonymous instances, effective_id should return the instance_id (UUID)
        let content = json!({
            "id": "7a1d2f34-5678-49ab-9012-abcdef123456",
            "type": "gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~"
        });

        let cfg = GtsConfig::default();
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

        assert_eq!(
            entity.effective_id(),
            Some("7a1d2f34-5678-49ab-9012-abcdef123456".to_owned())
        );
    }

    #[test]
    fn test_effective_id_returns_none_when_no_id() {
        // When there's no id field, effective_id should return None
        let content = json!({
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let cfg = GtsConfig::default();
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

        assert_eq!(entity.effective_id(), None);
    }

    #[test]
    fn test_well_known_instance_single_segment_no_schema_id() {
        // Well-known instance with single-segment GTS ID (no chain)
        // Should not have schema_id extracted from chain
        let content = json!({
            "id": "gts.vendor.package.namespace.type.v1.0~a.b.c.d.v1"
        });

        let cfg = GtsConfig::default();
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

        assert!(!entity.is_schema);
        assert!(entity.gts_id.is_some());
        assert_eq!(
            entity.gts_id.as_ref().unwrap().id,
            "gts.vendor.package.namespace.type.v1.0~a.b.c.d.v1"
        );
        // Chained ID should have schema_id extracted from the chain
        assert_eq!(
            entity.schema_id,
            Some("gts.vendor.package.namespace.type.v1.0~".to_owned())
        );
    }

    #[test]
    fn test_extract_ref_strings_normalizes_gts_uri_prefix() {
        // $ref values with gts:// prefix should be normalized (prefix stripped)
        let content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "allOf": [
                {"$ref": "gts://gts.other.package.namespace.type.v2.0~"}
            ],
            "properties": {
                "nested": {
                    "$ref": "gts://gts.third.package.namespace.type.v3.0~"
                }
            }
        });

        let cfg = GtsConfig::default();
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

        // schema_refs should contain normalized refs (without gts:// prefix)
        assert!(!entity.schema_refs.is_empty());
        assert!(
            entity
                .schema_refs
                .iter()
                .any(|r| r.id == "gts.other.package.namespace.type.v2.0~"),
            "Ref should be normalized (gts:// prefix stripped)"
        );
        assert!(
            entity
                .schema_refs
                .iter()
                .any(|r| r.id == "gts.third.package.namespace.type.v3.0~"),
            "Nested ref should be normalized"
        );
        // Should not contain the gts:// prefix
        assert!(
            !entity
                .schema_refs
                .iter()
                .any(|r| r.id.starts_with("gts://")),
            "No ref should contain gts:// prefix"
        );
    }

    #[test]
    fn test_extract_ref_strings_preserves_local_refs() {
        // Local JSON Pointer refs should be preserved as-is
        let content = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "gts://gts.vendor.package.namespace.type.v1.0~",
            "$defs": {
                "Base": {"type": "object"}
            },
            "allOf": [
                {"$ref": "#/$defs/Base"}
            ]
        });

        let cfg = GtsConfig::default();
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

        // Local refs should be in schema_refs
        assert!(
            entity.schema_refs.iter().any(|r| r.id == "#/$defs/Base"),
            "Local ref should be preserved"
        );
    }

    #[test]
    fn test_instance_without_id_field_has_no_effective_id() {
        // Instance without id field should have no effective_id
        // This is the case that should return an error during registration
        let content = json!({
            "type": "gts.vendor.package.namespace.type.v1.0~",
            "name": "test"
        });

        let cfg = GtsConfig::default();
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

        assert!(!entity.is_schema);
        assert_eq!(
            entity.effective_id(),
            None,
            "Instance without id should have no effective_id"
        );
        assert!(entity.instance_id.is_none());
        assert!(entity.gts_id.is_none());
    }
}
