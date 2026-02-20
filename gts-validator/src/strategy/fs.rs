//! Filesystem validation source.
//!
//! Discovers files on disk and reads them safely for the validation pipeline.
//! Security properties enforced here:
//! - Symlinks are not followed by default (`follow_links: false`)
//! - Resolved paths are checked to remain within the repository root
//! - Device files, pipes, and sockets are skipped
//! - Maximum directory depth is enforced to prevent infinite recursion
//! - Bounded streaming reads prevent TOCTOU and memory `DoS`

use std::io::Read;
use std::path::{Path, PathBuf};

use glob::Pattern;
use walkdir::WalkDir;

use crate::config::FsSourceConfig;
use crate::error::{ScanError, ScanErrorKind};
use crate::strategy::ContentFormat;

/// Directories to skip
pub const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", "vendor", ".gts-spec"];

/// Files to skip (path suffixes).
/// NOTE: Repo-specific paths should be passed via `FsSourceConfig.exclude` instead.
/// This list is reserved for files that are universally irrelevant across GTS repos.
pub const SKIP_FILES: &[&str] = &[];

/// Result of attempting to read a file for scanning.
pub enum ScanResult {
    /// File was read successfully; contains the UTF-8 content.
    Ok(String),
    /// File could not be read or validated; contains the scan error.
    Err(ScanError),
}

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
///
/// Returns `(files, scan_errors)`:
/// - `files`: paths that passed all filters and are ready to read.
/// - `scan_errors`: walk errors (permission denied, loop, etc.) and boundary violations.
///   These are never silently discarded — CI must treat them as failures.
pub fn find_files(config: &FsSourceConfig) -> (Vec<PathBuf>, Vec<ScanError>) {
    let mut files = Vec::new();
    let mut scan_errors = Vec::new();

    let mut exclude_patterns = Vec::with_capacity(config.exclude.len());
    for pat_str in &config.exclude {
        match Pattern::new(pat_str) {
            Ok(pat) => exclude_patterns.push(pat),
            Err(e) => {
                scan_errors.push(ScanError {
                    file: PathBuf::from(pat_str),
                    kind: ScanErrorKind::InvalidExcludePattern,
                    message: format!("Invalid exclude glob pattern '{pat_str}': {e}"),
                });
            }
        }
    }

    for root in &config.paths {
        // Canonicalize the root once so we can enforce the boundary for every entry.
        let canonical_root = match root.canonicalize() {
            Ok(r) => r,
            Err(e) => {
                scan_errors.push(ScanError {
                    file: root.clone(),
                    kind: ScanErrorKind::IoError,
                    message: format!("Failed to canonicalize root path: {e}"),
                });
                continue;
            }
        };

        if root.is_file() {
            if matches_file_pattern(root) && !matches_exclude(root, &exclude_patterns) {
                files.push(root.clone());
            }
            continue;
        }

        if !root.is_dir() {
            continue;
        }

        for entry_result in WalkDir::new(root)
            .follow_links(config.follow_links)
            .max_depth(config.max_depth)
            .into_iter()
            .filter_entry(is_not_skip_dir)
        {
            let entry = match entry_result {
                Ok(e) => e,
                Err(walk_err) => {
                    // Propagate walk errors (permission denied, loop, etc.) as ScanErrors.
                    let path = walk_err
                        .path()
                        .map_or_else(|| root.clone(), Path::to_path_buf);
                    scan_errors.push(ScanError {
                        file: path,
                        kind: ScanErrorKind::WalkError,
                        message: format!("Directory traversal error: {walk_err}"),
                    });
                    continue;
                }
            };

            let file_path = entry.path();

            if !file_path.is_file() {
                continue;
            }

            // Enforce repository boundary: canonicalize and verify the resolved path
            // stays within the root. This catches symlink escapes even when follow_links
            // is true, and rejects any path that resolves outside the scan root.
            match file_path.canonicalize() {
                Ok(canonical_path) => {
                    if !canonical_path.starts_with(&canonical_root) {
                        scan_errors.push(ScanError {
                            file: file_path.to_path_buf(),
                            kind: ScanErrorKind::OutsideRepository,
                            message: format!(
                                "Path resolves outside repository root: {} -> {}",
                                file_path.display(),
                                canonical_path.display()
                            ),
                        });
                        continue;
                    }
                }
                Err(e) => {
                    scan_errors.push(ScanError {
                        file: file_path.to_path_buf(),
                        kind: ScanErrorKind::IoError,
                        message: format!("Failed to canonicalize path: {e}"),
                    });
                    continue;
                }
            }

            // Skip devices, pipes, sockets — only regular files
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                if let Ok(ft) = entry.metadata().map(|m| m.file_type())
                    && (ft.is_block_device()
                        || ft.is_char_device()
                        || ft.is_fifo()
                        || ft.is_socket())
                {
                    continue;
                }
            }

            if !matches_file_pattern(file_path) {
                continue;
            }

            if matches_exclude(file_path, &exclude_patterns) {
                continue;
            }

            let rel_path = file_path.to_string_lossy();
            if SKIP_FILES.iter().any(|skip| rel_path.ends_with(skip)) {
                continue;
            }

            files.push(file_path.to_path_buf());
        }
    }

    files.sort();
    files.dedup();
    (files, scan_errors)
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

/// Read a file using a bounded streaming read, enforcing `max_file_size`.
///
/// Uses `Read::take` to avoid TOCTOU races and prevent memory `DoS`:
/// the kernel size check and the actual read are the same operation.
/// Never calls `read_to_string` on an unbounded handle.
///
/// Returns `ScanResult::Err` (never silently discards failures) if:
/// - The file exceeds `max_file_size`
/// - An I/O error occurs
/// - The content is not valid UTF-8
pub fn read_file_bounded(path: &Path, max_file_size: u64) -> ScanResult {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return ScanResult::Err(ScanError {
                file: path.to_owned(),
                kind: ScanErrorKind::IoError,
                message: format!("Failed to open file: {e}"),
            });
        }
    };

    // Read at most max_file_size + 1 bytes to detect oversized files
    let mut buffer = Vec::new();
    match file.take(max_file_size + 1).read_to_end(&mut buffer) {
        Ok(_) => {}
        Err(e) => {
            return ScanResult::Err(ScanError {
                file: path.to_owned(),
                kind: ScanErrorKind::IoError,
                message: format!("Failed to read file: {e}"),
            });
        }
    }

    if buffer.len() as u64 > max_file_size {
        return ScanResult::Err(ScanError {
            file: path.to_owned(),
            kind: ScanErrorKind::FileTooLarge,
            message: format!("File exceeds maximum size of {max_file_size} bytes"),
        });
    }

    match String::from_utf8(buffer) {
        Ok(content) => ScanResult::Ok(content),
        Err(_) => ScanResult::Err(ScanError {
            file: path.to_owned(),
            kind: ScanErrorKind::InvalidEncoding,
            message: "File is not valid UTF-8".to_owned(),
        }),
    }
}
