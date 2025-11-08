pub mod entities;
pub mod files_reader;
pub mod gts;
pub mod ops;
pub mod path_resolver;
pub mod schema_cast;
pub mod store;

#[cfg(test)]
mod gts_tests;

#[cfg(test)]
#[path = "ops_tests.rs"]
mod ops_tests;

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

// Re-export commonly used types
pub use entities::{GtsConfig, GtsEntity, GtsFile, ValidationError, ValidationResult};
pub use files_reader::GtsFileReader;
pub use gts::{GtsError, GtsID, GtsIdSegment, GtsWildcard};
pub use ops::GtsOps;
pub use path_resolver::JsonPathResolver;
pub use schema_cast::{GtsEntityCastResult, SchemaCastError};
pub use store::{GtsReader, GtsStore, GtsStoreQueryResult, StoreError};
