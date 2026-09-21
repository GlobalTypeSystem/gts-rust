/// x-gts-ref validation support for GTS schemas.
///
/// This module implements validation for the `x-gts-ref` extension as specified
/// in the GTS specification v0.5, section 9.5.
///
/// # Overview
///
/// The `x-gts-ref` extension allows schemas to enforce that string values must be
/// valid GTS identifiers or match specific patterns. This is useful for ensuring
/// referential integrity in GTS-based systems.
///
/// # Features
///
/// 1. **Schema Validation**: Validates that `x-gts-ref` fields in schemas contain valid patterns
/// 2. **Instance Validation**: Validates that instance values match their `x-gts-ref` constraints
/// 3. **JSON Pointer Resolution**: Supports JSON Pointer references (e.g., `/$id`, `/properties/name`)
/// 4. **GTS ID Pattern Matching**: Validates GTS IDs and prefix patterns (e.g., `gts.x.y._.z.v1~`)
///
/// # Examples
///
/// ## Schema with x-gts-ref
///
/// ```json
/// {
///   "$id": "gts://gts.x.example._.user.v1~",
///   "$schema": "http://json-schema.org/draft-07/schema#",
///   "type": "object",
///   "properties": {
///     "id": {
///       "type": "string",
///       "x-gts-ref": "/$id"
///     },
///     "role": {
///       "type": "string",
///       "x-gts-ref": "gts.x.example._.role.v1~"
///     }
///   }
/// }
/// ```
///
/// ## Usage
///
/// ```rust
/// use gts::XGtsRefValidator;
/// use serde_json::json;
///
/// let validator = XGtsRefValidator::new();
///
/// // Validate a schema
/// let schema = json!({
///     "$id": "gts://gts.x.test._.schema.v1~",
///     "$schema": "http://json-schema.org/draft-07/schema#",
///     "type": "object",
///     "properties": {
///         "id": {"type": "string", "x-gts-ref": "/$id"}
///     }
/// });
/// let errors = validator.validate_schema(&schema, "", None);
/// assert!(errors.is_empty());
///
/// // Validate an instance - note: the value must match $id WITHOUT the gts:// prefix
/// let instance = json!({"id": "gts.x.test._.schema.v1~"});
/// let errors = validator.validate_instance(&instance, &schema, "");
/// assert!(errors.is_empty());
/// ```
///
/// # x-gts-ref Patterns
///
/// The `x-gts-ref` field can contain:
///
/// - **GTS ID Pattern**: A full or prefix GTS identifier (e.g., `gts.x.y._.z.v1~`)
/// - **JSON Pointer**: A reference to another field in the schema (e.g., `/$id`, `/properties/name`)
///
/// ## JSON Pointer Resolution
///
/// When an `x-gts-ref` starts with `/`, it's treated as a JSON Pointer that resolves
/// to a value in the schema. The resolved value must be a valid GTS ID pattern.
///
/// Example:
/// ```json
/// {
///   "$id": "gts://gts.x.example._.user.v1~",
///   "$schema": "http://json-schema.org/draft-07/schema#",
///   "type": "object",
///   "properties": {
///     "type": {"type": "string", "x-gts-ref": "/$id"}
///   }
/// }
/// ```
///
/// In this case, the `type` field must match the schema's `$id` value.
use std::sync::Arc;

use jsonschema::error::ValidationErrorKind;
use serde_json::Value;
use std::fmt;

use crate::gts::{GTS_ID_PREFIX, GTS_ID_URI_PREFIX, GtsId, GtsIdPattern};
use crate::schema_modifiers::X_GTS_REF;

/// Error type for x-gts-ref validation failures
#[derive(Debug, Clone)]
pub struct XGtsRefValidationError {
    pub field_path: String,
    pub value: String,
    pub ref_pattern: String,
    pub reason: String,
}

impl XGtsRefValidationError {
    #[must_use]
    pub fn new(field_path: String, value: String, ref_pattern: String, reason: String) -> Self {
        Self {
            field_path,
            value,
            ref_pattern,
            reason,
        }
    }
}

impl fmt::Display for XGtsRefValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "x-gts-ref validation failed for field '{}': {}",
            self.field_path, self.reason
        )
    }
}

impl std::error::Error for XGtsRefValidationError {}

/// Checks whether a pattern-matching reference names a registered entity.
///
/// Callers precompute results because validation cannot borrow the store.
pub(crate) type ReferenceExists = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Collects strings for prefetching reference existence.
pub(crate) fn candidate_reference_values(instance: &Value) -> Vec<String> {
    let mut found = std::collections::BTreeSet::new();
    let mut pending = vec![instance];
    while let Some(node) = pending.pop() {
        match node {
            Value::String(text) => {
                found.insert(text.clone());
            }
            Value::Object(map) => pending.extend(map.values()),
            Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    found.into_iter().collect()
}

/// The pattern a declaration denotes, or why it is not a usable declaration.
///
/// Accepts a GTS pattern or a JSON Pointer resolving to one.
fn resolve_declaration(declared: &Value, root: &Value) -> Result<GtsIdPattern, String> {
    let Some(declared) = declared.as_str() else {
        return Err(format!("x-gts-ref value must be a string, got {declared}"));
    };

    if declared.starts_with(GTS_ID_PREFIX) {
        return GtsIdPattern::try_new(declared)
            .map_err(|e| format!("Invalid GTS identifier: {declared}: {}", e.cause));
    }

    if declared.starts_with('/') {
        let Some(resolved) = XGtsRefValidator::resolve_pointer(root, declared) else {
            return Err(format!("Cannot resolve reference path '{declared}'"));
        };
        return GtsIdPattern::try_new(&resolved).map_err(|e| {
            format!(
                "Resolved reference '{declared}' -> '{resolved}' is not a valid GTS identifier: {}",
                e.cause
            )
        });
    }

    Err(format!(
        "Invalid x-gts-ref value: '{declared}' must start with '{GTS_ID_PREFIX}' or '/'"
    ))
}

/// Registers `x-gts-ref` as a native keyword for dialect-aware applicability.
///
/// Invalid declarations fail compilation. Relative references resolve in `root`.
pub(crate) fn with_x_gts_ref(
    options: jsonschema::ValidationOptions,
    root: &Value,
    exists: Option<ReferenceExists>,
) -> jsonschema::ValidationOptions {
    let root = Arc::new(root.clone());
    options.with_keyword(X_GTS_REF, move |_parent, declared, _location| {
        let pattern =
            resolve_declaration(declared, &root).map_err(jsonschema::ValidationError::schema)?;
        Ok(Box::new(XGtsRefKeyword {
            pattern,
            exists: exists.clone(),
        }))
    })
}

/// One compiled `x-gts-ref` declaration.
struct XGtsRefKeyword {
    /// The pattern the declaration resolved to.
    pattern: GtsIdPattern,
    exists: Option<ReferenceExists>,
}

impl XGtsRefKeyword {
    /// Returns the violation for a string instance; ignores other value types.
    fn violation(&self, instance: &Value) -> Option<String> {
        let value = instance.as_str()?;
        let pattern = self.pattern.pattern();

        let Ok(id) = GtsId::try_new(value) else {
            return Some(format!("Value '{value}' is not a valid GTS identifier"));
        };
        if !id.matches_pattern(&self.pattern) {
            return Some(format!(
                "Value '{value}' does not match pattern '{pattern}'"
            ));
        }
        // Existence participates in branch selection like any other constraint.
        match &self.exists {
            Some(exists) if !exists(value) => Some(format!(
                "'{value}' references an entity that is not registered"
            )),
            _ => None,
        }
    }
}

impl jsonschema::Keyword for XGtsRefKeyword {
    fn validate<'i>(&self, instance: &'i Value) -> Result<(), jsonschema::ValidationError<'i>> {
        match self.violation(instance) {
            Some(reason) => Err(jsonschema::ValidationError::custom(reason)),
            None => Ok(()),
        }
    }

    fn is_valid(&self, instance: &Value) -> bool {
        self.violation(instance).is_none()
    }
}

/// Whether `error` was raised by the `x-gts-ref` keyword rather than by the
/// standard vocabulary.
pub(crate) fn is_x_gts_ref_error(error: &jsonschema::ValidationError<'_>) -> bool {
    matches!(error.kind(), ValidationErrorKind::Custom { keyword, .. } if keyword == X_GTS_REF)
}

/// Extracts reference violations only when they fully explain `error`.
fn attributed_refs(
    schema: &Value,
    error: &jsonschema::ValidationError<'_>,
) -> Option<Vec<XGtsRefValidationError>> {
    if is_x_gts_ref_error(error) {
        return Some(vec![describe_error(schema, error)]);
    }

    // Multiple matching branches are a composition error, not a ref error.
    let (ValidationErrorKind::AnyOf { context: branches }
    | ValidationErrorKind::OneOfNotValid { context: branches }
    | ValidationErrorKind::OneOfMultipleValid { context: branches }) = error.kind()
    else {
        return None;
    };

    let mut attributed = Vec::new();
    for cause in branches.iter().flatten() {
        attributed.extend(attributed_refs(schema, cause)?);
    }
    (!attributed.is_empty()).then_some(attributed)
}

/// Splits standard and `x-gts-ref` diagnostics.
pub(crate) fn split_errors<'i>(
    schema: &Value,
    errors: impl Iterator<Item = jsonschema::ValidationError<'i>>,
) -> (Vec<String>, Vec<XGtsRefValidationError>) {
    let mut standard = Vec::new();
    let mut references = Vec::new();
    for error in errors {
        match attributed_refs(schema, &error) {
            Some(attributed) => references.extend(attributed),
            None => standard.push(crate::json_schema::render_error(&error)),
        }
    }
    (standard, references)
}

/// Builds an `x-gts-ref` diagnostic from the validator error paths.
fn describe_error(
    schema: &Value,
    error: &jsonschema::ValidationError<'_>,
) -> XGtsRefValidationError {
    let declared = schema
        .pointer(error.schema_path().as_str())
        .and_then(Value::as_str)
        .unwrap_or_default();
    let value = match error.instance().as_ref() {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    XGtsRefValidationError::new(
        error.instance_path().to_string(),
        value,
        declared.to_owned(),
        error.to_string(),
    )
}

/// Reports applicable `x-gts-ref` violations and fails closed.
///
/// `exists` enables store-aware checks; `None` checks patterns only.
pub(crate) fn validate_instance_refs(
    instance: &Value,
    schema: &Value,
    instance_path: &str,
    exists: Option<ReferenceExists>,
) -> Vec<XGtsRefValidationError> {
    let validator = match crate::json_schema::gts_validator_for(schema, exists) {
        Ok(validator) => validator,
        Err(e) => {
            return vec![XGtsRefValidationError::new(
                instance_path.to_owned(),
                String::new(),
                String::new(),
                format!("x-gts-ref checking needs a compilable schema: {e}"),
            )];
        }
    };

    let diagnosis = crate::json_schema::diagnose(&validator, schema, instance);
    let mut references = diagnosis.references;

    // An unexplained rejection must not look like a clean reference check.
    if let Some(reason) = diagnosis.unexplained {
        references.push(XGtsRefValidationError::new(
            instance_path.to_owned(),
            String::new(),
            String::new(),
            format!("the references here were not verified: {reason}"),
        ));
        return references;
    }

    if instance_path.is_empty() {
        return references;
    }
    references
        .into_iter()
        .map(|mut error| {
            error.field_path = format!("{instance_path}{}", error.field_path);
            error
        })
        .collect()
}

/// Joins the caller prefix, subschema location, and keyword name.
fn declaration_path(prefix: &str, location: &str) -> String {
    [prefix, location, X_GTS_REF]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, Clone, Copy, Default)]
pub struct XGtsRefValidator;

// These methods take &self for API consistency even though XGtsRefValidator is zero-sized.
// This allows future extension with state if needed.
#[allow(clippy::unused_self, clippy::trivially_copy_pass_by_ref)]
impl XGtsRefValidator {
    /// Create a new validator
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Validate an instance against x-gts-ref constraints in schema
    ///
    /// # Arguments
    /// * `instance` - The data instance to validate
    /// * `schema` - The JSON schema with x-gts-ref extensions
    /// * `instance_path` - Prefix for the reported instance locations
    ///
    /// # Returns
    /// List of validation errors (empty if valid)
    #[must_use]
    pub fn validate_instance(
        &self,
        instance: &Value,
        schema: &Value,
        instance_path: &str,
    ) -> Vec<XGtsRefValidationError> {
        validate_instance_refs(instance, schema, instance_path, None)
    }

    /// Validate x-gts-ref declarations in a schema definition
    ///
    /// Visits only schema positions defined by the document's dialect.
    ///
    /// # Arguments
    /// * `schema` - The JSON schema to validate
    /// * `schema_path` - Prefix for the reported declaration locations
    /// * `root_schema` - The root schema (for resolving relative refs)
    ///
    /// # Returns
    /// List of validation errors (empty if valid)
    #[must_use]
    pub fn validate_schema(
        &self,
        schema: &Value,
        schema_path: &str,
        root_schema: Option<&Value>,
    ) -> Vec<XGtsRefValidationError> {
        let root = root_schema.unwrap_or(schema);
        let mut errors = Vec::new();

        crate::schema_modifiers::for_each_schema_node(schema, &mut |node, location| {
            let Some(declared) = node.get(X_GTS_REF) else {
                return;
            };
            if let Err(reason) = resolve_declaration(declared, root) {
                // Non-string declarations have no pattern to report.
                let (value, ref_pattern) = declared.as_str().map_or_else(
                    || (format!("{declared:?}"), String::new()),
                    |spelling| (spelling.to_owned(), spelling.to_owned()),
                );
                errors.push(XGtsRefValidationError::new(
                    declaration_path(schema_path, location),
                    value,
                    ref_pattern,
                    reason,
                ));
            }
        });

        errors
    }

    /// Resolve a JSON Pointer against the schema root.
    ///
    /// Uses `serde_json`'s RFC 6901 implementation, including arrays and escapes.
    ///
    /// # Returns
    /// The resolved value as a string or None if not found.
    /// Note: For `/$id` references, the `gts://` prefix is stripped from the value
    /// as per GTS specification (relative self-reference should match the $id without the prefix).
    fn resolve_pointer(schema: &Value, pointer: &str) -> Option<String> {
        Self::resolve_pointer_inner(schema, pointer, 0)
    }

    /// Depth-guarded pointer resolution: relative `x-gts-ref` hops recurse here,
    /// and a self-referential chain would overflow the stack without the cap.
    fn resolve_pointer_inner(schema: &Value, pointer: &str, depth: usize) -> Option<String> {
        const MAX_POINTER_DEPTH: usize = 64;
        if depth > MAX_POINTER_DEPTH {
            return None;
        }

        let current = schema.pointer(pointer)?;

        // If current is a string, return it (stripping gts:// prefix if present)
        if let Some(s) = current.as_str() {
            return Some(Self::strip_gts_uri_prefix(s));
        }

        // If current is an object with x-gts-ref, resolve it
        if let Some(obj) = current.as_object()
            && let Some(ref_value) = obj.get(X_GTS_REF)
            && let Some(ref_str) = ref_value.as_str()
        {
            if ref_str.starts_with('/') {
                return Self::resolve_pointer_inner(schema, ref_str, depth + 1);
            }
            return Some(ref_str.to_owned());
        }

        None
    }

    /// Strip the `gts://` prefix from a value if present.
    ///
    /// This is used for `/$id` relative references where the schema's `$id` field
    /// contains a full GTS URI (e.g., `gts://gts.x.example._.user.v1~`) but the
    /// instance value should match without the prefix (e.g., `gts.x.example._.user.v1~`).
    fn strip_gts_uri_prefix(value: &str) -> String {
        value
            .strip_prefix(GTS_ID_URI_PREFIX)
            .unwrap_or(value)
            .to_owned()
    }
}

#[cfg(test)]
#[path = "x_gts_ref_test.rs"]
mod x_gts_ref_test;
