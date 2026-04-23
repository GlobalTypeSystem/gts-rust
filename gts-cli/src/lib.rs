// Re-export the logging module for use in integration tests
pub mod logging;

// Other modules needed by CLI
pub mod gen_common;
pub mod gen_instances;
pub mod gen_schemas;
pub mod server;

// Re-export CLI types and functions for testing
pub mod cli;
pub use cli::{Cli, Commands, run_with_cli};
