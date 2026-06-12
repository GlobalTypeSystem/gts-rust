//! Shared GTS ID parsing primitives.
//!
//! This crate provides the single source of truth for GTS identifier parsing
//! and validation, used by both the `gts` runtime library and the `gts-macros`
//! proc-macro crate.

mod error;
mod gts_id;
mod gts_id_segment;
mod gts_id_wildcard;
mod parse;

pub use error::{GtsIdError, GtsIdSegmentError};
pub use gts_id::GtsId;
pub use gts_id_segment::{GtsIdSegment, GtsIdSegmentParts};
pub use gts_id_wildcard::GtsIdWildcard;
pub use parse::{GTS_MAX_LENGTH, GTS_PREFIX, parse_gts_id};
