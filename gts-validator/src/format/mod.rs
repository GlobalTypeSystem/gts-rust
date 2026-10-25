//! Format-specific scanners for GTS identifier discovery and validation.
//!
//! Each sub-module handles a specific file format:
//! - `markdown` — Markdown files with code-block state machine
//! - `json` — JSON tree-walker (also JSONC, after `gts::jsonc::strip`)
//! - `yaml` — YAML scanner (delegates to JSON walker via `serde_json::Value`)

pub mod json;
pub mod markdown;
pub mod yaml;
