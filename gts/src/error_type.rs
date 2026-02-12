//! GTS error type trait for compile-time error definitions.
//!
//! This module provides the `GtsError` trait which enables compile-time
//! error type definitions using the `#[gts_error]` proc macro. Error types
//! are classified using GTS identifiers following the two-segment chain model.
//!
//! # Example
//!
//! ```ignore
//! use gts::GtsError;
//!
//! #[gts_error(
//!     r#type = "gts.cf.core.errors.err.v1",
//!     status = 500,
//!     title = "Error",
//! )]
//! pub struct BaseError;
//!
//! #[gts_error(
//!     r#type = "cf.types_registry.entity.not_found.v1",
//!     base = BaseError,
//!     status = 404,
//!     title = "Entity Not Found",
//! )]
//! pub struct EntityNotFoundError { pub gts_id: String }
//!
//! assert_eq!(BaseError::gts_id(), "gts://gts.cf.core.errors.err.v1~");
//! assert_eq!(EntityNotFoundError::gts_id(), "gts://gts.cf.core.errors.err.v1~cf.types_registry.entity.not_found.v1~");
//! assert_eq!(EntityNotFoundError::STATUS, 404);
//! assert_eq!(EntityNotFoundError::TITLE, "Entity Not Found");
//! ```

use std::collections::HashMap;

/// Trait for types that represent a GTS error type definition.
///
/// This trait is automatically implemented by the `#[gts_error]` proc macro.
/// It provides the error's GTS identity, HTTP status, title, and a method
/// to extract sanitized metadata from struct fields.
///
/// Consuming crates (e.g., `modkit-errors`) can use this trait to build
/// RFC 9457 Problem Details responses and error registration metadata.
pub trait GtsError: std::fmt::Display + std::fmt::Debug {
    /// Full GTS type URI for this error.
    ///
    /// For root errors: `gts://{type}~`
    /// For child errors: `gts://{base_chain}{type}~`
    ///
    /// Example: `gts://gts.cf.core.errors.err.v1~cf.system.logical.not_found.v1~`
    #[must_use]
    fn gts_id() -> &'static str;

    /// HTTP status code for this error type.
    ///
    /// The value is guaranteed to be valid because the `#[gts_error]` macro
    /// validates the status code at compile time (100–599).
    #[must_use]
    fn status() -> http::StatusCode;

    /// Static human-readable title for this error type.
    const TITLE: &'static str;

    /// Collect sanitized metadata from struct fields.
    ///
    /// Fields annotated with `#[gts_error(skip_metadata)]` are excluded.
    /// Fields annotated with `#[gts_error(as_errors)]` are placed under the `"errors"` key.
    /// All other fields are serialized as key-value pairs.
    ///
    /// Returns `None` if the struct has no metadata-eligible fields.
    #[must_use]
    fn error_metadata(&self) -> Option<HashMap<String, serde_json::Value>>;
}
