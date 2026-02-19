//! Validation source strategies.
//!
//! Phase 1 provides only the filesystem strategy (`fs` module) with a concrete
//! `validate_fs()` public API. A `ValidationSource` trait may be introduced in
//! a future phase when a second concrete strategy demands it — until then, the
//! design stays concrete to avoid speculative abstraction.

pub mod fs;

/// Content format for dispatching to the correct scanner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentFormat {
    Markdown,
    Json,
    Yaml,
}
