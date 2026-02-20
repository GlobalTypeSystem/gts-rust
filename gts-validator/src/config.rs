//! Configuration types for GTS validation.
//!
//! Split into core validation config (universal) and source-specific config
//! (how content is discovered). This ensures the core API does not leak
//! filesystem concerns.

use std::path::PathBuf;

/// Vendor matching policy for GTS ID validation.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum VendorPolicy {
    /// Accept any vendor (no vendor enforcement).
    #[default]
    Any,
    /// All GTS IDs must match this exact vendor (example vendors are always tolerated).
    MustMatch(String),
    /// All GTS IDs must match one of the listed vendors (example vendors are always tolerated).
    AllowList(Vec<String>),
}

/// Controls how GTS identifier candidates are discovered in markdown files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiscoveryMode {
    /// Only match well-formed GTS patterns — fewer false positives (default).
    #[default]
    StrictSpecOnly,
    /// Permissive regex catches ALL gts.* strings including malformed IDs.
    /// Use for strict CI enforcement where every malformed ID must be reported.
    Heuristic,
}

/// Core validation config — applies regardless of input source.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ValidationConfig {
    /// Vendor matching policy for all GTS IDs.
    /// Example vendors (acme, globex, etc.) are always tolerated regardless of policy.
    pub vendor_policy: VendorPolicy,
    /// Scan JSON/YAML object keys for GTS identifiers (default: off).
    pub scan_keys: bool,
    /// Discovery mode for markdown scanning.
    ///
    /// - `StrictSpecOnly` (default): only well-formed GTS patterns are discovered.
    /// - `Heuristic`: a permissive regex catches ALL gts.* strings, including malformed IDs.
    pub discovery_mode: DiscoveryMode,
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
    /// Whether to follow symbolic links.
    ///
    /// **Defaults to `false`** — following symlinks allows escaping the repository
    /// root, traversing system directories, and reading secrets in CI environments.
    /// Only enable if you explicitly trust all symlinks in the repository.
    pub follow_links: bool,
    /// Maximum directory traversal depth (default: 64).
    /// Prevents infinite recursion via deeply nested symlinks or directories.
    pub max_depth: usize,
    /// Maximum total number of files to scan (default: `100_000`).
    /// Prevents memory exhaustion on pathological repositories.
    pub max_files: usize,
    /// Maximum total bytes to read across all files (default: 512 MB).
    /// Prevents memory exhaustion when many large files are present.
    pub max_total_bytes: u64,
}

impl Default for FsSourceConfig {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            exclude: Vec::new(),
            max_file_size: 10_485_760,
            follow_links: false,
            max_depth: 64,
            max_files: 100_000,
            max_total_bytes: 536_870_912,
        }
    }
}
