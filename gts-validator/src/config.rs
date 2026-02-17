//! Configuration types for GTS validation.
//!
//! Split into core validation config (universal) and source-specific config
//! (how content is discovered). This ensures the core API does not leak
//! filesystem concerns.

use std::path::PathBuf;

/// Core validation config — applies regardless of input source.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ValidationConfig {
    /// Expected vendor for all GTS IDs (e.g., "x").
    /// Example vendors (acme, globex, etc.) are always tolerated.
    pub vendor: Option<String>,
    /// Scan JSON/YAML object keys for GTS identifiers (default: off).
    pub scan_keys: bool,
    /// Enable relaxed discovery (catches more candidates, including malformed ones).
    ///
    /// - `false` (default): only well-formed GTS patterns are discovered — fewer false positives.
    /// - `true`: a permissive regex catches ALL gts.* strings, including malformed IDs,
    ///   so they can be reported as errors. Use this for strict CI enforcement.
    pub strict: bool,
    /// Additional skip tokens for markdown scanning.
    /// If any of these strings appear before a GTS candidate on the same line,
    /// validation is skipped for that candidate. Case-insensitive matching.
    /// Example: `vec!["**given**".to_owned()]` to skip BDD-style bold formatting.
    pub skip_tokens: Vec<String>,
}

/// Filesystem-specific source options.
///
/// NOTE: `paths` is required and must be non-empty. Default scan roots
/// (e.g. `docs/modules/libs/examples`) are a CLI/wrapper concern, not
/// baked into the library — keeps `gts-validator` repo-layout-agnostic.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FsSourceConfig {
    /// Paths to scan (files or directories). Required, must be non-empty.
    pub paths: Vec<PathBuf>,
    /// Exclude patterns (glob format).
    pub exclude: Vec<String>,
    /// Maximum file size in bytes (default: 10 MB).
    pub max_file_size: u64,
    /// Whether to follow symbolic links (default: true — preserves current behavior).
    pub follow_links: bool,
}

impl Default for FsSourceConfig {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            exclude: Vec::new(),
            max_file_size: 10_485_760,
            follow_links: true,
        }
    }
}
