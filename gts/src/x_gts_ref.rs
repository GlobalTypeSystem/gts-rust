/// x-gts-ref validation support for GTS schemas.
///
/// This module implements the `x-gts-ref` extension of GTS specification v0.14,
/// section 9.6.
///
/// # Overview
///
/// The `x-gts-ref` extension constrains a string value to a GTS identifier that
/// matches a given pattern, keeping references in GTS-based systems sound.
///
/// # Features
///
/// 1. **Schema Validation**: Validates that `x-gts-ref` declarations name a usable pattern
/// 2. **Instance Validation**: Validates that instance values match their `x-gts-ref` constraints
/// 3. **Registry Checks**: Presence and validity of the target, as far as
///    [`GtsRefValidation`] asks for
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
/// # x-gts-ref Operands
///
/// The `x-gts-ref` field can contain:
///
/// - **GTS ID Pattern**: A full or prefix GTS identifier (e.g., `gts.x.y._.z.v1~`),
///   including a wildcard pattern (e.g., `gts.x.y.*`)
/// - **Self-reference**: `/$id`, the identifier of the leaf type being validated,
///   even where the constraint is inherited from a base or trait schema (for a
///   schema validated on its own, its own `$id`). It is the only pointer operand
///   the spec allows; every other slash-prefixed value is rejected.
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

/// The one pointer operand a declaration may use (spec v0.14 §9.6).
const SELF_ID_POINTER: &str = "/$id";

/// How far `x-gts-ref` targets are checked (spec v0.14 §9.6).
///
/// Syntax and pattern conformance are checked in every mode; the mode only
/// decides how much the registry is consulted about the target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GtsRefValidation {
    /// Do not consult the registry.
    None,
    /// The target must be registered.
    AnyPresent,
    /// The target must be registered and itself valid.
    #[default]
    AnyValid,
}

impl GtsRefValidation {
    /// Parses the spelling used by the `gts-ref-validation` request parameter.
    ///
    /// # Errors
    /// Returns the rejected spelling when it names no mode.
    pub fn parse(spelling: &str) -> Result<Self, String> {
        match spelling {
            "none" => Ok(Self::None),
            "any-present" => Ok(Self::AnyPresent),
            "any-valid" => Ok(Self::AnyValid),
            other => Err(format!(
                "unknown gts-ref-validation mode '{other}': expected \
                 'none', 'any-present' or 'any-valid'"
            )),
        }
    }

    /// Whether the registry is consulted at all.
    #[must_use]
    pub fn checks_registry(self) -> bool {
        self != Self::None
    }

    /// Whether the target must itself validate.
    #[must_use]
    pub fn checks_validity(self) -> bool {
        self == Self::AnyValid
    }
}

/// Every usable `x-gts-ref` pattern the document declares, by location.
///
/// Unusable declarations are skipped: [`XGtsRefValidator::validate_schema`]
/// already reports them.
pub(crate) fn declared_patterns(schema: &Value) -> Vec<(String, GtsIdPattern)> {
    let selected = XGtsRefValidator::self_id(schema);
    let mut declared = Vec::new();
    crate::schema_modifiers::for_each_schema_node(schema, &mut |node, location| {
        if let Some(value) = node.get(X_GTS_REF)
            && let Ok(pattern) = resolve_declaration(value, selected.as_deref())
        {
            declared.push((declaration_path("", location), pattern));
        }
    });
    declared
}

/// The pattern a declaration denotes, or why it is not a usable declaration.
///
/// Accepts a GTS pattern or the `/$id` self-reference, which names `selected`:
/// the type being validated (spec v0.14 §9.6).
fn resolve_declaration(declared: &Value, selected: Option<&str>) -> Result<GtsIdPattern, String> {
    let Some(declared) = declared.as_str() else {
        return Err(format!("x-gts-ref value must be a string, got {declared}"));
    };

    if declared.starts_with(GTS_ID_PREFIX) {
        return GtsIdPattern::try_new(declared)
            .map_err(|e| format!("Invalid GTS identifier: {declared}: {}", e.cause));
    }

    if declared == SELF_ID_POINTER {
        let Some(resolved) = selected else {
            return Err(format!("Cannot resolve reference path '{declared}'"));
        };
        return GtsIdPattern::try_new(resolved).map_err(|e| {
            format!(
                "Resolved reference '{declared}' -> '{resolved}' is not a valid GTS identifier: {}",
                e.cause
            )
        });
    }

    Err(format!(
        "Invalid x-gts-ref value: '{declared}' must start with '{GTS_ID_PREFIX}' \
         or be the self-reference '{SELF_ID_POINTER}'"
    ))
}

/// Registers `x-gts-ref` as a native keyword for dialect-aware applicability.
///
/// Invalid declarations fail compilation. `/$id` names `selected`, the type
/// being validated, wherever in the compiled schema it is declared.
pub(crate) fn with_x_gts_ref(
    options: jsonschema::ValidationOptions,
    selected: Option<String>,
    exists: Option<ReferenceExists>,
) -> jsonschema::ValidationOptions {
    options.with_keyword(X_GTS_REF, move |_parent, declared, _location| {
        let pattern = resolve_declaration(declared, selected.as_deref())
            .map_err(jsonschema::ValidationError::schema)?;
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

impl<'i> jsonschema::Keyword<'i> for XGtsRefKeyword {
    fn validate(&self, instance: &'i Value) -> Result<(), jsonschema::ValidationError<'i>> {
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
        let selected = Self::self_id(root_schema.unwrap_or(schema));
        let mut errors = Vec::new();

        crate::schema_modifiers::for_each_schema_node(schema, &mut |node, location| {
            let Some(declared) = node.get(X_GTS_REF) else {
                return;
            };
            if let Err(reason) = resolve_declaration(declared, selected.as_deref()) {
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

    /// The document's own GTS identifier, as `/$id` denotes it when the
    /// document itself is the type being validated.
    pub(crate) fn self_id(schema: &Value) -> Option<String> {
        schema
            .pointer(SELF_ID_POINTER)
            .and_then(Value::as_str)
            .map(Self::strip_gts_uri_prefix)
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
