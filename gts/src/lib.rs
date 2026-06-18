pub mod entities;
pub mod files_reader;
pub mod gts;
pub mod ops;
pub mod path_resolver;
pub mod schema;
pub mod schema_cast;
pub mod schema_compat;
pub mod schema_modifiers;
pub mod schema_narrow;
pub mod schema_refs;
pub mod schema_traits;
pub mod store;
#[doc(hidden)]
pub mod testing;
pub mod x_gts_ref;

// Re-export commonly used types
pub use entities::{GtsConfig, GtsEntity, GtsFile, ValidationError, ValidationResult};
pub use files_reader::GtsFileReader;
#[allow(deprecated)]
pub use gts::GtsSchemaId;
pub use gts::{
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
pub use schema_narrow::{NarrowError, try_narrow};
pub use schema_refs::{ExtractRefsError, extract_gts_refs};
pub use schema_traits::{GtsTraitsSchema, inline_traits_schema_of};
pub use store::{GtsReader, GtsStore, GtsStoreQueryResult, ResolvedTypeSchema, StoreError};
pub use x_gts_ref::{XGtsRefValidationError, XGtsRefValidator};
