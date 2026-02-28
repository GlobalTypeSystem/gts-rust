use anyhow::{Result, bail};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Directories that are automatically ignored (e.g., trybuild `compile_fail` tests)
pub const AUTO_IGNORE_DIRS: &[&str] = &["compile_fail"];

/// Reason why a file was skipped
#[derive(Debug, Clone, Copy)]
pub enum SkipReason {
    ExcludePattern,
    AutoIgnoredDir,
    IgnoreDirective,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExcludePattern => write!(f, "matched --exclude pattern"),
            Self::AutoIgnoredDir => write!(f, "in auto-ignored directory (compile_fail)"),
            Self::IgnoreDirective => write!(f, "has // gts:ignore directive"),
        }
    }
}

/// Check if a path matches any of the exclude patterns
#[must_use]
pub fn should_exclude_path(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    for pattern in patterns {
        if matches_glob_pattern(&path_str, pattern) {
            return true;
        }
    }
    false
}

/// Simple glob pattern matching
/// Supports: * (any characters), ** (any path segments)
#[must_use]
pub fn matches_glob_pattern(path: &str, pattern: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let regex_pattern = regex::escape(pattern)
        .replace(r"\*\*", "<<DOUBLESTAR>>")
        .replace(r"\*", "[^/]*")
        .replace("<<DOUBLESTAR>>", ".*");

    if let Ok(re) = Regex::new(&format!("(^|/){regex_pattern}($|/)")) {
        re.is_match(&normalized)
    } else {
        normalized.contains(pattern)
    }
}

/// Check if path is in an auto-ignored directory (e.g., `compile_fail`)
#[must_use]
pub fn is_in_auto_ignored_dir(path: &Path) -> bool {
    path.components().any(|component| {
        if let Some(name) = component.as_os_str().to_str() {
            AUTO_IGNORE_DIRS.contains(&name)
        } else {
            false
        }
    })
}

/// Check if file content starts with the gts:ignore directive
#[must_use]
pub fn has_ignore_directive(content: &str) -> bool {
    for line in content.lines().take(10) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.to_lowercase().starts_with("// gts:ignore") {
            return true;
        }
        if !trimmed.starts_with("//") && !trimmed.starts_with("#!") {
            break;
        }
    }
    false
}

/// Walk Rust source files in the given source path, applying exclusion rules.
/// Calls `visitor` for each file that should be processed, with (path, content).
/// Returns (`files_scanned`, `files_skipped`).
///
/// # Errors
/// Returns an error if the visitor closure returns an error for any file.
pub fn walk_rust_files<F>(
    source_path: &Path,
    exclude_patterns: &[String],
    verbose: u8,
    mut visitor: F,
) -> Result<(usize, usize)>
where
    F: FnMut(&Path, &str) -> Result<()>,
{
    let mut files_scanned = 0;
    let mut files_skipped = 0;

    for entry in WalkDir::new(source_path).follow_links(true) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("warning: skipping unreadable path during walk: {e}");
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }

        if should_exclude_path(path, exclude_patterns) {
            files_skipped += 1;
            if verbose > 0 {
                println!(
                    "  Skipped: {} ({})",
                    path.display(),
                    SkipReason::ExcludePattern
                );
            }
            continue;
        }

        if is_in_auto_ignored_dir(path) {
            files_skipped += 1;
            if verbose > 0 {
                println!(
                    "  Skipped: {} ({})",
                    path.display(),
                    SkipReason::AutoIgnoredDir
                );
            }
            continue;
        }

        match fs::read_to_string(path) {
            Err(e) => {
                eprintln!("warning: skipping unreadable file {}: {e}", path.display());
                files_skipped += 1;
            }
            Ok(content) => {
                if has_ignore_directive(&content) {
                    files_skipped += 1;
                    if verbose > 0 {
                        println!(
                            "  Skipped: {} ({})",
                            path.display(),
                            SkipReason::IgnoreDirective
                        );
                    }
                    continue;
                }

                files_scanned += 1;
                visitor(path, &content)?;
            }
        }
    }

    Ok((files_scanned, files_skipped))
}

/// Compute the sandbox root from the source path and optional output override.
///
/// - If `--output` is provided: sandbox root is the output directory (trusted root).
/// - If `--source` is a file (no `--output`): sandbox root is the file's parent directory.
/// - If `--source` is a directory (no `--output`): sandbox root is the directory itself.
///
/// # Errors
/// Returns an error if the output directory or source path cannot be canonicalized.
pub fn compute_sandbox_root(
    source_canonical: &Path,
    output_override: Option<&str>,
) -> Result<PathBuf> {
    if let Some(output_dir) = output_override {
        let out = Path::new(output_dir);
        if out.exists() {
            Ok(out.canonicalize()?)
        } else {
            fs::create_dir_all(out)?;
            Ok(out.canonicalize()?)
        }
    } else if source_canonical.is_file() {
        Ok(source_canonical
            .parent()
            .unwrap_or(source_canonical)
            .to_path_buf())
    } else {
        Ok(source_canonical.to_path_buf())
    }
}

/// Safe canonicalization for potentially non-existent paths.
///
/// Algorithm:
/// 1. Reject any raw `..` component anywhere in the path (checked before filesystem access).
/// 2. If the path already exists, canonicalize normally.
/// 3. Walk up parent components until an existing ancestor is found.
/// 4. Canonicalize that ancestor (resolves symlinks, `.`, `..`).
/// 5. Append the remaining suffix components.
/// 6. Returns the resulting path (not yet validated against sandbox).
///
/// # Errors
/// Returns an error if a `..` component appears anywhere in the path,
/// or if canonicalization fails for the existing ancestor.
pub fn safe_canonicalize_nonexistent(path: &Path) -> Result<PathBuf> {
    // Eagerly reject any .. component in the raw path before any filesystem ops.
    // This covers cases like /tmp/nonexistent/../escape where .. appears in the middle.
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            bail!(
                "Security error: path traversal via '..' is not permitted in output paths: {}",
                path.display()
            );
        }
    }

    if path.exists() {
        return Ok(path.canonicalize()?);
    }

    // Walk up to find the first existing ancestor
    let mut existing_ancestor = path.to_path_buf();
    let mut suffix_components: Vec<std::ffi::OsString> = Vec::new();

    loop {
        if existing_ancestor.exists() {
            break;
        }
        match existing_ancestor.file_name() {
            Some(name) => {
                suffix_components.push(name.to_owned());
            }
            None => {
                // Reached root without finding existing ancestor
                break;
            }
        }
        match existing_ancestor.parent() {
            Some(parent) => existing_ancestor = parent.to_path_buf(),
            None => break,
        }
    }

    let canonical_ancestor = if existing_ancestor.exists() {
        existing_ancestor.canonicalize()?
    } else {
        existing_ancestor
    };

    // Re-append suffix in original order (we built it in reverse)
    suffix_components.reverse();
    let mut result = canonical_ancestor;
    for component in suffix_components {
        result = result.join(component);
    }

    Ok(result)
}

/// Validate that the output path is within the sandbox boundary.
/// Returns the safe canonical path on success.
///
/// # Errors
/// Returns an error if the resolved path escapes the sandbox root.
#[allow(dead_code)]
pub fn validate_output_path_in_sandbox(
    output_path: &Path,
    sandbox_root: &Path,
    annotation_name: &str,
    source_file: &Path,
    dir_path: &str,
) -> Result<PathBuf> {
    let canonical = safe_canonicalize_nonexistent(output_path)?;

    if !canonical.starts_with(sandbox_root) {
        bail!(
            "Security error in {} - dir_path '{}' attempts to write outside sandbox boundary. \
            Resolved to: {}, but must be within: {}",
            source_file.display(),
            dir_path,
            canonical.display(),
            sandbox_root.display(),
            // annotation_name for diagnostics
        );
    }
    let _ = annotation_name;

    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_glob_pattern() {
        assert!(matches_glob_pattern(
            "src/tests/compile_fail/test.rs",
            "compile_fail"
        ));
        assert!(matches_glob_pattern(
            "tests/compile_fail/test.rs",
            "compile_fail"
        ));
        assert!(matches_glob_pattern("src/tests/foo.rs", "tests/*"));
        assert!(matches_glob_pattern("src/examples/bar.rs", "examples/*"));
        assert!(matches_glob_pattern("a/b/c/d/test.rs", "**/test.rs"));
    }

    #[test]
    fn test_matches_glob_pattern_literal_metacharacters() {
        // '+' is a regex quantifier but must be treated as a literal in glob patterns.
        assert!(matches_glob_pattern("src/foo+bar.rs", "foo+bar.rs"));
        assert!(!matches_glob_pattern("src/fooXbar.rs", "foo+bar.rs"));
        // '[' and ']' are regex character-class delimiters but must be literal.
        assert!(matches_glob_pattern("src/foo[0].rs", "foo[0].rs"));
        assert!(!matches_glob_pattern("src/foo0.rs", "foo[0].rs"));
    }

    #[test]
    fn test_is_in_auto_ignored_dir() {
        assert!(is_in_auto_ignored_dir(Path::new(
            "tests/compile_fail/test.rs"
        )));
        assert!(is_in_auto_ignored_dir(Path::new("src/compile_fail/foo.rs")));
        assert!(!is_in_auto_ignored_dir(Path::new("src/models.rs")));
        assert!(!is_in_auto_ignored_dir(Path::new("tests/integration.rs")));
    }

    #[test]
    fn test_has_ignore_directive() {
        assert!(has_ignore_directive("// gts:ignore\nuse foo::bar;"));
        assert!(has_ignore_directive("// GTS:IGNORE\nuse foo::bar;"));
        assert!(has_ignore_directive(
            "//! Module doc\n// gts:ignore\nuse foo::bar;"
        ));
        assert!(!has_ignore_directive("use foo::bar;\n// gts:ignore"));
        assert!(!has_ignore_directive("use foo::bar;"));
    }

    #[test]
    fn test_should_exclude_path_matching_pattern() {
        let patterns = vec!["test_*".to_owned(), "**/target/**".to_owned()];
        let path = Path::new("src/test_helper.rs");
        assert!(should_exclude_path(path, &patterns));
    }

    #[test]
    fn test_should_exclude_path_no_match() {
        let patterns = vec!["test_*".to_owned(), "**/compile_fail/**".to_owned()];
        let path = Path::new("src/main.rs");
        assert!(!should_exclude_path(path, &patterns));
    }

    #[test]
    fn test_safe_canonicalize_nonexistent_traversal_rejected() {
        // Build a path with .. in the non-existent suffix via a real temp dir.
        // TempDir exists on all platforms so canonicalize of the parent succeeds,
        // but the sub-path `nonexistent/../escape` contains `..` and must be rejected.
        let tmp = tempfile::TempDir::new().unwrap();
        let nonexistent = tmp.path().join("nonexistent").join("..").join("escape");
        let result = safe_canonicalize_nonexistent(&nonexistent);
        assert!(result.is_err(), "Should reject '..' in non-existent suffix");
    }
}
