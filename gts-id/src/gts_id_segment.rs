//! A single parsed segment of a GTS identifier.
//!
//! [`GtsIdSegment`] is the structured view of one segment (the part between
//! `~` markers): its parsed tokens plus its 1-based position (`num`) and
//! absolute byte `offset` within the original ID string. It is produced by the
//! parsers in [`crate::parse`]; callers obtain segments by parsing a full
//! [`GtsID`](crate::GtsID) or [`GtsWildcard`](crate::GtsWildcard), never by
//! constructing one directly.

/// Parsed GTS segment containing vendor, package, namespace, type, and version info.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct GtsIdSegment {
    pub num: usize,
    pub offset: usize,
    pub segment: String,
    pub vendor: String,
    pub package: String,
    pub namespace: String,
    pub type_name: String,
    pub ver_major: u32,
    pub ver_minor: Option<u32>,
    pub is_type: bool,
    pub is_wildcard: bool,
    pub is_uuid_tail: bool,
}
