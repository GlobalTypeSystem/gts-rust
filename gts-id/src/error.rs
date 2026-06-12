//! The error type shared across all GTS identifier and wildcard parsing.

use std::fmt;

/// Pinpoints the `~`-delimited segment at fault within a GTS identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GtsIdSegmentError {
    /// 1-based segment number.
    pub num: usize,
    /// Byte offset of this segment within the full ID string.
    pub offset: usize,
    /// The raw segment string that failed parsing.
    pub segment: String,
}

/// Error from GTS identifier / wildcard parsing.
///
/// There is a single failure category — "this GTS string is invalid" —
/// described by [`cause`](Self::cause). [`segment`](Self::segment) is present
/// when the failure could be pinned to a specific `~`-delimited segment;
/// otherwise it is an identifier-level problem (prefix, case, length, wildcard
/// placement, the single-segment-instance rule, …).
///
/// The `gts` crate re-exports this type under its own name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GtsIdError {
    /// The full GTS string (identifier or wildcard pattern) that failed.
    pub input: String,
    /// Human-readable description of the problem.
    pub cause: String,
    /// Set when a specific segment is at fault.
    pub segment: Option<GtsIdSegmentError>,
}

impl GtsIdError {
    /// Build an identifier-level error (no specific segment located).
    #[must_use]
    pub fn new(input: impl Into<String>, cause: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            cause: cause.into(),
            segment: None,
        }
    }

    /// Attach the location of the offending `~`-segment.
    #[must_use]
    pub fn with_segment(mut self, num: usize, offset: usize, segment: impl Into<String>) -> Self {
        self.segment = Some(GtsIdSegmentError {
            num,
            offset,
            segment: segment.into(),
        });
        self
    }
}

impl fmt::Display for GtsIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.segment {
            Some(s) => write!(
                f,
                "Invalid GTS segment #{} @ offset {}: '{}': {}",
                s.num, s.offset, s.segment, self.cause
            ),
            None => write!(f, "Invalid GTS identifier: {}: {}", self.input, self.cause),
        }
    }
}

impl std::error::Error for GtsIdError {}
