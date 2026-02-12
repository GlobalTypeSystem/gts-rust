#![allow(clippy::unwrap_used, clippy::expect_used)]

use gts::GtsError;
use gts_macros::gts_error;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Test error structs
// ---------------------------------------------------------------------------

// Root error (no base)
#[gts_error(r#type = "gts.cf.core.errors.err.v1", status = 500, title = "Error")]
pub struct BaseError;

// Child errors (chained from BaseError)

#[gts_error(
    r#type = "cf.system.logical.not_found.v1",
    base = BaseError,
    status = 404,
    title = "Not Found",
)]
pub struct EntityNotFoundError {
    pub gts_id: String,
}

#[gts_error(
    r#type = "cf.system.logical.validation_failed.v1",
    base = BaseError,
    status = 422,
    title = "Validation Failed",
)]
pub struct ValidationFailedError {
    #[gts_error(as_errors)]
    pub violations: Vec<String>,
    pub entity_type: String,
}

#[gts_error(
    r#type = "cf.system.transport.connection_refused.v1",
    base = BaseError,
    status = 503,
    title = "Connection Refused",
)]
pub struct ConnectionRefusedError {
    pub target_host: String,
    #[gts_error(skip_metadata)]
    pub internal_trace: String,
}

#[gts_error(
    r#type = "cf.system.http.internal_server_error.v1",
    base = BaseError,
    status = 500,
    title = "Internal Server Error",
)]
pub struct InternalServerErrorType {
    #[gts_error(skip_metadata)]
    pub cause: String,
}

#[gts_error(
    r#type = "cf.system.http.rate_limited.v1",
    base = BaseError,
    status = 429,
    title = "Rate Limited",
)]
pub struct RateLimitedError {
    pub retry_after: u64,
}

#[gts_error(
    r#type = "cf.system.logical.conflict.v1",
    base = BaseError,
    status = 409,
    title = "Conflict",
)]
pub struct ConflictError {
    #[gts_error(skip_metadata)]
    pub debug_info: String,
    #[gts_error(skip_metadata)]
    pub stack_trace: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldViolation {
    pub field: String,
    pub message: String,
}

#[gts_error(
    r#type = "cf.system.logical.validation_failed_rich.v1",
    base = BaseError,
    status = 422,
    title = "Validation Failed",
)]
pub struct RichValidationError {
    #[gts_error(as_errors)]
    pub violations: Vec<FieldViolation>,
    pub entity_type: String,
    #[gts_error(skip_metadata)]
    pub raw_input: String,
}

// Unit struct without base (another root error)
#[gts_error(
    r#type = "gts.cf.core.errors.unit.v1",
    status = 500,
    title = "Unit Error"
)]
pub struct UnitRootError;

// ---------------------------------------------------------------------------
// Constants tests
// ---------------------------------------------------------------------------

#[test]
fn test_root_error_gts_id() {
    assert_eq!(BaseError::gts_id(), "gts://gts.cf.core.errors.err.v1~");
}

#[test]
fn test_child_error_gts_id() {
    assert_eq!(
        EntityNotFoundError::gts_id(),
        "gts://gts.cf.core.errors.err.v1~cf.system.logical.not_found.v1~"
    );
}

#[test]
fn test_gts_id_starts_with_gts_uri_prefix() {
    assert!(BaseError::gts_id().starts_with("gts://"));
    assert!(EntityNotFoundError::gts_id().starts_with("gts://"));
    assert!(ValidationFailedError::gts_id().starts_with("gts://"));
    assert!(ConnectionRefusedError::gts_id().starts_with("gts://"));
    assert!(InternalServerErrorType::gts_id().starts_with("gts://"));
}

#[test]
fn test_gts_id_ends_with_tilde() {
    assert!(BaseError::gts_id().ends_with('~'));
    assert!(EntityNotFoundError::gts_id().ends_with('~'));
    assert!(ValidationFailedError::gts_id().ends_with('~'));
    assert!(InternalServerErrorType::gts_id().ends_with('~'));
}

#[test]
fn test_gts_id_contains_base_segment() {
    assert!(EntityNotFoundError::gts_id().contains("gts.cf.core.errors.err.v1~"));
    assert!(ValidationFailedError::gts_id().contains("gts.cf.core.errors.err.v1~"));
}

#[test]
fn test_root_error_single_segment() {
    let id = BaseError::gts_id();
    let without_prefix = id.strip_prefix("gts://").unwrap();
    let without_trailing = without_prefix.strip_suffix('~').unwrap();
    let segments: Vec<&str> = without_trailing.split('~').collect();
    assert_eq!(
        segments.len(),
        1,
        "Root error should have 1 segment, got: {segments:?}"
    );
}

#[test]
fn test_child_error_two_segment_chain() {
    let id = EntityNotFoundError::gts_id();
    let without_prefix = id.strip_prefix("gts://").unwrap();
    let without_trailing = without_prefix.strip_suffix('~').unwrap();
    let segments: Vec<&str> = without_trailing.split('~').collect();
    assert_eq!(
        segments.len(),
        2,
        "Child error should have 2-segment chain, got: {segments:?}"
    );
}

#[test]
fn test_status_method() {
    assert_eq!(EntityNotFoundError::status(), http::StatusCode::NOT_FOUND);
    assert_eq!(
        ValidationFailedError::status(),
        http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        ConnectionRefusedError::status(),
        http::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        InternalServerErrorType::status(),
        http::StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        RateLimitedError::status(),
        http::StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(ConflictError::status(), http::StatusCode::CONFLICT);
}

#[test]
fn test_title_constant() {
    assert_eq!(EntityNotFoundError::TITLE, "Not Found");
    assert_eq!(ValidationFailedError::TITLE, "Validation Failed");
    assert_eq!(ConnectionRefusedError::TITLE, "Connection Refused");
    assert_eq!(InternalServerErrorType::TITLE, "Internal Server Error");
    assert_eq!(RateLimitedError::TITLE, "Rate Limited");
}

// ---------------------------------------------------------------------------
// GtsError trait tests
// ---------------------------------------------------------------------------

#[test]
fn test_trait_gts_id_matches_method() {
    assert_eq!(
        <EntityNotFoundError as GtsError>::gts_id(),
        EntityNotFoundError::gts_id()
    );
}

#[test]
fn test_trait_status_matches_method() {
    assert_eq!(
        <EntityNotFoundError as GtsError>::status(),
        EntityNotFoundError::status()
    );
}

#[test]
fn test_trait_title_matches_constant() {
    assert_eq!(
        <EntityNotFoundError as GtsError>::TITLE,
        EntityNotFoundError::TITLE
    );
}

// ---------------------------------------------------------------------------
// Display trait tests
// ---------------------------------------------------------------------------

#[test]
fn test_display_format() {
    let err = EntityNotFoundError {
        gts_id: "some-entity-123".to_owned(),
    };
    let display = format!("{err}");
    assert_eq!(
        display,
        "Not Found: gts://gts.cf.core.errors.err.v1~cf.system.logical.not_found.v1~"
    );
}

#[test]
fn test_display_root_error() {
    let err = BaseError;
    let display = format!("{err}");
    assert_eq!(display, "Error: gts://gts.cf.core.errors.err.v1~");
}

#[test]
fn test_display_unit_root_error() {
    let err = UnitRootError;
    let display = format!("{err}");
    assert_eq!(display, "Unit Error: gts://gts.cf.core.errors.unit.v1~");
}

// ---------------------------------------------------------------------------
// std::error::Error trait tests
// ---------------------------------------------------------------------------

#[test]
fn test_implements_error_trait() {
    let err = EntityNotFoundError {
        gts_id: "test".to_owned(),
    };
    // Verify it can be used as a &dyn std::error::Error
    let dyn_err: &dyn std::error::Error = &err;
    assert!(dyn_err.to_string().contains("Not Found"));
}

#[test]
fn test_implements_error_trait_unit_struct() {
    let err = BaseError;
    let dyn_err: &dyn std::error::Error = &err;
    assert!(dyn_err.to_string().contains("Error"));
}

// ---------------------------------------------------------------------------
// Metadata tests
// ---------------------------------------------------------------------------

#[test]
fn test_metadata_basic_field() {
    let err = EntityNotFoundError {
        gts_id: "entity-42".to_owned(),
    };
    let metadata = err.error_metadata().expect("should have metadata");
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata.get("gts_id").unwrap(), "entity-42");
}

#[test]
fn test_metadata_multiple_fields() {
    let err = RateLimitedError { retry_after: 30 };
    let metadata = err.error_metadata().expect("should have metadata");
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata.get("retry_after").unwrap(), 30);
}

#[test]
fn test_metadata_skip_metadata() {
    let err = ConnectionRefusedError {
        target_host: "db.example.com".to_owned(),
        internal_trace: "sensitive-stack-trace".to_owned(),
    };
    let metadata = err.error_metadata().expect("should have metadata");
    assert_eq!(metadata.len(), 1);
    assert!(metadata.contains_key("target_host"));
    assert!(
        !metadata.contains_key("internal_trace"),
        "skip_metadata field should be excluded"
    );
}

#[test]
fn test_metadata_as_errors() {
    let err = ValidationFailedError {
        violations: vec![
            "field 'name' is required".to_owned(),
            "field 'email' is invalid".to_owned(),
        ],
        entity_type: "User".to_owned(),
    };
    let metadata = err.error_metadata().expect("should have metadata");
    assert_eq!(metadata.len(), 2);
    assert!(
        metadata.contains_key("errors"),
        "as_errors field should be under 'errors' key"
    );
    assert!(metadata.contains_key("entity_type"));

    let errors = metadata.get("errors").unwrap().as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0], "field 'name' is required");
}

#[test]
fn test_metadata_as_errors_with_complex_type() {
    let err = RichValidationError {
        violations: vec![FieldViolation {
            field: "email".to_owned(),
            message: "invalid format".to_owned(),
        }],
        entity_type: "User".to_owned(),
        raw_input: "should-be-excluded".to_owned(),
    };
    let metadata = err.error_metadata().expect("should have metadata");
    assert_eq!(metadata.len(), 2);
    assert!(metadata.contains_key("errors"));
    assert!(metadata.contains_key("entity_type"));
    assert!(
        !metadata.contains_key("raw_input"),
        "skip_metadata field should be excluded"
    );

    let errors = metadata.get("errors").unwrap().as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["field"], "email");
    assert_eq!(errors[0]["message"], "invalid format");
}

#[test]
fn test_metadata_unit_struct_returns_none() {
    let err = BaseError;
    assert!(err.error_metadata().is_none());
}

#[test]
fn test_metadata_child_with_all_skipped_returns_none() {
    let err = InternalServerErrorType {
        cause: "something".to_owned(),
    };
    assert!(
        err.error_metadata().is_none(),
        "all fields are skip_metadata, should return None"
    );
}

#[test]
fn test_metadata_all_fields_skipped_returns_none() {
    let err = ConflictError {
        debug_info: "some debug".to_owned(),
        stack_trace: "some trace".to_owned(),
    };
    assert!(
        err.error_metadata().is_none(),
        "all fields are skip_metadata, should return None"
    );
}

// ---------------------------------------------------------------------------
// Debug / Clone derive tests
// ---------------------------------------------------------------------------

#[test]
fn test_debug_is_derived() {
    let err = EntityNotFoundError {
        gts_id: "test".to_owned(),
    };
    let debug = format!("{err:?}");
    assert!(debug.contains("EntityNotFoundError"));
    assert!(debug.contains("test"));
}

#[test]
fn test_clone_is_derived() {
    let err = EntityNotFoundError {
        gts_id: "test".to_owned(),
    };
    let cloned = err.clone();
    assert_eq!(
        cloned.error_metadata().unwrap().get("gts_id"),
        err.error_metadata().unwrap().get("gts_id")
    );
}

// ---------------------------------------------------------------------------
// StatusCode tests
// ---------------------------------------------------------------------------

#[test]
fn test_status_as_u16() {
    assert_eq!(EntityNotFoundError::status().as_u16(), 404);
    assert_eq!(BaseError::status().as_u16(), 500);
    assert_eq!(RateLimitedError::status().as_u16(), 429);
}

// ---------------------------------------------------------------------------
// Compile-fail tests
// ---------------------------------------------------------------------------

#[test]
fn test_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/gts_error_*.rs");
}
