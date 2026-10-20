//! Entity references: `x-gts-type-ref` and `x-gts-instance-ref`
//! (README §9.6.1, §9.6.3; ADR-0008).
//!
//! Both keywords constrain a string to a concrete GTS identifier selected by
//! their operand; `x-gts-type-ref` requires a Type ID and `x-gts-instance-ref`
//! an Instance ID. Syntax and matching come from `gts-id`; this module adds the
//! operand contract, the candidate kind, registry lookup, and the JSON Schema
//! integration.
//! Shared diagnostics also cover `x-gts-pattern-ref`, whose checks are defined
//! in `x_gts_pattern_ref` and never consult the registry.
//!
//! # Operands
//!
//! - A concrete Type ID or a §10 wildcard pattern, for either keyword.
//! - A concrete Instance ID (UUID-tailed included), for `x-gts-instance-ref`
//!   only: it selects no types.
//! - `/$id`, the leaf GTS Type Schema selected for validation, even where the
//!   constraint is inherited from a base or trait schema. It must resolve to a
//!   concrete Type ID.
//!
//! Every other operand is a schema error, wherever it is declared. The removed
//! `x-gts-ref` keyword is an unknown `x-gts-*` keyword.
//!
//! # Usage
//!
//! ```rust
//! use gts::XGtsRefValidator;
//! use serde_json::json;
//!
//! let validator = XGtsRefValidator::new();
//! let schema = json!({
//!     "$id": "gts://gts.x.test._.schema.v1~",
//!     "$schema": "http://json-schema.org/draft-07/schema#",
//!     "type": "object",
//!     "properties": {
//!         "type": {"type": "string", "x-gts-type-ref": "/$id"},
//!         "role": {"type": "string", "x-gts-instance-ref": "gts.x.test._.role.v1~"}
//!     }
//! });
//! assert!(validator.validate_schema(&schema, "", None).is_empty());
//!
//! let instance = json!({
//!     "type": "gts.x.test._.schema.v1~",
//!     "role": "gts.x.test._.role.v1~x.test._.admin.v1"
//! });
//! assert!(validator.validate_instance(&instance, &schema, "").is_empty());
//!
//! // A Type ID is the wrong kind for an instance reference.
//! let wrong_kind = json!({"role": "gts.x.test._.role.v1~"});
//! assert!(!validator.validate_instance(&wrong_kind, &schema, "").is_empty());
//! ```
use std::sync::Arc;

use jsonschema::error::ValidationErrorKind;
use serde_json::Value;
use std::fmt;

use crate::gts::{GTS_ID_MAX_LENGTH, GTS_ID_URI_PREFIX, GtsId, GtsIdPattern};

/// Constrains a string to a GTS Type Identifier.
pub const X_GTS_TYPE_REF: &str = "x-gts-type-ref";
/// Constrains a string to a GTS Instance Identifier.
pub const X_GTS_INSTANCE_REF: &str = "x-gts-instance-ref";
/// Both entity-reference keywords.
pub(crate) const ENTITY_REF_KEYWORDS: [&str; 2] = [X_GTS_TYPE_REF, X_GTS_INSTANCE_REF];

/// The GTS reference keyword responsible for a validation violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GtsRefKeyword {
    /// `x-gts-type-ref`: a concrete GTS Type Identifier.
    Type,
    /// `x-gts-instance-ref`: a concrete GTS Instance Identifier.
    Instance,
    /// `x-gts-pattern-ref`: a GTS identifier pattern.
    Pattern,
}

impl GtsRefKeyword {
    /// The keyword's JSON Schema spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Type => X_GTS_TYPE_REF,
            Self::Instance => X_GTS_INSTANCE_REF,
            Self::Pattern => crate::x_gts_pattern_ref::X_GTS_PATTERN_REF,
        }
    }

    /// Recognizes a GTS reference keyword by its JSON Schema spelling.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            X_GTS_TYPE_REF => Some(Self::Type),
            X_GTS_INSTANCE_REF => Some(Self::Instance),
            crate::x_gts_pattern_ref::X_GTS_PATTERN_REF => Some(Self::Pattern),
            _ => None,
        }
    }
}

impl fmt::Display for GtsRefKeyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The identifier kind an entity reference requires of its candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EntityRefKind {
    /// `x-gts-type-ref`: a Type ID, ending in `~`.
    Type,
    /// `x-gts-instance-ref`: an Instance ID.
    Instance,
}

impl EntityRefKind {
    /// Both kinds, in keyword order.
    pub const ALL: [Self; 2] = [Self::Type, Self::Instance];

    /// The keyword requiring this kind.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Type => X_GTS_TYPE_REF,
            Self::Instance => X_GTS_INSTANCE_REF,
        }
    }

    /// The kind `keyword` requires, if it is an entity-reference keyword.
    #[must_use]
    pub fn of_keyword(keyword: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.keyword() == keyword)
    }

    /// Whether `id` has this kind. Determined after parsing (ADR-0008).
    #[must_use]
    pub fn admits(self, id: &GtsId) -> bool {
        id.is_type() == (self == Self::Type)
    }

    const fn noun(self) -> &'static str {
        match self {
            Self::Type => "GTS Type Identifier",
            Self::Instance => "GTS Instance Identifier",
        }
    }

    const fn other(self) -> Self {
        match self {
            Self::Type => Self::Instance,
            Self::Instance => Self::Type,
        }
    }
}

impl From<EntityRefKind> for GtsRefKeyword {
    fn from(kind: EntityRefKind) -> Self {
        match kind {
            EntityRefKind::Type => Self::Type,
            EntityRefKind::Instance => Self::Instance,
        }
    }
}

/// A GTS reference validation failure, including pattern-reference violations.
#[derive(Debug, Clone)]
pub struct XGtsRefValidationError {
    /// The violated keyword: `x-gts-type-ref`, `x-gts-instance-ref`, or
    /// `x-gts-pattern-ref`. `None` for a general failure to check references.
    pub keyword: Option<GtsRefKeyword>,
    pub field_path: String,
    pub value: String,
    pub ref_pattern: String,
    pub reason: String,
}

impl XGtsRefValidationError {
    /// Creates an error without attributing it to a particular keyword.
    /// Use [`Self::with_keyword`] for a known reference-keyword violation.
    #[must_use]
    pub fn new(field_path: String, value: String, ref_pattern: String, reason: String) -> Self {
        Self {
            keyword: None,
            field_path,
            value,
            ref_pattern,
            reason,
        }
    }

    /// Identifies the reference keyword that produced this error.
    #[must_use]
    pub fn with_keyword(mut self, keyword: GtsRefKeyword) -> Self {
        self.keyword = Some(keyword);
        self
    }
}

impl fmt::Display for XGtsRefValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GTS reference validation failed for field '{}': {}",
            self.field_path, self.reason
        )
    }
}

impl std::error::Error for XGtsRefValidationError {}

/// Checks whether a candidate names an entity that satisfies the selected
/// reference policy.
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

/// The one pointer operand a declaration may use (README §9.6.1).
const SELF_ID_POINTER: &str = "/$id";

/// How far entity-reference targets are checked (README §9.6.1).
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

/// Every usable entity-reference operand the document declares, by location.
///
/// Registry lookup of an operand does not depend on the keyword's kind
/// (README §9.6.1). Unusable declarations are skipped:
/// [`XGtsRefValidator::validate_schema`] already reports them.
pub(crate) fn declared_patterns(schema: &Value) -> Vec<(String, GtsIdPattern)> {
    let selected = XGtsRefValidator::self_id(schema);
    let mut declared = Vec::new();
    crate::schema_modifiers::for_each_schema_node(schema, &mut |node, location| {
        for kind in EntityRefKind::ALL {
            if let Some(value) = node.get(kind.keyword())
                && let Ok(pattern) = resolve_declaration(kind, value, selected.as_deref())
            {
                declared.push((declaration_path("", location, kind.keyword()), pattern));
            }
        }
    });
    declared
}

/// The pattern an operand denotes, or why it is a schema error.
///
/// `selected` is the selected leaf type `/$id` names, without `gts://`
/// (README §9.6.1).
pub(crate) fn resolve_declaration(
    kind: EntityRefKind,
    declared: &Value,
    selected: Option<&str>,
) -> Result<GtsIdPattern, String> {
    let keyword = kind.keyword();
    let Some(spelling) = declared.as_str() else {
        return Err(format!(
            "{keyword} operand must be a string, got {declared}"
        ));
    };

    if spelling == SELF_ID_POINTER {
        let Some(selected) = selected else {
            return Err(format!(
                "{keyword} operand '{SELF_ID_POINTER}' has no selected GTS Type Schema '$id' \
                 to resolve to"
            ));
        };
        return match GtsId::try_new(selected) {
            Ok(id) if id.is_type() => Ok(GtsIdPattern::from(id)),
            _ => Err(format!(
                "{keyword} operand '{SELF_ID_POINTER}' resolves to '{selected}', which is not a \
                 concrete GTS Type Identifier"
            )),
        };
    }

    let pattern = GtsIdPattern::try_new(spelling).map_err(|e| {
        format!(
            "{keyword} operand '{spelling}': {}; expected a GTS identifier, a wildcard pattern \
             or '{SELF_ID_POINTER}'",
            e.cause
        )
    })?;
    if kind == EntityRefKind::Type && GtsId::try_new(spelling).is_ok_and(|id| !id.is_type()) {
        return Err(format!(
            "{keyword} operand '{spelling}' is a GTS Instance Identifier, which selects no \
             GTS Type Identifiers"
        ));
    }
    Ok(pattern)
}

/// Registers both entity-reference keywords for dialect-aware applicability.
///
/// Invalid operands fail compilation, so `not` cannot invert them. `/$id`
/// names `selected`, the type being validated, wherever it is declared.
/// `exists` adds the registry policy; `None` checks structure only.
pub(crate) fn with_x_gts_ref<'o>(
    options: jsonschema::ValidationOptions<'o>,
    selected: Option<&str>,
    exists: Option<ReferenceExists>,
) -> jsonschema::ValidationOptions<'o> {
    let options = with_entity_ref(
        options,
        EntityRefKind::Type,
        selected.map(str::to_owned),
        exists.clone(),
    );
    with_entity_ref(
        options,
        EntityRefKind::Instance,
        selected.map(str::to_owned),
        exists,
    )
}

/// Registers the keyword requiring `kind`.
fn with_entity_ref(
    options: jsonschema::ValidationOptions<'_>,
    kind: EntityRefKind,
    selected: Option<String>,
    exists: Option<ReferenceExists>,
) -> jsonschema::ValidationOptions<'_> {
    options.with_keyword(kind.keyword(), move |_parent, declared, _location| {
        let pattern = resolve_declaration(kind, declared, selected.as_deref())
            .map_err(jsonschema::ValidationError::schema)?;
        Ok(Box::new(EntityRefKeyword {
            kind,
            pattern,
            exists: exists.clone(),
        }))
    })
}

/// One compiled entity-reference declaration.
struct EntityRefKeyword {
    kind: EntityRefKind,
    /// The pattern the operand resolved to.
    pattern: GtsIdPattern,
    exists: Option<ReferenceExists>,
}

impl EntityRefKeyword {
    /// Returns the violation for a string instance; ignores other value types.
    fn violation(&self, instance: &Value) -> Option<String> {
        let value = instance.as_str()?;
        let pattern = self.pattern.pattern();

        let Ok(id) = GtsId::try_new(value) else {
            return Some(format!("Value '{value}' is not a valid GTS identifier"));
        };
        if !self.kind.admits(&id) {
            return Some(format!(
                "Value '{value}' is a {}, but {} requires a {}",
                self.kind.other().noun(),
                self.kind.keyword(),
                self.kind.noun()
            ));
        }
        if !id.matches_pattern(&self.pattern) {
            return Some(format!(
                "Value '{value}' does not match pattern '{pattern}'"
            ));
        }
        // Existence participates in branch selection like any other constraint.
        match &self.exists {
            Some(exists) if !exists(value) => Some(format!(
                "'{value}' references an entity that is not registered, or not valid where the \
                 reference policy requires it"
            )),
            _ => None,
        }
    }
}

impl<'i> jsonschema::Keyword<'i> for EntityRefKeyword {
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

/// The strings an entity reference accepts: identifiers of one kind selected
/// by a literal operand (`/$id` already resolved).
///
/// Used to compare constraints (README §9.6.1 "Derivation and evolution"). It
/// reasons only through the shared `gts-id` matching and coverage.
#[derive(Debug, Clone)]
pub(crate) struct EntityScope {
    kind: EntityRefKind,
    pattern: GtsIdPattern,
}

/// The shortest instance segment, `a.a.a.a.v0`.
const MIN_SEGMENT_LEN: usize = 10;

impl EntityScope {
    /// The scope `keyword` declares with a literal `operand`, or `None` when
    /// the keyword is not an entity reference or the operand is unusable.
    pub(crate) fn of(keyword: &str, operand: &Value) -> Option<Self> {
        let kind = EntityRefKind::of_keyword(keyword)?;
        let pattern = resolve_declaration(kind, operand, None).ok()?;
        Some(Self { kind, pattern })
    }

    pub(crate) const fn kind(&self) -> EntityRefKind {
        self.kind
    }

    /// Whether `id` is in this scope.
    pub(crate) fn admits(&self, id: &GtsId) -> bool {
        self.kind.admits(id) && id.matches_pattern(&self.pattern)
    }

    /// Whether the scope provably selects no identifier: a concrete Type ID
    /// too long for any instance tail, under an instance reference.
    pub(crate) fn is_empty(&self) -> bool {
        self.kind == EntityRefKind::Instance
            && GtsId::try_new(self.pattern.pattern())
                .is_ok_and(|id| id.is_type() && id.id().len() + MIN_SEGMENT_LEN > GTS_ID_MAX_LENGTH)
    }

    /// Whether every identifier `other` accepts, `self` accepts too.
    ///
    /// Coverage of the unfiltered selections is sufficient for one kind;
    /// otherwise only an empty `other` is included.
    pub(crate) fn includes(&self, other: &Self) -> bool {
        other.is_empty() || (self.kind == other.kind && self.pattern.covers(&other.pattern))
    }

    /// Concrete identifiers in this scope, for exhibiting a counterexample.
    ///
    /// Not exhaustive: an empty result proves nothing.
    pub(crate) fn witnesses(&self) -> Vec<GtsId> {
        let spelling = self.pattern.pattern();
        let mut candidates = Vec::new();
        for token in ["a", "z"] {
            let instance = format!("{token}.{token}.{token}.{token}.v0");
            let derived = format!("{instance}~");
            match spelling.strip_suffix('*') {
                None => {
                    candidates.push(spelling.to_owned());
                    candidates.push(format!("{spelling}{instance}"));
                    candidates.push(format!("{spelling}{derived}"));
                }
                Some(stem) => {
                    let completions = [
                        derived.clone(),
                        format!("{token}.{token}.{token}.v0~"),
                        format!("{token}.{token}.v0~"),
                        format!("{token}.v0~"),
                        "v0~".to_owned(),
                        "0~".to_owned(),
                    ];
                    for completion in completions {
                        let as_type = format!("{stem}{completion}");
                        candidates.push(as_type.trim_end_matches('~').to_owned());
                        candidates.push(format!("{as_type}{instance}"));
                        candidates.push(as_type);
                    }
                }
            }
        }
        candidates
            .iter()
            .filter_map(|candidate| GtsId::try_new(candidate).ok())
            .filter(|id| self.admits(id))
            .collect()
    }
}

/// Whether `error` was raised by any of the three GTS reference keywords.
pub(crate) fn is_x_gts_ref_error(error: &jsonschema::ValidationError<'_>) -> bool {
    matches!(
        error.kind(),
        ValidationErrorKind::Custom { keyword, .. } if GtsRefKeyword::from_name(keyword).is_some()
    )
}

/// Extracts reference violations only when they fully explain `error`.
fn attributed_refs(
    schema: &Value,
    error: &jsonschema::ValidationError<'_>,
) -> Option<Vec<XGtsRefValidationError>> {
    if is_x_gts_ref_error(error) {
        return Some(vec![describe_error(schema, error)]);
    }

    // Multiple matching branches are a composition error, not a ref error,
    // even when a further branch fails on a reference.
    let (ValidationErrorKind::AnyOf { context: branches }
    | ValidationErrorKind::OneOfNotValid { context: branches }) = error.kind()
    else {
        return None;
    };

    let mut attributed = Vec::new();
    for cause in branches.iter().flatten() {
        attributed.extend(attributed_refs(schema, cause)?);
    }
    (!attributed.is_empty()).then_some(attributed)
}

/// Splits other constraint diagnostics from GTS reference diagnostics.
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

/// Builds a GTS reference diagnostic from the validator error paths.
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
    let mut diagnosis = XGtsRefValidationError::new(
        error.instance_path().to_string(),
        value,
        declared.to_owned(),
        error.to_string(),
    );
    if let ValidationErrorKind::Custom { keyword, .. } = error.kind() {
        diagnosis.keyword = GtsRefKeyword::from_name(keyword);
    }
    diagnosis
}

/// Reports applicable GTS reference violations and fails closed.
///
/// `exists` enables store-aware checks; `None` checks structure only.
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
                format!("GTS reference validation needs a compilable schema: {e}"),
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
fn declaration_path(prefix: &str, location: &str, keyword: &str) -> String {
    [prefix, location, keyword]
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

    /// Validate an instance against the GTS reference keywords in a schema, without
    /// registry lookup.
    ///
    /// # Arguments
    /// * `instance` - The data instance to validate
    /// * `schema` - The JSON schema declaring the references
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

    /// Validate the entity-reference operands in a schema definition
    ///
    /// Visits only schema positions defined by the document's dialect,
    /// including definitions, inactive branches, and the trait schema.
    ///
    /// # Arguments
    /// * `schema` - The JSON schema to validate
    /// * `schema_path` - Prefix for the reported declaration locations
    /// * `root_schema` - The document whose `$id` `/$id` names
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
            for kind in EntityRefKind::ALL {
                let Some(declared) = node.get(kind.keyword()) else {
                    continue;
                };
                if let Err(reason) = resolve_declaration(kind, declared, selected.as_deref()) {
                    // Non-string declarations have no pattern to report.
                    let (value, ref_pattern) = declared.as_str().map_or_else(
                        || (format!("{declared:?}"), String::new()),
                        |spelling| (spelling.to_owned(), spelling.to_owned()),
                    );
                    errors.push(
                        XGtsRefValidationError::new(
                            declaration_path(schema_path, location, kind.keyword()),
                            value,
                            ref_pattern,
                            reason,
                        )
                        .with_keyword(kind.into()),
                    );
                }
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
    /// The schema's `$id` holds a GTS URI (e.g., `gts://gts.x.example._.user.v1~`),
    /// while `/$id` names the canonical identifier without it.
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
