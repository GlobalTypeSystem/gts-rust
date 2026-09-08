use gts::entities::{GtsConfig, GtsEntity, GtsFile};
use gts::gts::{GTS_ID_PREFIX, GtsId};
use gts::store::GtsStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

const EXCLUDE_DIRS: &[&str] = &["node_modules", "dist", "build"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsJsonValidationIssue {
    pub file: String,
    pub stage: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsJsonValidationResult {
    pub ok: bool,
    pub files: usize,
    pub documents: usize,
    pub gts_entities: usize,
    pub schemas: usize,
    pub instances: usize,
    pub issues: Vec<GtsJsonValidationIssue>,
}

pub struct GtsJsonValidator {
    path: PathBuf,
    cfg: GtsConfig,
    files: usize,
    documents: usize,
    entities: Vec<GtsEntity>,
    issues: Vec<GtsJsonValidationIssue>,
}

impl GtsJsonValidator {
    #[must_use]
    pub fn new(path: &str, cfg: GtsConfig) -> Self {
        let expanded = shellexpand::tilde(path).to_string();
        GtsJsonValidator {
            path: PathBuf::from(expanded),
            cfg,
            files: 0,
            documents: 0,
            entities: Vec::new(),
            issues: Vec::new(),
        }
    }

    #[must_use]
    pub fn validate(mut self) -> GtsJsonValidationResult {
        let json_files = self.collect_json_files();
        for file_path in &json_files {
            self.read_file(file_path);
        }
        self.validate_json_schemas();
        let mut store = self.register_gts_entities();
        let (schemas_count, instances_count) = self.count_schema_instance();
        self.validate_schemas(&mut store);
        self.validate_instances(&mut store);

        let ok = self.issues.is_empty();
        GtsJsonValidationResult {
            ok,
            files: self.files,
            documents: self.documents,
            gts_entities: schemas_count + instances_count,
            schemas: schemas_count,
            instances: instances_count,
            issues: self.issues,
        }
    }

    fn collect_json_files(&mut self) -> Vec<PathBuf> {
        let resolved = self
            .path
            .canonicalize()
            .unwrap_or_else(|_| self.path.clone());

        if resolved.is_file() {
            if resolved
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                return vec![resolved];
            }
            self.add_issue_path(&resolved, "discovery", "Expected a .json file");
            return vec![];
        }

        if !resolved.is_dir() {
            self.add_issue_path(
                &resolved,
                "discovery",
                "Path does not exist or is not accessible",
            );
            return vec![];
        }

        let mut files: Vec<PathBuf> = Vec::new();
        let mut seen = HashSet::new();

        for entry in WalkDir::new(&resolved)
            .follow_links(true)
            .into_iter()
            .flatten()
        {
            let path = entry.path();

            if path.is_dir()
                && let Some(name) = path.file_name()
                && EXCLUDE_DIRS.contains(&name.to_string_lossy().as_ref())
            {
                continue;
            }

            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                let rp = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
                let key = rp.to_string_lossy().to_string();
                if seen.insert(key) {
                    files.push(rp);
                }
            }
        }

        files.sort();
        files
    }

    fn read_file(&mut self, file_path: &Path) {
        self.files += 1;
        let content_str = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                self.add_issue_path(file_path, "json", &e.to_string());
                return;
            }
        };

        let content: Value = match serde_json::from_str(&content_str) {
            Ok(v) => v,
            Err(e) => {
                self.add_issue_path(file_path, "json", &e.to_string());
                return;
            }
        };

        let json_file = GtsFile::new(
            file_path.to_string_lossy().to_string(),
            file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            content.clone(),
        );

        let values: Vec<(Option<usize>, &Value)> = if let Some(arr) = content.as_array() {
            arr.iter().enumerate().map(|(i, v)| (Some(i), v)).collect()
        } else {
            vec![(None, &content)]
        };

        for (index, value) in values {
            self.documents += 1;
            let entity = GtsEntity::new(
                Some(json_file.clone()),
                index,
                value,
                Some(&self.cfg),
                None,
                false,
                String::new(),
                None,
                None,
            );
            self.entities.push(entity);
        }
    }

    fn validate_json_schemas(&mut self) {
        let mut new_issues = Vec::new();
        for entity in &self.entities {
            if !entity.content.is_object() {
                continue;
            }
            let Some(obj) = entity.content.as_object() else {
                continue;
            };
            if let Some(schema_val) = obj.get("$schema")
                && !schema_val.is_string()
            {
                new_issues.push(GtsJsonValidationIssue {
                    file: Self::entity_file(entity),
                    stage: "json-schema".to_owned(),
                    message: "$schema must be a string".to_owned(),
                    index: entity.list_sequence,
                });
            }
        }
        self.issues.extend(new_issues);
    }

    fn register_gts_entities(&mut self) -> GtsStore {
        let mut store = GtsStore::new();
        let mut keys: HashSet<String> = HashSet::new();

        for entity in &mut self.entities {
            if !Self::is_gts_related(&entity.content) {
                continue;
            }

            let key = Self::registry_key(entity);
            let Some(mut key) = key else {
                self.issues.push(GtsJsonValidationIssue {
                    file: Self::entity_file(entity),
                    stage: "registry".to_owned(),
                    message: "GTS-related document has no registrable GTS ID".to_owned(),
                    index: entity.list_sequence,
                });
                continue;
            };

            // For non-schema entities without a selected_entity_field, generate UUID from raw id
            if !entity.is_schema
                && entity.selected_entity_field.is_none()
                && let Some(ref instance_id) = entity.instance_id
            {
                let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, instance_id.as_bytes());
                entity.instance_id = Some(uuid.to_string());
                key = uuid.to_string();
            }

            if keys.contains(&key) {
                self.issues.push(GtsJsonValidationIssue {
                    file: Self::entity_file(entity),
                    stage: "registry".to_owned(),
                    message: format!("Duplicate GTS entity ID '{key}'"),
                    index: entity.list_sequence,
                });
                continue;
            }

            keys.insert(key);
            let _ = store.register(entity.clone());
        }

        store
    }

    fn count_schema_instance(&self) -> (usize, usize) {
        let mut schemas = 0;
        let mut instances = 0;
        for entity in &self.entities {
            if !Self::is_gts_related(&entity.content) {
                continue;
            }
            if Self::registry_key(entity).is_none() {
                continue;
            }
            if entity.is_schema {
                schemas += 1;
            } else {
                instances += 1;
            }
        }
        (schemas, instances)
    }

    fn validate_schemas(&mut self, store: &mut GtsStore) {
        let mut schema_ids: Vec<(String, usize)> = Vec::new();
        for entity in &self.entities {
            if entity.is_schema
                && let Some(ref gts_id) = entity.gts_id
            {
                let id = gts_id.id().to_owned();
                if store.get(&id).is_some() {
                    let depth = gts_id.segments().len();
                    schema_ids.push((id, depth));
                }
            }
        }
        schema_ids.sort_by_key(|(_, depth)| *depth);

        // Validate base types (depth 1) first
        for (id, depth) in &schema_ids {
            if *depth != 1 {
                continue;
            }
            if let Err(e) = store.validate_schema(id) {
                let entity = store.get(id);
                let (file, index) = entity.map_or_else(
                    || (id.clone(), None),
                    |e| (Self::entity_file(e), e.list_sequence),
                );
                self.issues.push(GtsJsonValidationIssue {
                    file,
                    stage: "base-type".to_owned(),
                    message: e.to_string(),
                    index,
                });
            }
        }

        // Then validate derived types (depth > 1)
        for (id, depth) in &schema_ids {
            if *depth <= 1 {
                continue;
            }
            if let Err(e) = store.validate_schema(id) {
                let entity = store.get(id);
                let (file, index) = entity.map_or_else(
                    || (id.clone(), None),
                    |e| (Self::entity_file(e), e.list_sequence),
                );
                self.issues.push(GtsJsonValidationIssue {
                    file,
                    stage: "derived-type".to_owned(),
                    message: e.to_string(),
                    index,
                });
            }
        }
    }

    fn validate_instances(&mut self, store: &mut GtsStore) {
        for entity in &self.entities {
            if entity.is_schema || !Self::is_gts_related(&entity.content) {
                continue;
            }
            let key = Self::registry_key(entity);
            let Some(key) = key else {
                continue;
            };
            if let Err(e) = store.validate_instance(&key) {
                self.issues.push(GtsJsonValidationIssue {
                    file: Self::entity_file(entity),
                    stage: "instance".to_owned(),
                    message: e.to_string(),
                    index: entity.list_sequence,
                });
            }
        }
    }

    fn is_gts_related(value: &Value) -> bool {
        match value {
            Value::String(s) => s.contains(GTS_ID_PREFIX),
            Value::Object(map) => map.values().any(Self::is_gts_related),
            Value::Array(arr) => arr.iter().any(Self::is_gts_related),
            _ => false,
        }
    }

    fn registry_key(entity: &GtsEntity) -> Option<String> {
        if entity.is_schema {
            return entity.gts_id.as_ref().map(|id| id.id().to_owned());
        }
        if let Some(ref instance_id) = entity.instance_id {
            if entity.gts_id.is_some() {
                return Some(instance_id.clone());
            }
            if let Some(ref type_id) = entity.type_id
                && GtsId::is_valid(type_id)
            {
                return Some(instance_id.clone());
            }
        }
        None
    }

    fn entity_file(entity: &GtsEntity) -> String {
        entity
            .file
            .as_ref()
            .map_or_else(|| entity.label.clone(), |f| f.path.clone())
    }

    fn add_issue_path(&mut self, path: &Path, stage: &str, message: &str) {
        self.issues.push(GtsJsonValidationIssue {
            file: path.to_string_lossy().to_string(),
            stage: stage.to_owned(),
            message: message.to_owned(),
            index: None,
        });
    }
}
