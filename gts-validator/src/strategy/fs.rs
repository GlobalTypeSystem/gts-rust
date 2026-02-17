//! Filesystem validation source.
//!
//! Discovers files on disk and yields `ValidationItem`s for the validation pipeline.

use std::path::{Path, PathBuf};

use glob::Pattern;
use walkdir::WalkDir;

use crate::config::FsSourceConfig;
use crate::strategy::{ContentFormat, ValidationItem};

/// Directories to skip
pub const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", "vendor", ".gts-spec"];

/// Files to skip (path suffixes).
/// NOTE: Repo-specific paths should be passed via `FsSourceConfig.exclude` instead.
/// This list is reserved for files that are universally irrelevant across GTS repos.
pub const SKIP_FILES: &[&str] = &[];

/// Check if a path matches any of the exclude patterns
fn matches_exclude(path: &Path, exclude_patterns: &[Pattern]) -> bool {
    let path_str = path.to_string_lossy();
    for pattern in exclude_patterns {
        if pattern.matches(&path_str)
            || path
                .file_name()
                .is_some_and(|name| pattern.matches(&name.to_string_lossy()))
        {
            return true;
        }
    }
    false
}

/// Check if a directory entry is a skip directory (for `WalkDir::filter_entry`).
/// Returns `true` if the entry should be **included** (i.e., is NOT a skip dir).
fn is_not_skip_dir(entry: &walkdir::DirEntry) -> bool {
    if entry.file_type().is_dir()
        && let Some(name) = entry.file_name().to_str()
    {
        return !SKIP_DIRS.contains(&name);
    }
    true
}

/// Check if file has a supported extension.
fn matches_file_pattern(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md" | "json" | "yaml" | "yml")
    )
}

/// Find all files to scan in the given paths.
#[must_use]
pub fn find_files(config: &FsSourceConfig) -> Vec<PathBuf> {
    let mut files = Vec::new();

    // Parse exclude patterns
    let exclude_patterns: Vec<Pattern> = config
        .exclude
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();

    for path in &config.paths {
        if path.is_file() {
            if matches_file_pattern(path) && !matches_exclude(path, &exclude_patterns) {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(path)
                .follow_links(config.follow_links)
                .into_iter()
                .filter_entry(is_not_skip_dir)
                .filter_map(Result::ok)
            {
                let file_path = entry.path();

                // Only process files
                if !file_path.is_file() {
                    continue;
                }

                // Check file pattern
                if !matches_file_pattern(file_path) {
                    continue;
                }

                // Check exclude patterns
                if matches_exclude(file_path, &exclude_patterns) {
                    continue;
                }

                // Check against skip files (suffix match, not substring)
                let rel_path = file_path.to_string_lossy();
                if SKIP_FILES.iter().any(|skip| rel_path.ends_with(skip)) {
                    continue;
                }

                files.push(file_path.to_path_buf());
            }
        }
    }

    files.sort();
    files.dedup();
    files
}

/// Determine the content format from a file extension.
pub fn content_format_for(path: &Path) -> Option<ContentFormat> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => Some(ContentFormat::Markdown),
        Some("json") => Some(ContentFormat::Json),
        Some("yaml" | "yml") => Some(ContentFormat::Yaml),
        _ => None,
    }
}

/// Read a file into a `ValidationItem`, respecting `max_file_size`.
///
/// Returns `None` if the file should be skipped (too large, read error, unsupported format).
pub fn read_validation_item(path: &Path, max_file_size: u64) -> Option<ValidationItem> {
    // Check file size
    if let Ok(metadata) = std::fs::metadata(path)
        && metadata.len() > max_file_size
    {
        return None;
    }

    // Verify the file has a supported format before reading
    content_format_for(path)?;

    let content = std::fs::read_to_string(path).ok()?;

    Some(ValidationItem { content })
}
