use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::entities::GtsEntity;
use crate::gts::{GtsId, GtsIdError, GtsIdPattern};
use crate::schema_cast::GtsEntityCastResult;
use crate::schema_evolution::{
    CompatibilityDiagnostic, CompatibilityVerdict, ObjectLevel, check_backward_diagnostics,
    check_forward_diagnostics, classify_object_levels,
};
use crate::schema_resolver::SchemaProvider;
use crate::x_gts_ref::GtsRefValidation;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("GTS instance with ID '{0}' not found in store")]
    InstanceNotFound(String),
    #[error("GTS type schema with ID '{0}' not found in store")]
    SchemaNotFound(String),
    #[error("Entity is invalid: {0}")]
    InvalidEntity(String),
    #[error("Invalid GTS type id: {0}")]
    InvalidTypeId(GtsIdError),
    #[error("Entity ID '{0}' is already registered with different content")]
    ImmutableConflict(String),
    #[error("{0}")]
    ValidationError(String),
    #[error("Invalid $ref: {0}")]
    InvalidRef(String),
    #[error("Circular $ref detected")]
    CircularRef,
    #[error("Unresolved $ref(s): {}", .0.join(", "))]
    UnresolvedRefs(Vec<String>),
}

pub trait GtsReader: Send {
    fn iter(&mut self) -> Box<dyn Iterator<Item = GtsEntity> + '_>;
    fn read_by_id(&self, entity_id: &str) -> Option<GtsEntity>;
    fn reset(&mut self);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsStoreQueryResult {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    pub count: usize,
    pub limit: usize,
    pub results: Vec<Value>,
}

/// Result of comparing two Type Schema documents for schema evolution.
///
/// Produced by [`GtsStore::compare_documents`], which resolves both documents
/// first. Both directions are computed in one pass; which one gates publication
/// is a policy decision for the caller, and gts-spec §6 leaves the enforced mode
/// to the implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use = "the compatibility verdict must be inspected"]
pub struct SchemaComparison {
    /// Evidence for an incompatible or unknown backward verdict, with the
    /// offending schema location on each entry.
    pub backward_diagnostics: Vec<CompatibilityDiagnostic>,
    /// Evidence for an incompatible or unknown forward verdict.
    pub forward_diagnostics: Vec<CompatibilityDiagnostic>,
    /// Content model of every object level of the resolved **new** document.
    ///
    /// A caller admitting the new definition uses this to report, per level,
    /// whether a later definition will be able to add an optional property
    /// there - see [`crate::schema_evolution::ContentModel::is_evolvable_in_place`].
    /// One flag for the
    /// whole document would not do: in the closed-envelope shape recommended by
    /// §4.4.1 the level that decides evolvability is inside an extension
    /// container, not the document root.
    pub candidate_object_levels: Vec<ObjectLevel>,
}

impl SchemaComparison {
    /// `Valid(old) ⊆ Valid(new)`: the new definition accepts every instance the
    /// old one accepted.
    ///
    /// Recomputed from [`Self::backward_diagnostics`] rather than stored beside
    /// them. A verdict is entirely a reading of its evidence, so keeping a copy
    /// would let a value exist - most easily one that was deserialized - that
    /// reports `Compatible` next to a non-empty diagnostic list.
    #[must_use]
    pub fn backward_compatibility(&self) -> CompatibilityVerdict {
        CompatibilityVerdict::from_diagnostics(&self.backward_diagnostics)
    }

    /// `Valid(new) ⊆ Valid(old)`: the old definition accepts every instance the
    /// new one accepts. Recomputed like [`Self::backward_compatibility`].
    #[must_use]
    pub fn forward_compatibility(&self) -> CompatibilityVerdict {
        CompatibilityVerdict::from_diagnostics(&self.forward_diagnostics)
    }

    /// `Valid(old) = Valid(new)`: both directions hold.
    #[must_use]
    pub fn full_compatibility(&self) -> CompatibilityVerdict {
        CompatibilityVerdict::full(self.backward_compatibility(), self.forward_compatibility())
    }

    /// Compares two documents whose references are already resolved.
    fn of_resolved(old_schema: &Value, new_schema: &Value) -> Self {
        let (_, backward_diagnostics) = check_backward_diagnostics(old_schema, new_schema);
        let (_, forward_diagnostics) = check_forward_diagnostics(old_schema, new_schema);
        Self {
            backward_diagnostics,
            forward_diagnostics,
            candidate_object_levels: classify_object_levels(new_schema),
        }
    }

    /// Object levels of the candidate that a later definition cannot extend
    /// with an optional property.
    #[must_use]
    pub fn levels_not_evolvable_in_place(&self) -> Vec<&ObjectLevel> {
        self.candidate_object_levels
            .iter()
            .filter(|level| !level.content_model.is_evolvable_in_place())
            .collect()
    }

    fn backward_messages(&self) -> Vec<String> {
        self.backward_diagnostics
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn forward_messages(&self) -> Vec<String> {
        self.forward_diagnostics
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}

/// Resolved view of a GTS type: self-contained unless a `$ref` cycle makes
/// inlining impossible, in which case `schema` is the body as authored.
///
/// A pure value computed from store contents — the library holds **no cache**
/// of these. Because schemas are append-only by versioned id (a new version is
/// a new `type_id`), a `ResolvedType` is safe for a *consumer* to cache forever
/// keyed by `type_id`: [`GtsStore::register_schema`] refuses to rebind an id to
/// different content.
#[derive(Debug, Clone)]
pub struct ResolvedType {
    /// The type id this resolution is for (the `type_id` passed to
    /// [`GtsStore::validate_schema`]).
    pub id: crate::GtsTypeId,
    /// `true` when the type declares `x-gts-abstract: true` — a template that
    /// cannot have direct instances and defers required-trait completeness.
    pub is_abstract: bool,
    /// `true` when the type declares `x-gts-final: true` — it cannot be extended.
    pub is_final: bool,
    /// Type body with all `#/` and `gts://` `$ref`s inlined, or the body as
    /// authored when a `$ref` cycle makes inlining impossible.
    pub schema: Value,
    /// Chain-merged (RFC 7396) and default-materialized trait values.
    pub effective_traits: Value,
    /// Dialect-pinned, `allOf`-composed, `$ref`-inlined effective traits schema.
    pub effective_traits_schema: Value,
}

/// Whether a [`GtsStore::register`] call changed the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Registration {
    /// The id was unbound and now holds the submitted entity.
    Inserted,
    /// The id already held identical content; the store is untouched.
    Unchanged,
}

pub struct GtsStore {
    by_id: HashMap<String, GtsEntity>,
    reader: Option<Box<dyn GtsReader>>,
    /// Ids whose validation is already on the stack. Reference cycles resolve
    /// to "valid" here so the outer validation is the one that decides.
    validating: HashSet<String>,
    /// What [`Self::entity_is_valid`] has decided during the running
    /// validation, so an entity reached along many paths is validated once.
    verdicts: HashMap<String, bool>,
    /// The ids `verdicts` holds as valid, in the order they were decided.
    judged_valid: Vec<String>,
}

impl Default for GtsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::schema_resolver::SchemaProvider for GtsStore {
    /// Looks the id up in the registered set directly (no reader fallback) and
    /// only exposes it when it is a schema entity — a `$ref` to a non-schema id
    /// stays unresolved.
    fn schema_content(&self, type_id: &str) -> Option<&Value> {
        self.by_id
            .get(type_id)
            .filter(|entity| entity.is_schema)
            .map(|entity| &entity.content)
    }
}

impl GtsStore {
    /// Empty, reader-free store. Callers populate it explicitly via
    /// [`Self::register`] / [`Self::register_schema`]. With no [`GtsReader`],
    /// `get` and resolution never fall back to lazy I/O — the store sees
    /// exactly what was registered.
    #[must_use]
    pub fn new() -> Self {
        GtsStore {
            by_id: HashMap::new(),
            reader: None,
            validating: HashSet::new(),
            verdicts: HashMap::new(),
            judged_valid: Vec::new(),
        }
    }

    /// Store backed by a [`GtsReader`], eagerly populated from it. `get` falls
    /// back to the reader for ids not yet cached.
    #[must_use]
    pub fn with_reader(reader: Box<dyn GtsReader>) -> Self {
        let mut store = GtsStore {
            by_id: HashMap::new(),
            reader: Some(reader),
            validating: HashSet::new(),
            verdicts: HashMap::new(),
            judged_valid: Vec::new(),
        };
        store.populate_from_reader();
        tracing::info!("Populated GtsStore with {} entities", store.by_id.len());
        store
    }

    fn populate_from_reader(&mut self) {
        if let Some(ref mut reader) = self.reader {
            for entity in reader.iter() {
                // Use effective_id() which handles both GTS IDs and anonymous instance IDs
                if let Some(id) = entity.effective_id() {
                    self.by_id.insert(id, entity);
                }
            }
        }
    }

    /// Registers an entity in the store.
    ///
    /// Ids are immutable: resubmitting identical content is a no-op that
    /// leaves the committed entity in place, and rebinding an id to different
    /// content is refused.
    ///
    /// # Errors
    /// Returns `StoreError::InvalidEntity` if the entity has no effective ID,
    /// or `StoreError::ImmutableConflict` if the id is already bound to
    /// different content.
    pub fn register(&mut self, entity: GtsEntity) -> Result<(), StoreError> {
        self.register_with_outcome(entity).map(|_| ())
    }

    /// [`Self::register`], reporting whether the store changed.
    ///
    /// # Errors
    /// See [`Self::register`].
    pub(crate) fn register_with_outcome(
        &mut self,
        entity: GtsEntity,
    ) -> Result<Registration, StoreError> {
        let id = entity
            .effective_id()
            .ok_or_else(|| StoreError::InvalidEntity("Entity has no effective ID".to_owned()))?;
        if let Some(existing) = self.get(&id) {
            return if existing.content == entity.content {
                Ok(Registration::Unchanged)
            } else {
                Err(StoreError::ImmutableConflict(id))
            };
        }
        self.by_id.insert(id, entity);
        Ok(Registration::Inserted)
    }

    /// Drops an id's binding, undoing a [`Registration::Inserted`].
    pub(crate) fn unregister(&mut self, entity_id: &str) {
        self.by_id.remove(entity_id);
    }

    /// Runs `action` with a temporary entity and restores the store on exit.
    ///
    /// # Errors
    /// Returns `StoreError::InvalidEntity` if the entity has no effective ID.
    pub(crate) fn with_transient_entity<T>(
        &mut self,
        entity: GtsEntity,
        action: impl FnOnce(&mut Self, &str) -> T,
    ) -> Result<T, StoreError> {
        let id = entity
            .effective_id()
            .ok_or_else(|| StoreError::InvalidEntity("Entity has no effective ID".to_owned()))?;
        let displaced = self.by_id.insert(id.clone(), entity);

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(self, &id)));

        match displaced {
            Some(previous) => {
                self.by_id.insert(id, previous);
            }
            None => {
                self.by_id.remove(&id);
            }
        }

        match outcome {
            Ok(value) => Ok(value),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    /// The GTS Type Identifier a canonical JSON GTS Type Schema declares.
    ///
    /// Canonical means a top-level `$schema` and a top-level `$id` of the form
    /// `gts://<type-id>` (README §2.4). Whether the dialect is supported is a
    /// validation question, not an identity one, so it is not checked here.
    ///
    /// # Errors
    /// Why `schema` is not a canonical GTS Type Schema.
    pub(crate) fn declared_type_id(schema: &Value) -> Result<String, String> {
        let Some(schema) = schema.as_object() else {
            return Err("a GTS Type Schema must be a JSON object".to_owned());
        };
        if !schema.get("$schema").is_some_and(Value::is_string) {
            return Err("a GTS Type Schema must declare a top-level '$schema'".to_owned());
        }
        let declared = schema.get("$id").and_then(Value::as_str);
        declared
            .and_then(|id| id.strip_prefix(crate::gts::GTS_ID_URI_PREFIX))
            .filter(|id| GtsId::try_new(id).is_ok_and(|id| id.is_type()))
            .map(str::to_owned)
            .ok_or_else(|| {
                format!(
                    "a GTS Type Schema must declare a top-level '$id' of the form \
                     '{}<type-id>' naming a GTS Type Identifier, got {}",
                    crate::gts::GTS_ID_URI_PREFIX,
                    declared.map_or_else(|| "none".to_owned(), |id| format!("'{id}'"))
                )
            })
    }

    /// Registers a schema in the store under an explicitly supplied id.
    ///
    /// The document must still be a canonical GTS Type Schema whose `$id`
    /// names `type_id`: a separately supplied id never stands in for the
    /// embedded one (README §2.4). Ids are immutable here too: see
    /// [`Self::register`].
    ///
    /// # Errors
    /// Returns `StoreError::InvalidTypeId` if `type_id` is not a valid GTS type
    /// id, `StoreError::InvalidEntity` if `schema` is not a canonical GTS Type
    /// Schema for `type_id`, or `StoreError::ImmutableConflict` if the id is
    /// already bound to different content.
    pub fn register_schema(&mut self, type_id: &str, schema: &Value) -> Result<(), StoreError> {
        let gts_id = GtsId::try_new(type_id).map_err(StoreError::InvalidTypeId)?;
        if !gts_id.is_type() {
            return Err(StoreError::InvalidTypeId(GtsIdError::new(
                type_id,
                "GTS type IDs must end with '~'",
            )));
        }
        let declared = Self::declared_type_id(schema).map_err(StoreError::InvalidEntity)?;
        if declared != type_id {
            return Err(StoreError::InvalidEntity(format!(
                "'$id' declares '{declared}', but the schema is registered as '{type_id}'"
            )));
        }
        let entity = GtsEntity::new(
            None,
            None,
            schema,
            None,
            Some(gts_id),
            true,
            String::new(),
            None,
            None,
        );
        self.register(entity)
    }

    pub fn get(&mut self, entity_id: &str) -> Option<&GtsEntity> {
        // Check cache first
        if self.by_id.contains_key(entity_id) {
            return self.by_id.get(entity_id);
        }

        // Try to fetch from reader
        if let Some(ref reader) = self.reader
            && let Some(entity) = reader.read_by_id(entity_id)
        {
            self.by_id.insert(entity_id.to_owned(), entity);
            return self.by_id.get(entity_id);
        }

        None
    }

    /// Fetches a schema entity by its type id.
    ///
    /// Validates that `type_id` is a well-formed GTS *type* id and that the
    /// stored entity is actually a schema, so callers don't have to repeat the
    /// id parse + `is_schema` checks.
    ///
    /// # Errors
    /// Returns `StoreError::InvalidTypeId` if `type_id` is not a valid type id,
    /// `StoreError::SchemaNotFound` if no entity exists for it, or
    /// `StoreError::InvalidEntity` if the entity found is not a schema (e.g. an
    /// instance happens to be registered under that id).
    fn get_schema_entity(&mut self, type_id: &str) -> Result<&GtsEntity, StoreError> {
        if let Err(e) = crate::GtsTypeId::try_new(type_id) {
            return Err(StoreError::InvalidTypeId(e));
        }
        match self.get(type_id) {
            Some(entity) if entity.is_schema => Ok(entity),
            Some(_) => Err(StoreError::InvalidEntity(format!(
                "Entity '{type_id}' is not a schema"
            ))),
            None => Err(StoreError::SchemaNotFound(type_id.to_owned())),
        }
    }

    /// Gets the content of a schema by its type ID.
    ///
    /// # Errors
    /// See [`Self::get_schema_entity`].
    pub fn get_schema_content(&mut self, type_id: &str) -> Result<Value, StoreError> {
        Ok(self.get_schema_entity(type_id)?.content.clone())
    }

    /// Fetches an instance entity by its id.
    ///
    /// Well-known instances parse as GTS ids and are keyed by their normalized
    /// id; anonymous instances (UUIDs, file paths) are not valid GTS ids and are
    /// keyed by their raw id, so an id that fails to parse is used verbatim
    /// rather than rejected.
    ///
    /// # Errors
    /// Returns `StoreError::InstanceNotFound` if no entity exists for the id.
    fn get_instance_entity(&mut self, instance_id: &str) -> Result<GtsEntity, StoreError> {
        let entity = self
            .get(instance_id)
            .cloned()
            .ok_or_else(|| StoreError::InstanceNotFound(instance_id.to_owned()))?;
        if entity.is_schema {
            return Err(StoreError::InvalidEntity(format!(
                "Entity '{instance_id}' is a schema, not an instance; \
                 the id must be an instance (not ending with '~')"
            )));
        }
        Ok(entity)
    }

    pub fn items(&self) -> impl Iterator<Item = (&String, &GtsEntity)> {
        self.by_id.iter()
    }

    /// Strict `$ref` resolution that errors on an unresolved supported local
    /// JSON Pointer or external GTS `$ref`, or a circular `$ref`.
    ///
    /// # Errors
    /// [`StoreError::UnresolvedRefs`] or [`StoreError::CircularRef`].
    pub fn resolve_schema_refs(&self, schema: &Value) -> Result<Value, StoreError> {
        crate::schema_resolver::SchemaResolver::new(self).resolve(schema)
    }

    /// The registered documents `schema` reaches through `gts://` references,
    /// transitively, keyed by the `gts://` URI they are referenced by.
    ///
    /// A validator compiled with them follows every `$ref` itself, under the
    /// rules of the dialect it appears in: cycles, embedded resources and
    /// `$ref` siblings all behave as JSON Schema defines. `schema` itself is
    /// found by its own `$id`.
    ///
    /// # Errors
    /// [`StoreError::InvalidRef`] for a malformed reference, or
    /// [`StoreError::UnresolvedRefs`] for one no registered schema answers.
    fn validation_resources(&self, schema: &Value) -> Result<Vec<(String, &Value)>, StoreError> {
        let references_of = |document: &Value| {
            crate::schema_refs::extract_gts_refs(document)
                .map_err(|e| StoreError::InvalidRef(e.to_string()))
        };
        let own = schema.get("$id").and_then(Value::as_str);
        let mut pending = references_of(schema)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut resources = Vec::new();
        while let Some(id) = pending.pop_first() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let uri = format!("{}{id}", crate::gts::GTS_ID_URI_PREFIX);
            if own == Some(uri.as_str()) {
                continue;
            }
            let Some(document) = self.schema_content(&id) else {
                return Err(StoreError::UnresolvedRefs(vec![uri]));
            };
            pending.extend(references_of(document)?);
            resources.push((uri, document));
        }
        Ok(resources)
    }

    /// Collapses a slice of x-gts-ref validation errors into a single
    /// `StoreError::ValidationError`, or `Ok(())` when there are none.
    fn check_x_gts_ref_errors(
        errors: &[crate::x_gts_ref::XGtsRefValidationError],
    ) -> Result<(), StoreError> {
        if errors.is_empty() {
            return Ok(());
        }
        let messages: Vec<String> = errors
            .iter()
            .map(|err| {
                if err.field_path.is_empty() {
                    err.reason.clone()
                } else {
                    format!("{}: {}", err.field_path, err.reason)
                }
            })
            .collect();
        Err(StoreError::ValidationError(format!(
            "x-gts-ref validation failed: {}",
            messages.join("; ")
        )))
    }

    fn validate_schema_x_gts_refs(schema_content: &Value) -> Result<(), StoreError> {
        let validator = crate::x_gts_ref::XGtsRefValidator::new();
        let x_gts_ref_errors = validator.validate_schema(schema_content, "", None);
        Self::check_x_gts_ref_errors(&x_gts_ref_errors)
    }

    /// Validates all `$ref` URI values in a schema.
    ///
    /// Rules:
    /// - Local refs (starting with `#`) are always valid
    /// - External refs must use `gts://` URI format
    /// - The GTS ID after `gts://` must be a valid GTS identifier
    ///
    /// Delegates to [`crate::schema_refs::extract_gts_refs`], the single
    /// canonical definition of what a GTS `$ref` is, so schema validation and
    /// dependency extraction cannot drift. The collected dependency set is
    /// discarded here - validation only cares that every `$ref` is well-formed.
    ///
    /// # Errors
    /// Returns `StoreError::InvalidRef` if any `$ref` is invalid.
    fn validate_ref_uris(schema: &Value) -> Result<(), StoreError> {
        crate::schema_refs::extract_gts_refs(schema)
            .map(|_| ())
            .map_err(|e| StoreError::InvalidRef(e.to_string()))
    }

    /// Validates every reference in a registered schema document: `$ref` URI
    /// shapes (local `#` pointer or `gts://` type id — [`Self::validate_ref_uris`])
    /// and `x-gts-ref` GTS ids ([`Self::validate_schema_x_gts_refs`]).
    ///
    /// Pure structural check: no dependency resolution and no JSON Schema
    /// meta-compilation. Meta-compilation happens in [`Self::validate_schema`]
    /// once `$ref`s are inlined, so this is safe to run at registration time
    /// even when forward references are not yet registered.
    ///
    /// # Errors
    /// `StoreError::SchemaNotFound` if the id is missing or its content is not
    /// an object; `StoreError::InvalidRef`/`ValidationError` for a malformed
    /// `$ref` or `x-gts-ref`.
    pub(crate) fn validate_schema_refs(&mut self, gts_id: &str) -> Result<(), StoreError> {
        let schema_content = self.get_schema_content(gts_id)?;
        if !schema_content.is_object() {
            return Err(StoreError::InvalidEntity(format!(
                "Schema '{gts_id}' content must be a dictionary"
            )));
        }

        // `$ref` URIs must be local (#...) or gts:// type ids.
        Self::validate_ref_uris(&schema_content)?;
        // `x-gts-ref` values must be valid GTS ids.
        Self::validate_schema_x_gts_refs(&schema_content)?;

        Ok(())
    }

    /// Validates a chained schema ID by checking each derived schema against its base.
    ///
    /// For a chained ID like `gts.A~B~C~`, validates:
    /// - B (derived from A) is compatible with A
    /// - C (derived from A~B) is compatible with A~B
    ///
    /// The heavy lifting is delegated to [`crate::schema_derivation`].
    ///
    /// # Errors
    /// Returns `StoreError::ValidationError` if any derived schema loosens base constraints.
    pub(crate) fn validate_schema_chain(&mut self, gts_id: &str) -> Result<(), StoreError> {
        let gid = GtsId::try_new(gts_id)
            .map_err(|e| StoreError::ValidationError(format!("Invalid GTS ID: {e}")))?;

        // Single-segment schemas have no parent to validate against
        if gid.segments().len() < 2 {
            return Ok(());
        }

        // Build pairs of (base_id, derived_id) for each adjacent level
        let chain_ids = gid.chain_ids();
        for i in 0..chain_ids.len() - 1 {
            let base_id = &chain_ids[i];
            let derived_id = &chain_ids[i + 1];

            // Check x-gts-final: if the base type is final, derivation is not allowed.
            if let Some(base_entity) = self.get(base_id)
                && base_entity
                    .content
                    .get(crate::schema_modifiers::X_GTS_FINAL)
                    == Some(&Value::Bool(true))
            {
                return Err(StoreError::ValidationError(format!(
                    "base type '{base_id}' is final and cannot be extended"
                )));
            }

            tracing::info!(
                "OP#12: Validating schema chain pair: base={} derived={}",
                base_id,
                derived_id
            );

            // Get and resolve both schemas
            let base_content = self.get_schema_content(base_id).map_err(|_| {
                StoreError::ValidationError(format!(
                    "Base schema '{base_id}' not found for chain validation"
                ))
            })?;
            let derived_content = self.get_schema_content(derived_id).map_err(|_| {
                StoreError::ValidationError(format!(
                    "Derived schema '{derived_id}' not found for chain validation"
                ))
            })?;

            let base_resolved = self
                .resolve_schema_refs(&base_content)
                .map_err(|e| StoreError::ValidationError(format!("Schema '{base_id}' has {e}")))?;
            let derived_resolved = self.resolve_schema_refs(&derived_content).map_err(|e| {
                StoreError::ValidationError(format!("Schema '{derived_id}' has {e}"))
            })?;

            let errors = crate::schema_derivation::validate_derivation_compatibility(
                &base_resolved,
                &derived_resolved,
                base_id,
                derived_id,
            );

            if !errors.is_empty() {
                return Err(StoreError::ValidationError(format!(
                    "Schema '{}' is not compatible with base '{}': {}",
                    derived_id,
                    base_id,
                    errors.join("; ")
                )));
            }
        }

        Ok(())
    }

    /// `true` when a schema document declares `x-gts-abstract: true`.
    pub(crate) fn content_is_abstract(content: &Value) -> bool {
        content.get(crate::schema_modifiers::X_GTS_ABSTRACT) == Some(&Value::Bool(true))
    }

    /// `true` when a schema document declares `x-gts-final: true`.
    pub(crate) fn content_is_final(content: &Value) -> bool {
        content.get(crate::schema_modifiers::X_GTS_FINAL) == Some(&Value::Bool(true))
    }

    /// Wrap trait-validation error messages in a `StoreError` tagged with the
    /// offending type id — the single home for this phrasing.
    fn wrap_trait_error(gts_id: &str, errors: &[String]) -> StoreError {
        StoreError::ValidationError(format!(
            "Schema '{gts_id}' trait validation failed: {}",
            errors.join("; ")
        ))
    }

    /// Build the [`EffectiveTraits`](crate::schema_traits::EffectiveTraits) for
    /// `type_id` by walking its `$id` chain (root → leaf).
    ///
    /// Collects `x-gts-traits-schema` subschemas and `x-gts-traits` values from
    /// each level's **raw** content (before `$ref` resolution inlines external
    /// schemas and drops the `x-gts-*` extension keys), inlines JSON Pointer
    /// `$ref`s against their host document, resolves any `gts://` `$ref`s inside
    /// the collected subschemas, RFC 7396-merges the values (descendant
    /// last-wins for scalars/arrays, recursive merge for objects, `null` deletes
    /// the key), then composes the effective trait-schema and materializes the
    /// values. The leaf's `$schema` dialect is re-injected into the composed
    /// schema. Used by [`Self::validate_schema`] (OP#13) and
    /// [`crate::ops::GtsOps`]'s entity-level trait check.
    ///
    /// # Errors
    /// `StoreError::ValidationError` if the id is invalid, an ancestor schema is
    /// missing, or a `$ref` inside a trait schema fails to resolve.
    pub(crate) fn effective_traits(
        &mut self,
        type_id: &str,
    ) -> Result<crate::schema_traits::EffectiveTraits, StoreError> {
        let gid = GtsId::try_new(type_id)
            .map_err(|e| StoreError::ValidationError(format!("Invalid GTS ID: {e}")))?;

        let mut trait_schemas: Vec<Value> = Vec::new();
        let mut merged_traits = serde_json::Map::new();

        for schema_id in &gid.chain_ids() {
            let content = self.get_schema_content(schema_id).map_err(|_| {
                StoreError::ValidationError(format!(
                    "Schema '{schema_id}' not found for trait validation"
                ))
            })?;

            // Collect this level's trait schemas, then inline any JSON Pointer
            // (`#/...`) `$ref`s against this host document (`content`) while it
            // is still the document root — see `inline_local_pointers`.
            let mut level_trait_schemas = Vec::new();
            crate::schema_traits::collect_trait_schema_from_value(
                &content,
                &mut level_trait_schemas,
            );
            for ts in level_trait_schemas {
                trait_schemas.push(crate::schema_traits::inline_local_pointers(ts, &content));
            }

            let mut level_traits = serde_json::Map::new();
            crate::schema_traits::collect_traits_from_value(&content, &mut level_traits);
            crate::schema_traits::merge_rfc7396_into(&mut merged_traits, &level_traits);
        }

        let mut resolved_trait_schemas: Vec<Value> = Vec::with_capacity(trait_schemas.len());
        for ts in &trait_schemas {
            let resolved = self.resolve_schema_refs(ts).map_err(|e| {
                StoreError::ValidationError(format!("Schema '{type_id}' trait schema has {e}"))
            })?;
            resolved_trait_schemas.push(resolved);
        }

        // Dialect comes from the leaf document's `$schema`, re-injected into the
        // composed trait schema because the inline fragment had its root-only
        // `$schema` stripped when embedded.
        let dialect = self
            .get(type_id)
            .and_then(|leaf| leaf.content.get("$schema").and_then(Value::as_str))
            .map(str::to_owned);

        Ok(crate::schema_traits::build_effective_traits(
            &resolved_trait_schemas,
            &Value::Object(merged_traits),
            dialect.as_deref(),
        )
        .for_type(type_id))
    }

    /// Fully validate a registered type schema and return its resolved
    /// [`ResolvedType`] in a single pass. Every type it depends on (its
    /// `$id`-chain ancestors and the targets of its `gts://` `$ref`s) must
    /// already be registered.
    ///
    /// Pipeline:
    /// 1. [`Self::validate_schema_refs`] — `$ref`/`x-gts-ref` structure;
    /// 2. [`crate::schema_modifiers::validate_gts_keywords`] — format and
    ///    top-level placement of `x-gts-final`/`x-gts-abstract`/`x-gts-traits`/
    ///    `x-gts-traits-schema`;
    /// 3. [`Self::validate_schema_chain`] — derived-vs-base compatibility (OP#12);
    /// 4. resolve: inline `#/` and `gts://` `$ref`s into a self-contained body;
    /// 5. meta-compile the resolved body against JSON Schema — registration
    ///    defers this whenever raw `gts://` refs are present, so it is done here
    ///    once every dependency is inlined, catching malformed schema bodies;
    /// 6. build the effective traits schema/values **exactly once** and validate
    ///    them (OP#13): provided trait values are always type/enum/`x-gts-ref`
    ///    checked; the required-trait completeness check is skipped for abstract
    ///    leaves.
    ///
    /// Abstract types still type-check any trait values they provide, but skip
    /// the OP#13 completeness check (a descendant closes the required traits).
    ///
    /// Uncached: a consumer that calls this repeatedly for the same `type_id`
    /// should cache the result (safe forever — versioned ids are immutable).
    /// [`crate::ops::GtsOps::validate_schema`] wraps this for the
    /// `/validate-type-schema` endpoint, discarding the resolved artifacts.
    ///
    /// # Errors
    /// `StoreError::ValidationError` if any validation stage fails or a
    /// dependency is missing from the store; `StoreError::SchemaNotFound` if the
    /// type is not registered.
    pub fn validate_schema(&mut self, type_id: &str) -> Result<ResolvedType, StoreError> {
        self.validate_schema_with(type_id, GtsRefValidation::default())
    }

    /// [`Self::validate_schema`] under an explicit `x-gts-ref` mode.
    ///
    /// # Errors
    /// See [`Self::validate_schema`].
    pub fn validate_schema_with(
        &mut self,
        type_id: &str,
        refs: GtsRefValidation,
    ) -> Result<ResolvedType, StoreError> {
        self.begin_validation();
        self.check_schema(type_id, refs)
    }

    /// Starts a top-level validation. Verdicts are shared only within one:
    /// registrations between calls can change what is valid. Nothing is on the
    /// stack here, so clearing `validating` only drops what a caught panic left.
    fn begin_validation(&mut self) {
        self.validating.clear();
        self.verdicts.clear();
        self.judged_valid.clear();
    }

    /// [`Self::validate_schema_with`] as part of the running validation.
    fn check_schema(
        &mut self,
        type_id: &str,
        refs: GtsRefValidation,
    ) -> Result<ResolvedType, StoreError> {
        let resolved = self.validate_schema_locally(type_id, refs)?;
        let mut validated = HashSet::from([type_id.to_owned()]);
        self.validate_related_types(type_id, refs, &mut validated)?;
        Ok(resolved)
    }

    /// Validates the types `type_id` derives from and `$ref`s, transitively.
    ///
    /// A type is only as valid as what it builds on, so an invalid ancestor or
    /// reference target invalidates it too (spec v0.14 §12). `validated` both
    /// memoizes and breaks reference cycles.
    fn validate_related_types(
        &mut self,
        type_id: &str,
        refs: GtsRefValidation,
        validated: &mut HashSet<String>,
    ) -> Result<(), StoreError> {
        for related in self.related_type_ids(type_id)? {
            if !validated.insert(related.clone()) {
                continue;
            }
            self.validate_schema_locally(&related, refs).map_err(|e| {
                StoreError::ValidationError(format!(
                    "'{type_id}' depends on GTS type '{related}', which is invalid: {e}"
                ))
            })?;
            self.validate_related_types(&related, refs, validated)?;
        }
        Ok(())
    }

    /// The immediate base type and every `gts://` `$ref` target of `type_id`.
    fn related_type_ids(&mut self, type_id: &str) -> Result<Vec<String>, StoreError> {
        let content = self.get_schema_content(type_id)?;
        let mut related: Vec<String> = GtsId::try_new(type_id)
            .ok()
            .and_then(|id| id.get_type_id())
            .into_iter()
            .collect();
        related.extend(
            crate::schema_refs::extract_gts_refs(&content)
                .map_err(|e| StoreError::InvalidRef(e.to_string()))?,
        );
        related.retain(|id| id != type_id);
        Ok(related)
    }

    /// Validates `type_id` on its own, without following its dependencies.
    fn validate_schema_locally(
        &mut self,
        type_id: &str,
        refs: GtsRefValidation,
    ) -> Result<ResolvedType, StoreError> {
        let content = self.get_schema_content(type_id)?;
        if !content.is_object() {
            return Err(StoreError::InvalidEntity(format!(
                "Schema '{type_id}' content must be a dictionary"
            )));
        }
        // First, so nothing below reads any part of the type under a dialect
        // its author did not choose.
        self.check_dialect(type_id, &content)?;

        // Validate $ref URIs (must be local #... or gts:// type ids)
        Self::validate_ref_uris(&content)?;

        // Validate x-gts-ref values (must be valid GTS ids)
        Self::validate_schema_x_gts_refs(&content)?;
        self.check_constraint_targets(&content, refs)?;

        // Validate GTS keywords
        crate::schema_modifiers::validate_gts_keywords(&content)
            .map_err(StoreError::ValidationError)?;

        // Validate schema derivation chain and base type compatibility
        self.validate_schema_chain(type_id)?;

        // Resolve schema references. JSON Schema allows recursion, so a cyclic
        // `$ref` graph makes the document unresolvable rather than invalid:
        // inlining is skipped here and whatever must materialize the document
        // (a trait-schema chain) reports the cycle at that point.
        let resolved_schema = match self.resolve_schema_refs(&content) {
            Ok(resolved) => Some(resolved),
            Err(StoreError::CircularRef) => None,
            Err(e) => {
                return Err(StoreError::ValidationError(format!(
                    "Schema '{type_id}' has {e}"
                )));
            }
        };

        // Syntax belongs to the document as authored, so it is checked against
        // the declared meta-schema without dereferencing anything — a `$ref`
        // cycle must not buy a schema an exemption from being well-formed.
        jsonschema::meta::validate(&content).map_err(|e| {
            StoreError::ValidationError(format!(
                "JSON Schema validation failed for '{type_id}': {e}"
            ))
        })?;

        // Compile the body exactly as instance validation builds it, so an
        // accepted type is one its instances can be validated against: with
        // every document it reaches, a `$ref` cycle included.
        let resources = self
            .validation_resources(&content)
            .map_err(|e| StoreError::ValidationError(format!("Schema '{type_id}' has {e}")))?;
        crate::json_schema::gts_validator_for_type(&content, Some(type_id), &resources, None)
            .map_err(|e| {
                StoreError::ValidationError(format!(
                    "JSON Schema validation failed for '{type_id}': {e}"
                ))
            })?;

        // Trait values are always validated against the effective trait-schema
        // (type/enum/`x-gts-ref` conformance), even for abstract types. Only the
        // required-trait *completeness* check is gated: an abstract type may
        // leave a required trait unresolved for a descendant to supply, so it is
        // validated with `check_unresolved = false`.
        let is_abstract = Self::content_is_abstract(&content);
        let traits = self.effective_traits(type_id)?;
        let satisfied = self.unsatisfied_references(&traits.values, refs);

        traits
            .validate(!is_abstract, Some(satisfied))
            .map_err(|errors| Self::wrap_trait_error(type_id, &errors))?;

        Ok(ResolvedType {
            id: crate::GtsTypeId::try_new(type_id).map_err(StoreError::InvalidTypeId)?,
            is_abstract,
            is_final: Self::content_is_final(&content),
            schema: resolved_schema.unwrap_or(content),
            effective_traits: traits.values,
            effective_traits_schema: traits.schema,
        })
    }

    /// Checks that `type_id` declares a supported dialect, the same one as the
    /// root of its `$id` chain, that none of its subschemas switches to another
    /// one, and that no `$ref` in it crosses dialects (README §11.0).
    ///
    /// Intermediate chain members and `gts://` targets get the same check when
    /// [`Self::validate_related_types`] validates them, so the whole hierarchy
    /// and reference graph end up on one dialect.
    fn check_dialect(&mut self, type_id: &str, content: &Value) -> Result<(), StoreError> {
        let fail = |reason: String| {
            StoreError::ValidationError(format!(
                "JSON Schema dialect check failed for '{type_id}': {reason}"
            ))
        };
        let dialect = crate::schema_dialect::document_dialect(content).map_err(fail)?;

        let root_id = GtsId::try_new(type_id)
            .ok()
            .and_then(|id| id.chain_ids().into_iter().next())
            .filter(|root_id| root_id != type_id);
        if let Some(root_id) = root_id
            && let Some(root) = self.get(&root_id)
        {
            let root_dialect = jsonschema::Draft::default().detect(&root.content);
            if root_dialect != dialect {
                return Err(fail(format!(
                    "it declares {} but its root type '{root_id}' selects {}; \
                     a derivation hierarchy has a single dialect",
                    crate::schema_dialect::dialect_name(dialect),
                    crate::schema_dialect::dialect_name(root_dialect)
                )));
            }
        }

        crate::schema_dialect::check_subschemas(content, dialect).map_err(fail)?;
        crate::schema_dialect::check_references(content, self).map_err(fail)
    }

    /// Whether `reference` satisfies `refs` as an `x-gts-ref` target.
    ///
    /// Non-identifiers are left to the pattern check, which reports them.
    fn reference_is_satisfied(&mut self, reference: &str, refs: GtsRefValidation) -> bool {
        if !refs.checks_registry() || GtsId::try_new(reference).is_err() {
            return true;
        }
        if self.get(reference).is_none() {
            return false;
        }
        !refs.checks_validity() || self.entity_is_valid(reference)
    }

    /// Whether a registered entity validates. Cycles count as valid: the
    /// validation already on the stack is the one that reports the problem.
    ///
    /// Decided once per validation. A verdict reached while a cycle was
    /// assumed valid is withdrawn if that assumption fails.
    fn entity_is_valid(&mut self, entity_id: &str) -> bool {
        if let Some(&valid) = self.verdicts.get(entity_id) {
            return valid;
        }
        if !self.validating.insert(entity_id.to_owned()) {
            return true;
        }
        let judged_before = self.judged_valid.len();
        let valid = match GtsId::try_new(entity_id) {
            Ok(id) if id.is_type() => self
                .check_schema(entity_id, GtsRefValidation::AnyValid)
                .is_ok(),
            Ok(_) => self
                .check_instance(entity_id, GtsRefValidation::AnyValid)
                .is_ok(),
            Err(_) => true,
        };
        self.validating.remove(entity_id);
        if valid {
            self.judged_valid.push(entity_id.to_owned());
        } else {
            // Anything found valid meanwhile may have assumed this entity valid
            // to break a cycle, so those verdicts are no longer safe to keep.
            for withdrawn in self.judged_valid.drain(judged_before..) {
                self.verdicts.remove(&withdrawn);
            }
        }
        self.verdicts.insert(entity_id.to_owned(), valid);
        valid
    }

    /// The candidate values in `document` that `refs` rejects.
    ///
    /// Precomputed because the `x-gts-ref` keyword cannot borrow the store.
    fn unsatisfied_references(
        &mut self,
        document: &Value,
        refs: GtsRefValidation,
    ) -> crate::x_gts_ref::ReferenceExists {
        let mut unsatisfied = HashSet::new();
        for reference in crate::x_gts_ref::candidate_reference_values(document) {
            if !self.reference_is_satisfied(&reference, refs) {
                unsatisfied.insert(reference);
            }
        }
        std::sync::Arc::new(move |reference: &str| !unsatisfied.contains(reference))
    }

    /// Checks that every `x-gts-ref` constraint target in `content` resolves.
    ///
    /// A non-wildcard pattern names one type; a wildcard is satisfied by any
    /// registered match, so the registry is scanned for one.
    fn check_constraint_targets(
        &mut self,
        content: &Value,
        refs: GtsRefValidation,
    ) -> Result<(), StoreError> {
        if !refs.checks_registry() {
            return Ok(());
        }
        for (location, pattern) in crate::x_gts_ref::declared_patterns(content) {
            let spelling = pattern.pattern().to_owned();
            let satisfied = if spelling.contains('*') {
                self.matching_ids(&pattern)
                    .into_iter()
                    .any(|id| !refs.checks_validity() || self.entity_is_valid(&id))
            } else {
                self.get(&spelling).is_some()
                    && (!refs.checks_validity() || self.entity_is_valid(&spelling))
            };
            if !satisfied {
                let requirement = if refs.checks_validity() {
                    "no registered GTS type satisfies it and validates"
                } else {
                    "no registered GTS type satisfies it"
                };
                return Err(StoreError::ValidationError(format!(
                    "x-gts-ref validation failed: {location} constrains values to \
                     '{spelling}', but {requirement}"
                )));
            }
        }
        Ok(())
    }

    /// Registered ids matching `pattern`.
    fn matching_ids(&self, pattern: &GtsIdPattern) -> Vec<String> {
        self.by_id
            .keys()
            .filter(|id| GtsId::try_new(id).is_ok_and(|parsed| parsed.matches_pattern(pattern)))
            .cloned()
            .collect()
    }

    /// Validate a caller-supplied instance payload against `type_id`'s schema.
    ///
    /// Stateless: no registered instance is required, but the type and its
    /// `$ref`/chain dependencies must be registered. Rejects abstract types
    /// (OP#6) and enforces `x-gts-ref`.
    ///
    /// # Errors
    /// `StoreError::ValidationError` on schema-compile failure, JSON Schema
    /// validation failure, abstract type, or `x-gts-ref` violation;
    /// `StoreError::SchemaNotFound` if the type is not registered.
    pub fn validate_payload(&mut self, type_id: &str, payload: &Value) -> Result<(), StoreError> {
        self.validate_payload_with(type_id, payload, GtsRefValidation::default())
    }

    /// [`Self::validate_payload`] under an explicit `x-gts-ref` mode.
    ///
    /// # Errors
    /// See [`Self::validate_payload`].
    pub fn validate_payload_with(
        &mut self,
        type_id: &str,
        payload: &Value,
        refs: GtsRefValidation,
    ) -> Result<(), StoreError> {
        self.begin_validation();
        self.check_payload(type_id, payload, refs)
    }

    /// [`Self::validate_payload_with`] as part of the running validation.
    fn check_payload(
        &mut self,
        type_id: &str,
        payload: &Value,
        refs: GtsRefValidation,
    ) -> Result<(), StoreError> {
        let content = self.get_schema_content(type_id)?;

        // Abstract types cannot have direct instances (OP#6).
        if Self::content_is_abstract(&content) {
            return Err(StoreError::ValidationError(format!(
                "type '{type_id}' is abstract and cannot have direct instances"
            )));
        }

        // An instance is no more valid than the type it claims.
        if !self.validating.contains(type_id) {
            self.check_schema(type_id, refs).map_err(|e| {
                StoreError::ValidationError(format!("type '{type_id}' is invalid: {e}"))
            })?;
        }

        // Payload validation needs only the type body — traits are
        // schema-level metadata (§9.7) and never appear in instances, so the
        // effective-traits build is deliberately skipped here. The validator
        // follows the body's `$ref`s itself; this also enforces `x-gts-ref`.
        let satisfied = self.unsatisfied_references(payload, refs);
        let resources = self
            .validation_resources(&content)
            .map_err(|e| StoreError::ValidationError(format!("Schema '{type_id}' has {e}")))?;
        let validator = crate::json_schema::gts_validator_for_type(
            &content,
            Some(type_id),
            &resources,
            Some(satisfied),
        )
        .map_err(|e| StoreError::ValidationError(format!("Invalid schema for '{type_id}': {e}")))?;

        if validator.is_valid(payload) {
            return Ok(());
        }
        // Inlined, the body shows whether a rejection is affordable to explain.
        let resolved = self.resolve_schema_refs(&content).ok();
        let diagnosis =
            crate::json_schema::diagnose_resolved(&validator, &content, resolved.as_ref(), payload);
        let mut errors = diagnosis.standard;
        errors.extend(diagnosis.unexplained);
        if !errors.is_empty() {
            return Err(StoreError::ValidationError(format!(
                "Validation failed: {}",
                errors.join(", ")
            )));
        }
        Self::check_x_gts_ref_errors(&diagnosis.references)?;

        Ok(())
    }

    /// Validates an instance against its schema.
    ///
    /// # Errors
    /// Returns `StoreError` if validation fails.
    pub fn validate_instance(&mut self, instance_id: &str) -> Result<(), StoreError> {
        self.validate_instance_with(instance_id, GtsRefValidation::default())
    }

    /// [`Self::validate_instance`] under an explicit `x-gts-ref` mode.
    ///
    /// # Errors
    /// See [`Self::validate_instance`].
    pub fn validate_instance_with(
        &mut self,
        instance_id: &str,
        refs: GtsRefValidation,
    ) -> Result<(), StoreError> {
        self.begin_validation();
        self.check_instance(instance_id, refs)
    }

    /// [`Self::validate_instance_with`] as part of the running validation.
    fn check_instance(
        &mut self,
        instance_id: &str,
        refs: GtsRefValidation,
    ) -> Result<(), StoreError> {
        let obj = self.get_instance_entity(instance_id)?;

        let type_id = obj.type_id.as_ref().ok_or_else(|| {
            StoreError::InvalidEntity(format!("Instance '{instance_id}' has no type_id"))
        })?;

        tracing::info!(
            "Validating instance {} against schema {}",
            instance_id,
            type_id
        );

        // A registered instance is just a stored payload; validation is identical
        // to validating a caller-supplied payload against its declared type.
        self.check_payload(type_id, &obj.content, refs)
    }

    /// Casts an entity from one schema to another.
    ///
    /// # Errors
    /// Returns `StoreError` if the cast fails.
    pub fn cast(
        &mut self,
        instance_id: &str,
        target_type_id: &str,
    ) -> Result<GtsEntityCastResult, StoreError> {
        let instance = self.get_instance_entity(instance_id)?;
        let instance_type_id = instance.type_id.clone().ok_or_else(|| {
            StoreError::InvalidEntity(format!("Instance '{instance_id}' has no type_id"))
        })?;
        let mut from_schema = self.get_schema_entity(&instance_type_id)?.clone();
        let mut target_schema = self.get_schema_entity(target_type_id)?.clone();

        // Resolve both schemas before casting, exactly as `is_compatible` does.
        // The compatibility verdicts this result carries are a property of the
        // effective resolved schemas (sec 4.4); comparing unresolved documents
        // here would let the same pair of schemas get one verdict through OP#8
        // and a different one through OP#9. Resolution also makes a base type's
        // properties and `const` values visible to the cast itself.
        from_schema.content = self
            .resolve_schema_refs(&from_schema.content)
            .map_err(|e| {
                StoreError::SchemaNotFound(format!(
                    "Could not resolve source schema '{instance_type_id}': {e}"
                ))
            })?;
        target_schema.content = self
            .resolve_schema_refs(&target_schema.content)
            .map_err(|e| {
                StoreError::SchemaNotFound(format!(
                    "Could not resolve target schema '{target_type_id}': {e}"
                ))
            })?;

        // Create a resolver to handle $ref in schemas
        // TODO: Implement custom resolver
        let resolver = None;

        instance
            .cast(&target_schema, &from_schema, resolver)
            .map_err(|e| StoreError::SchemaNotFound(e.to_string()))
    }

    /// Admits one side of a compatibility comparison and warms the reader
    /// cache, rendering the failure as the message the result carries.
    ///
    /// Returns nothing on success on purpose: the entity is read back through a
    /// shared borrow afterwards, so both documents can be reached at once
    /// without cloning either. The `&mut self` here is only what the lazy
    /// reader fallback in [`Self::get`] needs.
    ///
    /// A missing schema keeps the historical `"Schema not found"` wording, which
    /// clients match on; every other cause - a malformed type id, an id naming a
    /// registered non-schema entity - reports itself, so the caller can tell an
    /// unregistered type from a request it should not have made at all.
    fn admit_compared_schema(&mut self, type_id: &str) -> Result<(), String> {
        self.get_schema_entity(type_id)
            .map(|_| ())
            .map_err(|error| match error {
                StoreError::SchemaNotFound(_) => "Schema not found".to_owned(),
                error => error.to_string(),
            })
    }

    /// Checks GTS schema-evolution compatibility using accepted-instance set inclusion.
    #[must_use = "the compatibility verdict and its diagnostics are the result of the check"]
    pub fn is_compatible(&mut self, old_type_id: &str, new_type_id: &str) -> GtsEntityCastResult {
        if let Err(message) = self
            .admit_compared_schema(old_type_id)
            .and_then(|()| self.admit_compared_schema(new_type_id))
        {
            return GtsEntityCastResult::undecided(old_type_id, new_type_id, message);
        }

        let resolution_failure = |message: String| {
            GtsEntityCastResult::undecided_with_direction(
                old_type_id,
                new_type_id,
                GtsEntityCastResult::infer_direction(old_type_id, new_type_id),
                message,
            )
        };
        // Both ids named a registered schema above and are now cached, so a
        // shared borrow reaches each document in place. `resolve_schema_refs`
        // also takes `&self` and returns a fresh owned document, so neither
        // side needs a clone of the registered entity.
        let (Some(old_content), Some(new_content)) = (
            self.schema_content(old_type_id),
            self.schema_content(new_type_id),
        ) else {
            return GtsEntityCastResult::undecided(
                old_type_id,
                new_type_id,
                "Schema not found".to_owned(),
            );
        };
        let old_schema = match self.resolve_schema_refs(old_content) {
            Ok(schema) => schema,
            Err(error) => {
                return resolution_failure(format!(
                    "Could not resolve old schema '{old_type_id}': {error}"
                ));
            }
        };
        let new_schema = match self.resolve_schema_refs(new_content) {
            Ok(schema) => schema,
            Err(error) => {
                return resolution_failure(format!(
                    "Could not resolve new schema '{new_type_id}': {error}"
                ));
            }
        };

        let comparison = SchemaComparison::of_resolved(&old_schema, &new_schema);
        let backward_compatibility = comparison.backward_compatibility();
        let forward_compatibility = comparison.forward_compatibility();
        let full_compatibility = comparison.full_compatibility();
        let backward_errors = comparison.backward_messages();
        let forward_errors = comparison.forward_messages();
        let incompatibility_reasons = backward_errors
            .iter()
            .map(|error| format!("backward: {error}"))
            .chain(
                forward_errors
                    .iter()
                    .map(|error| format!("forward: {error}")),
            )
            .collect();

        // Determine direction
        let direction = GtsEntityCastResult::infer_direction(old_type_id, new_type_id);

        GtsEntityCastResult {
            from_id: old_type_id.to_owned(),
            to_id: new_type_id.to_owned(),
            old: old_type_id.to_owned(),
            new: new_type_id.to_owned(),
            direction,
            added_properties: Vec::new(),
            removed_properties: Vec::new(),
            changed_properties: Vec::new(),
            full_compatibility,
            backward_compatibility,
            forward_compatibility,
            incompatibility_reasons,
            backward_errors,
            forward_errors,
            // Produced here, so this build's versions are the provenance.
            specification_version: Some(crate::GTS_SPECIFICATION_VERSION.to_owned()),
            implementation_version: Some(crate::GTS_IMPLEMENTATION_VERSION.to_owned()),
            casted_entity: None,
            error: None,
        }
    }

    /// Compares two Type Schema **documents** rather than two registered
    /// identifiers.
    ///
    /// [`Self::is_compatible`] requires both definitions to be addressable by
    /// GTS Type Identifier, which the conformance API assumes (gts-spec §4.2).
    /// An implementation that replaces a definition in place under an unchanged
    /// identifier never has two such identifiers, and §4.2 leaves revision
    /// addressing to that implementation. This entry point serves that case: it
    /// takes the two documents, resolves their references against this store,
    /// and returns both directions plus the per-level content model of the
    /// candidate in one call.
    ///
    /// Resolution is not optional. §4.4 requires the content model to be read
    /// from the fully resolved effective schema, so comparing authored
    /// documents would misclassify a level that is closed only through a `$ref`
    /// to its base.
    ///
    /// # Errors
    /// [`StoreError::SchemaNotFound`] when either document has a reference this
    /// store cannot resolve. Failing here rather than comparing unresolved
    /// documents keeps an undecidable check from being reported as a verdict.
    pub fn compare_documents(
        &self,
        old_schema: &Value,
        new_schema: &Value,
    ) -> Result<SchemaComparison, StoreError> {
        let old_resolved = self.resolve_schema_refs(old_schema).map_err(|error| {
            StoreError::SchemaNotFound(format!("Could not resolve the old document: {error}"))
        })?;
        let new_resolved = self.resolve_schema_refs(new_schema).map_err(|error| {
            StoreError::SchemaNotFound(format!("Could not resolve the new document: {error}"))
        })?;
        Ok(SchemaComparison::of_resolved(&old_resolved, &new_resolved))
    }

    /// Legacy name retained for source compatibility.
    ///
    /// Compatibility is no longer defined specifically in terms of a minor
    /// version change; callers should prefer [`Self::is_compatible`].
    #[must_use = "the compatibility verdict and its diagnostics are the result of the check"]
    pub fn is_minor_compatible(
        &mut self,
        old_type_id: &str,
        new_type_id: &str,
    ) -> GtsEntityCastResult {
        self.is_compatible(old_type_id, new_type_id)
    }

    pub fn build_schema_graph(&mut self, gts_id: &str) -> Value {
        let mut seen_gts_ids = std::collections::HashSet::new();
        self.gts2node(gts_id, &mut seen_gts_ids)
    }

    fn gts2node(
        &mut self,
        gts_id: &str,
        seen_gts_ids: &mut std::collections::HashSet<String>,
    ) -> Value {
        let mut ret = serde_json::Map::new();
        ret.insert("id".to_owned(), Value::String(gts_id.to_owned()));

        if seen_gts_ids.contains(gts_id) {
            return Value::Object(ret);
        }

        seen_gts_ids.insert(gts_id.to_owned());

        // Clone the entity to avoid borrowing issues
        let entity_clone = self.get(gts_id).cloned();

        if let Some(entity) = entity_clone {
            let mut refs = serde_json::Map::new();

            // Collect ref IDs first to avoid borrow issues
            let ref_ids: Vec<_> = entity
                .gts_refs
                .iter()
                .filter(|r| {
                    r.id != gts_id
                        && !r.id.starts_with("http://json-schema.org")
                        && !r.id.starts_with("https://json-schema.org")
                })
                .map(|r| (r.source_path.clone(), r.id.clone()))
                .collect();

            for (source_path, ref_id) in ref_ids {
                refs.insert(source_path, self.gts2node(&ref_id, seen_gts_ids));
            }

            if !refs.is_empty() {
                ret.insert("refs".to_owned(), Value::Object(refs));
            }

            if let Some(ref type_id) = entity.type_id {
                if !type_id.starts_with("http://json-schema.org")
                    && !type_id.starts_with("https://json-schema.org")
                {
                    let type_id_clone = type_id.clone();
                    ret.insert(
                        "type_id".to_owned(),
                        self.gts2node(&type_id_clone, seen_gts_ids),
                    );
                }
            } else {
                let mut errors = ret
                    .get("errors")
                    .and_then(|e| e.as_array())
                    .cloned()
                    .unwrap_or_default();
                errors.push(Value::String("Schema not recognized".to_owned()));
                ret.insert("errors".to_owned(), Value::Array(errors));
            }
        } else {
            let mut errors = ret
                .get("errors")
                .and_then(|e| e.as_array())
                .cloned()
                .unwrap_or_default();
            errors.push(Value::String("Entity not found".to_owned()));
            ret.insert("errors".to_owned(), Value::Array(errors));
        }

        Value::Object(ret)
    }

    #[must_use]
    pub fn query(&self, expr: &str, limit: usize) -> GtsStoreQueryResult {
        let mut result = GtsStoreQueryResult {
            error: String::new(),
            count: 0,
            limit,
            results: Vec::new(),
        };

        // Parse the query expression
        let (base, _, filt) = expr.partition('[');
        let base_pattern = base.trim();
        let is_wildcard = base_pattern.contains('*');

        // Parse filters if present
        let filter_str = if filt.is_empty() {
            ""
        } else {
            filt.rsplit_once(']').map_or("", |x| x.0)
        };
        let filters = Self::parse_query_filters(filter_str);

        // Validate and create pattern
        let (wildcard_pattern, exact_gts_id, error) =
            Self::validate_query_pattern(base_pattern, is_wildcard);
        if !error.is_empty() {
            result.error = error;
            return result;
        }

        // Filter entities
        for entity in self.by_id.values() {
            if result.results.len() >= limit {
                break;
            }

            if !entity.content.is_object() {
                continue;
            }

            let Some(ref gts_id) = entity.gts_id else {
                continue;
            };

            // Check if ID matches the pattern
            if !Self::matches_id_pattern(
                gts_id,
                base_pattern,
                is_wildcard,
                wildcard_pattern.as_ref(),
                exact_gts_id.as_ref(),
            ) {
                continue;
            }

            // Check filters
            if !Self::matches_filters(&entity.content, &filters) {
                continue;
            }

            result.results.push(entity.content.clone());
        }

        result.count = result.results.len();
        result
    }

    fn parse_query_filters(filter_str: &str) -> HashMap<String, String> {
        let mut filters = HashMap::new();
        if filter_str.is_empty() {
            return filters;
        }

        let parts: Vec<&str> = filter_str.split(',').map(str::trim).collect();
        for part in parts {
            if let Some((k, v)) = part.split_once('=') {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                filters.insert(k.trim().to_owned(), v.to_owned());
            }
        }

        filters
    }

    fn validate_query_pattern(
        base_pattern: &str,
        is_wildcard: bool,
    ) -> (Option<GtsIdPattern>, Option<GtsId>, String) {
        if is_wildcard {
            if !base_pattern.ends_with(".*") && !base_pattern.ends_with("~*") {
                return (
                    None,
                    None,
                    "Invalid query: wildcard patterns must end with .* or ~*".to_owned(),
                );
            }
            match GtsIdPattern::try_new(base_pattern) {
                Ok(pattern) => (Some(pattern), None, String::new()),
                Err(e) => (None, None, format!("Invalid query: {e}")),
            }
        } else {
            match GtsId::try_new(base_pattern) {
                Ok(gts_id) => {
                    if gts_id.segments().is_empty() {
                        (
                            None,
                            None,
                            "Invalid query: GTS ID has no valid segments".to_owned(),
                        )
                    } else {
                        (None, Some(gts_id), String::new())
                    }
                }
                Err(e) => (None, None, format!("Invalid query: {e}")),
            }
        }
    }

    fn matches_id_pattern(
        entity_id: &GtsId,
        base_pattern: &str,
        is_wildcard: bool,
        wildcard_pattern: Option<&GtsIdPattern>,
        exact_gts_id: Option<&GtsId>,
    ) -> bool {
        if is_wildcard && let Some(pattern) = wildcard_pattern {
            // OP#4 allows a final bare `~*` to match an empty suffix, while
            // OP#10 queries require the wildcard position to be present in the
            // stored ID. Preserve that query-specific chain-depth constraint.
            return entity_id.segments().len() >= pattern.segments().len()
                && entity_id.matches_pattern(pattern);
        }

        // For non-wildcard patterns, use matches_pattern to support version flexibility
        if let Some(_exact) = exact_gts_id {
            match GtsIdPattern::try_new(base_pattern) {
                Ok(pattern_as_wildcard) => entity_id.matches_pattern(&pattern_as_wildcard),
                Err(_) => entity_id.id() == base_pattern,
            }
        } else {
            entity_id.id() == base_pattern
        }
    }

    fn matches_filters(entity_content: &Value, filters: &HashMap<String, String>) -> bool {
        if filters.is_empty() {
            return true;
        }

        if let Some(obj) = entity_content.as_object() {
            for (key, value) in filters {
                let entity_value = obj.get(key).map_or_else(String::new, ToString::to_string);

                // Support wildcard in filter values
                if value == "*" {
                    if entity_value.is_empty() || entity_value == "null" {
                        return false;
                    }
                } else if entity_value != format!("\"{value}\"") && entity_value != *value {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}

// Helper trait for string partitioning
trait StringPartition {
    fn partition(&self, delimiter: char) -> (&str, &str, &str);
}

impl StringPartition for str {
    fn partition(&self, delimiter: char) -> (&str, &str, &str) {
        if let Some(pos) = self.find(delimiter) {
            let (before, after_with_delim) = self.split_at(pos);
            let after = &after_with_delim[delimiter.len_utf8()..];
            (before, &after_with_delim[..delimiter.len_utf8()], after)
        } else {
            (self, "", "")
        }
    }
}
#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;
