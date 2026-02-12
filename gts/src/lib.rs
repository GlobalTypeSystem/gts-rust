pub mod entities;
pub mod files_reader;
pub mod gts;
pub mod ops;
pub mod path_resolver;
pub mod schema;
pub mod schema_cast;
pub mod store;
pub mod x_gts_ref;

// Re-export commonly used types
pub use entities::{GtsConfig, GtsEntity, GtsFile, ValidationError, ValidationResult};
pub use files_reader::GtsFileReader;
pub use gts::{GtsError, GtsID, GtsIdSegment, GtsInstanceId, GtsSchemaId, GtsWildcard};
pub use ops::GtsOps;
pub use path_resolver::JsonPathResolver;
pub use schema::{
    GtsDeserialize, GtsDeserializeWrapper, GtsNoDirectDeserialize, GtsNoDirectSerialize, GtsSchema,
    GtsSerialize, GtsSerializeWrapper, deserialize_gts, serialize_gts, strip_schema_metadata,
};
pub use schema_cast::{GtsEntityCastResult, SchemaCastError};
pub use store::{GtsReader, GtsStore, GtsStoreQueryResult, StoreError};
pub use x_gts_ref::{XGtsRefValidationError, XGtsRefValidator};
