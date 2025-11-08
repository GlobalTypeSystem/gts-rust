use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::entities::{GtsConfig, GtsEntity, GtsFile};
use crate::store::GtsReader;

const EXCLUDE_LIST: &[&str] = &["node_modules", "dist", "build"];

pub struct GtsFileReader {
    paths: Vec<PathBuf>,
    cfg: GtsConfig,
    files: Vec<PathBuf>,
    initialized: bool,
}

impl GtsFileReader {
    pub fn new(path: Vec<String>, cfg: Option<GtsConfig>) -> Self {
        let paths = path
            .iter()
            .map(|p| PathBuf::from(shellexpand::tilde(p).to_string()))
            .collect();

        GtsFileReader {
            paths,
            cfg: cfg.unwrap_or_default(),
            files: Vec::new(),
            initialized: false,
        }
    }

    fn collect_files(&mut self) {
        let valid_extensions = vec![".json", ".jsonc", ".gts"];
        let mut seen = std::collections::HashSet::new();
        let mut collected = Vec::new();

        for path in &self.paths {
            let resolved_path = path.canonicalize().unwrap_or_else(|_| path.clone());

            if resolved_path.is_file() {
                if let Some(ext) = resolved_path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if valid_extensions.contains(&format!(".{}", ext_str).as_str()) {
                        let rp = resolved_path.to_string_lossy().to_string();
                        if !seen.contains(&rp) {
                            seen.insert(rp.clone());
                            tracing::debug!("- discovered file: {:?}", resolved_path);
                            collected.push(resolved_path.clone());
                        }
                    }
                }
            } else if resolved_path.is_dir() {
                for entry in WalkDir::new(&resolved_path).follow_links(true) {
                    if let Ok(entry) = entry {
                        let path = entry.path();

                        // Skip excluded directories
                        if path.is_dir() {
                            if let Some(name) = path.file_name() {
                                if EXCLUDE_LIST.contains(&name.to_string_lossy().as_ref()) {
                                    continue;
                                }
                            }
                        }

                        if path.is_file() {
                            if let Some(ext) = path.extension() {
                                let ext_str = ext.to_string_lossy().to_lowercase();
                                if valid_extensions.contains(&format!(".{}", ext_str).as_str()) {
                                    let rp = path
                                        .canonicalize()
                                        .unwrap_or_else(|_| path.to_path_buf())
                                        .to_string_lossy()
                                        .to_string();
                                    if !seen.contains(&rp) {
                                        seen.insert(rp.clone());
                                        tracing::debug!("- discovered file: {:?}", path);
                                        collected.push(PathBuf::from(rp));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        self.files = collected;
    }

    fn load_json_file(&self, file_path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(file_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value)
    }

    fn process_file(&self, file_path: &Path) -> Vec<GtsEntity> {
        let mut entities = Vec::new();

        match self.load_json_file(file_path) {
            Ok(content) => {
                let json_file = GtsFile::new(
                    file_path.to_string_lossy().to_string(),
                    file_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    content.clone(),
                );

                // Handle both single objects and arrays
                if let Some(arr) = content.as_array() {
                    for (idx, item) in arr.iter().enumerate() {
                        let entity = GtsEntity::new(
                            Some(json_file.clone()),
                            Some(idx),
                            item.clone(),
                            Some(&self.cfg),
                            None,
                            false,
                            String::new(),
                            None,
                            None,
                        );
                        if entity.gts_id.is_some() {
                            tracing::debug!(
                                "- discovered entity: {}",
                                entity.gts_id.as_ref().unwrap().id
                            );
                            entities.push(entity);
                        }
                    }
                } else {
                    let entity = GtsEntity::new(
                        Some(json_file),
                        None,
                        content,
                        Some(&self.cfg),
                        None,
                        false,
                        String::new(),
                        None,
                        None,
                    );
                    if entity.gts_id.is_some() {
                        tracing::debug!(
                            "- discovered entity: {}",
                            entity.gts_id.as_ref().unwrap().id
                        );
                        entities.push(entity);
                    }
                }
            }
            Err(_) => {
                // Skip files that can't be parsed
            }
        }

        entities
    }
}

impl GtsReader for GtsFileReader {
    fn iter(&mut self) -> Box<dyn Iterator<Item = GtsEntity> + '_> {
        if !self.initialized {
            self.collect_files();
            self.initialized = true;
        }

        tracing::debug!(
            "Processing {} files from {:?}",
            self.files.len(),
            self.paths
        );

        let entities: Vec<GtsEntity> = self
            .files
            .iter()
            .flat_map(|file_path| self.process_file(file_path))
            .collect();

        Box::new(entities.into_iter())
    }

    fn read_by_id(&self, _entity_id: &str) -> Option<GtsEntity> {
        // For FileReader, we don't support random access by ID
        None
    }

    fn reset(&mut self) {
        self.initialized = false;
    }
}
