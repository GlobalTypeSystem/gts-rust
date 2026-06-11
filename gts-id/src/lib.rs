//! Shared GTS ID parsing primitives.
//!
//! This crate provides the single source of truth for GTS identifier parsing
//! and validation, used by both the `gts` runtime library and the `gts-macros`
//! proc-macro crate.

mod error;
mod gts_id;
mod gts_id_segment;
mod gts_wildcard;
mod parse;

pub use error::{GtsIdError, GtsSegmentError};
pub use gts_id::GtsID;
pub use gts_id_segment::GtsIdSegment;
pub use gts_wildcard::GtsWildcard;
pub use parse::{GTS_MAX_LENGTH, GTS_PREFIX, parse_gts_string};
