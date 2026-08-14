pub mod entities;
pub mod files_reader;
pub mod gts;
pub mod ops;
pub mod path_resolver;
pub mod schema;
pub mod schema_cast;
pub mod schema_derivation;
pub mod schema_evolution;
pub mod schema_modifiers;
pub mod schema_narrow;
pub mod schema_refs;
pub mod schema_resolver;
mod schema_semantics;
pub mod schema_traits;
pub mod store;
#[doc(hidden)]
pub mod testing;
pub mod x_gts_ref;

/// GTS specification revision implemented by compatibility and validation logic.
pub const GTS_SPECIFICATION_VERSION: &str = "0.13";

/// Version of this Rust implementation.
pub const GTS_IMPLEMENTATION_VERSION: &str = env!("CARGO_PKG_VERSION");

// Re-export commonly used types
pub use entities::{GtsConfig, GtsEntity, GtsFile, ValidationError, ValidationResult};
pub use files_reader::GtsFileReader;
#[allow(deprecated)]
pub use gts::GtsSchemaId;
pub use gts::{
    DEFAULT_GTS_ID_PREFIX, GTS_ID_MAX_LENGTH, GTS_ID_PREFIX, GTS_ID_PREFIX_ENV, GTS_ID_URI_PREFIX,
    GtsId, GtsIdError, GtsIdPattern, GtsIdPatternSegment, GtsIdSegment, GtsIdSegmentParts,
    GtsInstanceId, GtsTypeId, GtsUuidTail,
};
pub use ops::GtsOps;
pub use path_resolver::JsonPathResolver;
pub use schema::{
    GtsDeserialize, GtsDeserializeWrapper, GtsNoDirectDeserialize, GtsNoDirectSerialize, GtsSchema,
    GtsSerialize, GtsSerializeWrapper, JSON_SCHEMA_DRAFT_07, TraitSchemaState, deserialize_gts,
    serialize_gts, strip_schema_metadata,
};
pub use schema_cast::{GtsEntityCastResult, SchemaCastError};
pub use schema_evolution::{
    CompatibilityDiagnostic, CompatibilityFinding, CompatibilityVerdict, ContentModel, ObjectLevel,
    check_backward_compatibility, check_backward_diagnostics, check_forward_compatibility,
    check_forward_diagnostics,
};
pub use schema_narrow::{NarrowError, try_narrow};
pub use schema_refs::{ExtractRefsError, InvalidRefReason, extract_gts_refs};
pub use schema_traits::{GtsTraitsSchema, inline_traits_schema_of};
pub use store::{
    GtsReader, GtsStore, GtsStoreQueryResult, ResolvedType, SchemaComparison, StoreError,
};
pub use x_gts_ref::{XGtsRefValidationError, XGtsRefValidator};
