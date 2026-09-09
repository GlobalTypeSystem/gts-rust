use gts::entities::{GtsConfig, GtsEntity, GtsFile};
use gts::gts::{GTS_ID_PREFIX, GTS_ID_URI_PREFIX, GtsId};
use gts::store::GtsStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

/// The GTS schema extension keyword. It is a fixed keyword (not derived from
/// `GTS_ID_PREFIX`), so the marker below is matched verbatim.
const X_GTS_REF_KEYWORD: &str = "x-gts-ref";

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
    exclude: Vec<String>,
    files: usize,
    documents: usize,
    entities: Vec<GtsEntity>,
    issues: Vec<GtsJsonValidationIssue>,
}

impl GtsJsonValidator {
    #[must_use]
    pub fn new(path: &str, cfg: GtsConfig, exclude: Vec<String>) -> Self {
        let expanded = shellexpand::tilde(path).to_string();
        GtsJsonValidator {
            path: PathBuf::from(expanded),
            cfg,
            exclude,
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
        self.check_schema_field_type();
        let (mut store, registered) = self.register_gts_entities();
        let (schemas_count, instances_count) = self.count_schema_instance();
        self.validate_schemas(&mut store);
        self.validate_instances(&mut store, &registered);

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

        // Clone into a local so the traversal closure does not borrow `self`
        // (the loop body needs `&mut self` to record issues).
        let exclude = self.exclude.clone();
        for entry in WalkDir::new(&resolved)
            .follow_links(true)
            .into_iter()
            .filter_entry(move |e| {
                // Prune excluded directories before descending into them.
                if e.file_type().is_dir()
                    && let Some(name) = e.file_name().to_str()
                {
                    return !exclude.iter().any(|x| x == name);
                }
                true
            })
        {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    // Surface traversal errors instead of silently dropping them.
                    let file = e.path().map_or_else(
                        || resolved.to_string_lossy().to_string(),
                        |p| p.to_string_lossy().to_string(),
                    );
                    self.issues.push(GtsJsonValidationIssue {
                        file,
                        stage: "discovery".to_owned(),
                        message: e.to_string(),
                        index: None,
                    });
                    continue;
                }
            };

            let path = entry.path();
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

    /// Cheap, portable heuristic to decide whether a document is GTS-related:
    /// its raw text must contain at least one GTS marker. GTS schemas and
    /// instances always carry one, so anything without a marker is ignored.
    ///
    /// The markers mirror the three combinations agreed on the PR:
    /// - `{GTS_ID_PREFIX}.`   — the configurable id prefix (e.g. `gts.`)
    /// - `{GTS_ID_PREFIX}://` — the `$id` URI scheme (`gts://`)
    /// - `x-{GTS_ID_PREFIX}-` — the `x-gts-ref` schema keyword
    ///
    /// The URI scheme and the `x-gts-ref` keyword are fixed in this
    /// implementation (they are not re-derived when `GTS_ID_PREFIX` is
    /// overridden), so they are matched against their real constant values to
    /// keep detection correct under a custom prefix.
    fn is_gts_related(text: &str) -> bool {
        text.contains(GTS_ID_PREFIX)
            || text.contains(GTS_ID_URI_PREFIX)
            || text.contains(X_GTS_REF_KEYWORD)
    }

    fn read_file(&mut self, file_path: &Path) {
        let content_str = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                self.add_issue_path(file_path, "json", &e.to_string());
                return;
            }
        };

        // Ignore files with no GTS marker before paying the parse cost.
        if !Self::is_gts_related(&content_str) {
            return;
        }
        self.files += 1;

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

    /// Checks that, when a `$schema` field is present, it is a string as the GTS
    /// schema-detection rule requires. This does not classify or validate the
    /// document — it only guards the type of the `$schema` marker.
    fn check_schema_field_type(&mut self) {
        let mut new_issues = Vec::new();
        for entity in &self.entities {
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

    fn register_gts_entities(&mut self) -> (GtsStore, HashSet<usize>) {
        let mut store = GtsStore::new();
        let mut keys: HashSet<String> = HashSet::new();
        let mut registered: HashSet<usize> = HashSet::new();

        for (idx, entity) in self.entities.iter_mut().enumerate() {
            let Some(mut key) = Self::registry_key(entity) else {
                // A schema ($schema present) that yields no registrable GTS id
                // has a malformed or non-GTS $id — report it instead of
                // silently dropping it. Non-schema documents without an id are
                // simply ignored.
                if entity.is_schema {
                    self.issues.push(GtsJsonValidationIssue {
                        file: Self::entity_file(entity),
                        stage: "registry".to_owned(),
                        message: "GTS schema has a malformed or non-GTS $id".to_owned(),
                        index: entity.list_sequence,
                    });
                }
                continue;
            };

            // For anonymous instances (no GTS id, no selected entity field),
            // derive a stable UUID from the raw instance id.
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
            if let Err(e) = store.register(entity.clone()) {
                self.issues.push(GtsJsonValidationIssue {
                    file: Self::entity_file(entity),
                    stage: "registry".to_owned(),
                    message: e.to_string(),
                    index: entity.list_sequence,
                });
            } else {
                registered.insert(idx);
            }
        }

        (store, registered)
    }

    fn count_schema_instance(&self) -> (usize, usize) {
        let mut schemas = 0;
        let mut instances = 0;
        for entity in &self.entities {
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
        // Validate schemas ordered by (derivation level, GTS ID, file name,
        // array index): base types (level 1) first, then level-2 derivations,
        // level-3, and so on — each level a total order on the remaining keys.
        // The reporting stage is derived from that level.
        struct Pending {
            depth: usize,
            id: String,
            file: String,
            index: Option<usize>,
        }

        let mut schemas: Vec<Pending> = Vec::new();
        for entity in &self.entities {
            if entity.is_schema
                && let Some(ref gts_id) = entity.gts_id
            {
                let id = gts_id.id().to_owned();
                if store.get(&id).is_some() {
                    schemas.push(Pending {
                        depth: gts_id.segments().len(),
                        id,
                        file: Self::entity_file(entity),
                        index: entity.list_sequence,
                    });
                }
            }
        }
        schemas.sort_by(|a, b| {
            (a.depth, &a.id, &a.file, a.index).cmp(&(b.depth, &b.id, &b.file, b.index))
        });

        for s in schemas {
            if let Err(e) = store.validate_schema(&s.id) {
                let stage = if s.depth <= 1 {
                    "base-type"
                } else {
                    "derived-type"
                };
                self.issues.push(GtsJsonValidationIssue {
                    file: s.file,
                    stage: stage.to_owned(),
                    message: e.to_string(),
                    index: s.index,
                });
            }
        }
    }

    fn validate_instances(&mut self, store: &mut GtsStore, registered: &HashSet<usize>) {
        // Validate instances after all schemas, using the same total order as
        // schemas: (derivation level, GTS ID, file name, array index). The
        // level is the GTS-ID chain depth (falling back to the declared type's
        // depth for anonymous instances), so it is stable and independent of
        // filesystem traversal.
        struct Pending {
            depth: usize,
            id: String,
            file: String,
            index: Option<usize>,
            registry_key: String,
        }

        let mut pending: Vec<Pending> = Vec::new();
        for (idx, entity) in self.entities.iter().enumerate() {
            if entity.is_schema {
                continue;
            }
            let Some(registry_key) = Self::registry_key(entity) else {
                continue;
            };
            // Skip rejected duplicates: only validate successfully registered entities
            if !registered.contains(&idx) {
                continue;
            }
            pending.push(Pending {
                depth: Self::entity_depth(entity),
                id: entity
                    .gts_id
                    .as_ref()
                    .map_or_else(String::new, |g| g.id().to_owned()),
                file: Self::entity_file(entity),
                index: entity.list_sequence,
                registry_key,
            });
        }
        pending.sort_by(|a, b| {
            (a.depth, &a.id, &a.file, a.index).cmp(&(b.depth, &b.id, &b.file, b.index))
        });

        for p in pending {
            if let Err(e) = store.validate_instance(&p.registry_key) {
                self.issues.push(GtsJsonValidationIssue {
                    file: p.file,
                    stage: "instance".to_owned(),
                    message: e.to_string(),
                    index: p.index,
                });
            }
        }
    }

    /// Derivation level used for ordering: the GTS-ID chain depth for schemas
    /// and well-known instances, falling back to the declared type's depth for
    /// anonymous instances (which have no GTS ID of their own).
    fn entity_depth(entity: &GtsEntity) -> usize {
        if let Some(ref gts_id) = entity.gts_id {
            return gts_id.segments().len();
        }
        if let Some(ref type_id) = entity.type_id
            && let Ok(gts_id) = GtsId::try_new(type_id)
        {
            return gts_id.segments().len();
        }
        0
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // All fixtures are built from the compile-time `GTS_ID_PREFIX` so the suite
    // also passes under a custom prefix (e.g. `GTS_ID_PREFIX=acme.`).
    fn base_schema() -> String {
        format!(
            r#"{{
                "$id": "{GTS_ID_URI_PREFIX}{GTS_ID_PREFIX}cli.core.test.base.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {{ "name": {{ "type": "string" }} }},
                "required": ["name"]
            }}"#
        )
    }

    fn leaf_schema() -> String {
        format!(
            r#"{{
                "$id": "{GTS_ID_URI_PREFIX}{GTS_ID_PREFIX}cli.core.test.base.v1~cli.core.test.leaf.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {{ "$ref": "{GTS_ID_URI_PREFIX}{GTS_ID_PREFIX}cli.core.test.base.v1~" }},
                    {{ "properties": {{ "extra": {{ "type": "string" }} }} }}
                ]
            }}"#
        )
    }

    fn base_instance() -> String {
        format!(
            r#"{{
                "id": "{GTS_ID_PREFIX}cli.core.test.base.v1~cli.app._.thing.v1.0",
                "name": "hello"
            }}"#
        )
    }

    fn write(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    fn run(dir: &Path) -> GtsJsonValidationResult {
        GtsJsonValidator::new(dir.to_str().unwrap(), GtsConfig::default(), vec![]).validate()
    }

    #[test]
    fn test_valid_schema_and_instance_set() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "base.schema.json", &base_schema());
        write(dir.path(), "thing.json", &base_instance());

        let result = run(dir.path());

        assert!(result.ok, "expected ok, issues: {:?}", result.issues);
        assert_eq!(result.schemas, 1);
        assert_eq!(result.instances, 1);
        assert_eq!(result.gts_entities, 2);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_derived_schema_set_is_valid() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "base.schema.json", &base_schema());
        write(dir.path(), "leaf.schema.json", &leaf_schema());

        let result = run(dir.path());

        assert!(result.ok, "expected ok, issues: {:?}", result.issues);
        assert_eq!(result.schemas, 2);
        assert_eq!(result.instances, 0);
    }

    #[test]
    fn test_malformed_schema_id_is_reported() {
        let dir = TempDir::new().unwrap();
        // gts:// URI is correct, but the body uses a non-GTS prefix (gtx.).
        let malformed = r#"{
            "$id": "gts://gtx.cli.core.test.bad.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        }"#;
        write(dir.path(), "bad.schema.json", malformed);

        let result = run(dir.path());

        assert!(!result.ok, "malformed GTS id should fail");
        assert_eq!(result.gts_entities, 0);
        assert!(
            result
                .issues
                .iter()
                .any(|i| i.stage == "registry" && i.message.contains("malformed")),
            "expected a malformed-id diagnostic, got: {:?}",
            result.issues
        );
    }

    #[test]
    fn test_incidental_prefix_mention_is_not_registered() {
        let dir = TempDir::new().unwrap();
        // The marker heuristic considers this file GTS-related (it mentions the
        // prefix), so it is parsed — but the document has no registrable GTS id
        // and is not a schema, so it is neither registered nor flagged.
        let doc = format!(
            r#"{{ "description": "see {GTS_ID_PREFIX}foo.bar for details", "value": 42 }}"#
        );
        write(dir.path(), "unrelated.json", &doc);

        let result = run(dir.path());

        assert!(
            result.ok,
            "incidental mention should not fail: {:?}",
            result.issues
        );
        assert_eq!(result.documents, 1);
        assert_eq!(result.gts_entities, 0);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_duplicate_entity_is_reported() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.schema.json", &base_schema());
        write(dir.path(), "b.schema.json", &base_schema());

        let result = run(dir.path());

        assert!(!result.ok, "duplicate ids should fail");
        assert!(
            result
                .issues
                .iter()
                .any(|i| i.message.contains("Duplicate")),
            "expected a duplicate diagnostic, got: {:?}",
            result.issues
        );
    }

    #[test]
    fn test_non_gts_files_are_ignored() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "base.schema.json", &base_schema());

        // A plain non-GTS JSON file (no marker) is ignored — not counted.
        write(
            dir.path(),
            "package.json",
            r#"{ "name": "pkg", "version": "1.0.0" }"#,
        );

        // Even a broken JSON file is ignored when it carries no GTS marker: the
        // heuristic replaces the old hard-coded directory exclusions, so nested
        // dependency dirs cost nothing beyond a cheap text scan.
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        write(&nm, "broken.json", "{ this is not json ");

        let result = run(dir.path());

        assert!(
            result.ok,
            "non-GTS files must be ignored: {:?}",
            result.issues
        );
        assert_eq!(result.files, 1, "only the GTS schema should be processed");
        assert_eq!(result.schemas, 1);
    }

    #[test]
    fn test_marker_heuristic_matches_expected_combinations() {
        let bare = GTS_ID_PREFIX.trim_end_matches('.');

        // Each of the three agreed combinations marks a file as GTS-related.
        assert!(GtsJsonValidator::is_gts_related(&format!(
            r#"{{ "id": "{bare}.x.y.z.t.v1~a.b.c.d.v1.0" }}"#
        )));
        assert!(GtsJsonValidator::is_gts_related(&format!(
            r#"{{ "$id": "{GTS_ID_URI_PREFIX}{bare}.x.y.z.t.v1~" }}"#
        )));
        assert!(GtsJsonValidator::is_gts_related(
            r#"{ "properties": { "p": { "x-gts-ref": "..." } } }"#
        ));

        // A document with none of the markers is not GTS-related.
        assert!(!GtsJsonValidator::is_gts_related(
            r#"{ "name": "widgets", "version": "1.0.0" }"#
        ));
    }

    // A depth-1 (base) Type Schema that fails validation: it references a
    // missing dependency, so `validate_schema` errors regardless of prefix.
    fn invalid_base(seg: &str) -> String {
        format!(
            r#"{{
                "$id": "{GTS_ID_URI_PREFIX}{GTS_ID_PREFIX}cli.core.test.{seg}.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {{ "$ref": "{GTS_ID_URI_PREFIX}{GTS_ID_PREFIX}cli.core.test.missing.v1~" }}
                ]
            }}"#
        )
    }

    // A depth-2 (derived) Type Schema under the valid base that fails
    // validation (missing dependency).
    fn invalid_leaf() -> String {
        format!(
            r#"{{
                "$id": "{GTS_ID_URI_PREFIX}{GTS_ID_PREFIX}cli.core.test.base.v1~cli.core.test.leaf.v1~",
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "allOf": [
                    {{ "$ref": "{GTS_ID_URI_PREFIX}{GTS_ID_PREFIX}cli.core.test.missing.v1~" }}
                ]
            }}"#
        )
    }

    // A well-known instance of the base type missing the required `name` field.
    fn invalid_instance(seg: &str) -> String {
        format!(r#"{{ "id": "{GTS_ID_PREFIX}cli.core.test.base.v1~cli.app._.{seg}.v1.0" }}"#)
    }

    #[test]
    fn test_schema_errors_ordered_by_depth_then_gts_id() {
        let dir = TempDir::new().unwrap();
        // Valid base so the derived leaf resolves its parent chain.
        write(dir.path(), "base.schema.json", &base_schema());
        // File names are deliberately out of GTS-ID / depth order to prove the
        // reported order comes from (depth, id), not filesystem traversal.
        write(dir.path(), "0_mmm.schema.json", &invalid_base("mmm"));
        write(dir.path(), "a_leaf.schema.json", &invalid_leaf());
        write(dir.path(), "z_aaa.schema.json", &invalid_base("aaa"));

        let result = run(dir.path());

        let schema_issues: Vec<&GtsJsonValidationIssue> = result
            .issues
            .iter()
            .filter(|i| i.stage == "base-type" || i.stage == "derived-type")
            .collect();

        assert_eq!(schema_issues.len(), 3, "issues: {:?}", result.issues);
        // Base types first, alphabetically by GTS ID: aaa, then mmm.
        assert_eq!(schema_issues[0].stage, "base-type");
        assert!(schema_issues[0].file.ends_with("z_aaa.schema.json"));
        assert_eq!(schema_issues[1].stage, "base-type");
        assert!(schema_issues[1].file.ends_with("0_mmm.schema.json"));
        // Derived (depth-2) types afterwards.
        assert_eq!(schema_issues[2].stage, "derived-type");
        assert!(schema_issues[2].file.ends_with("a_leaf.schema.json"));
    }

    #[test]
    fn test_instance_errors_ordered_by_gts_id() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "base.schema.json", &base_schema());
        // File order (a_zeta, z_alpha) is the reverse of GTS-ID order
        // (alpha < zeta), so this proves the sort keys off the GTS ID.
        write(dir.path(), "z_alpha.json", &invalid_instance("alpha"));
        write(dir.path(), "a_zeta.json", &invalid_instance("zeta"));

        let result = run(dir.path());

        let instance_issues: Vec<&GtsJsonValidationIssue> = result
            .issues
            .iter()
            .filter(|i| i.stage == "instance")
            .collect();

        assert_eq!(instance_issues.len(), 2, "issues: {:?}", result.issues);
        assert!(
            instance_issues[0].file.ends_with("z_alpha.json"),
            "alpha should be first: {instance_issues:?}"
        );
        assert!(
            instance_issues[1].file.ends_with("a_zeta.json"),
            "zeta should be second: {instance_issues:?}"
        );
    }
}
