//! Shared GTS ID parsing primitives.
//!
//! This crate provides the single source of truth for GTS identifier parsing
//! and validation, used by both the `gts` runtime library and the `gts-macros`
//! proc-macro crate.

mod error;
mod gts_id;
mod gts_id_pattern;
mod gts_id_segment;
pub(crate) mod parse;
pub(crate) mod prefix;

pub use error::{GtsIdError, GtsIdSegmentError};
pub use gts_id::GtsId;
pub use gts_id_pattern::GtsIdPattern;
pub use gts_id_segment::{GtsIdPatternSegment, GtsIdSegment, GtsIdSegmentParts, GtsUuidTail};
pub use parse::GTS_ID_MAX_LENGTH;
pub use prefix::{DEFAULT_GTS_ID_PREFIX, GTS_ID_PREFIX, GTS_ID_PREFIX_ENV};

/// Default JSON member names for an instance ID, in lookup order.
pub const DEFAULT_ENTITY_ID_FIELDS: &[&str] = &[
    "$id", "gtsId", "gtsIid", "gtsOid", "gtsI", "gts_id", "gts_oid", "gts_iid", "id",
];

/// Default JSON member names for an instance's type ID, in lookup order.
pub const DEFAULT_TYPE_ID_FIELDS: &[&str] = &[
    "gtsTid", "gtsType", "gtsT", "gts_t", "gts_tid", "gts_type", "type", "schema",
];
