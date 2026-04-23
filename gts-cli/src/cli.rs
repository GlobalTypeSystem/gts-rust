use anyhow::Result;
use clap::{Parser, Subcommand};
use gts::GtsOps;
use std::io::Write;

use crate::gen_instances::generate_instances_from_rust;
use crate::gen_schemas::generate_schemas_from_rust;
use crate::server::GtsHttpServer;

#[derive(Parser)]
#[command(name = "gts")]
#[command(about = "GTS helpers CLI (demo)", long_about = None)]
pub struct Cli {
    /// Increase verbosity (can be used multiple times)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Path to optional GTS config JSON to override defaults
    #[arg(long)]
    pub config: Option<String>,

    /// Path to json and schema files or directories (global default)
    #[arg(long)]
    pub path: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Validate a GTS ID format
    ValidateId {
        #[arg(long)]
        gts_id: String,
    },
    /// Parse a GTS ID into its components
    ParseId {
        #[arg(long)]
        gts_id: String,
    },
    /// Match a GTS ID against a pattern
    MatchIdPattern {
        #[arg(long)]
        pattern: String,
        #[arg(long)]
        candidate: String,
    },
    /// Generate UUID from a GTS ID
    Uuid {
        #[arg(long)]
        gts_id: String,
        #[arg(long, default_value = "major")]
        scope: String,
    },
    /// Validate an instance against its schema
    ValidateInstance {
        #[arg(long)]
        gts_id: String,
    },
    /// Validate a schema against its base schema (OP#12 schema-vs-schema)
    ValidateSchema {
        #[arg(long)]
        schema_id: String,
    },
    /// Validate an entity (instance or schema) by GTS ID
    ValidateEntity {
        #[arg(long)]
        gts_id: String,
    },
    /// Resolve relationships for an entity
    ResolveRelationships {
        #[arg(long)]
        gts_id: String,
    },
    /// Check compatibility between two schemas
    Compatibility {
        #[arg(long)]
        old_schema_id: String,
        #[arg(long)]
        new_schema_id: String,
    },
    /// Cast an instance or schema to a target schema
    Cast {
        #[arg(long)]
        from_id: String,
        #[arg(long)]
        to_schema_id: String,
    },
    /// Query entities using an expression
    Query {
        #[arg(long)]
        expr: String,
        #[arg(long, default_value = "100")]
        limit: usize,
    },
    /// Get attribute value from a GTS entity
    Attr {
        #[arg(long)]
        gts_with_path: String,
    },
    /// List all entities
    List {
        #[arg(long, default_value = "100")]
        limit: usize,
    },
    /// Start the GTS HTTP server
    Server {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "8000")]
        port: u16,
    },
    /// Generate `OpenAPI` specification
    OpenapiSpec {
        #[arg(long)]
        out: String,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "8000")]
        port: u16,
    },
    /// Generate GTS artifacts from Rust source code with `#[struct_to_gts_schema]` /
    /// `#[gts_well_known_instance]` annotations
    GenerateFromRust {
        /// Source directory or file to scan for annotated items
        #[arg(long)]
        source: String,
        /// Output directory for generated files (optional: uses paths from macro if not specified)
        #[arg(long)]
        output: Option<String>,
        /// Exclude patterns (can be specified multiple times). Supports glob patterns.
        /// Example: --exclude "tests/*" --exclude "examples/*"
        #[arg(long, action = clap::ArgAction::Append)]
        exclude: Vec<String>,
        /// What to generate: schemas (default), instances, or all
        #[arg(long, default_value = "schemas")]
        mode: GenerateMode,
    },
}

/// Controls what `generate-from-rust` generates.
#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum GenerateMode {
    /// Generate JSON schemas from `#[struct_to_gts_schema]` annotations (default)
    Schemas,
    /// Generate well-known instance JSON files from `#[gts_well_known_instance]` annotations
    Instances,
    /// Generate both schemas and instances
    All,
}
/// Run the CLI application
///
/// # Errors
///
/// Returns an error if command execution fails
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    run_with_cli(cli).await
}

/// Execute CLI commands with a parsed Cli struct
/// This function is separated from `run()` to allow for testing
///
/// # Errors
///
/// Returns an error if:
/// - Configuration loading fails
/// - File I/O operations fail
/// - Command execution fails
pub async fn run_with_cli(cli: Cli) -> Result<()> {
    // Set up logging to match Python implementation
    // WARNING (no -v), INFO (-v), DEBUG (-vv)
    let log_level = match cli.verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        _ => tracing::Level::DEBUG,
    };

    // Only initialize logging if not already initialized (for testing)
    let _ = tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .try_init();

    run_command(cli).await
}

/// Execute a command with the given CLI configuration
async fn run_command(cli: Cli) -> Result<()> {
    // Parse path into Vec<String>
    let path = cli.path.map(|p| vec![p]);

    // Create GtsOps
    let mut ops = GtsOps::new(path, cli.config, cli.verbose as usize);

    match cli.command {
        Commands::Server { host, port } => {
            println!("starting the server @ http://{host}:{port}");
            if cli.verbose == 0 {
                println!("use --verbose to see server logs");
            }
            let server = GtsHttpServer::new(ops, host.clone(), port, cli.verbose);
            server.run().await?;
        }
        Commands::OpenapiSpec { out, host, port } => {
            let server = GtsHttpServer::new(ops, host, port, cli.verbose);
            let spec = server.openapi_spec();
            std::fs::write(&out, serde_json::to_string_pretty(&spec)?)?;
            let result = serde_json::json!({
                "ok": true,
                "out": out
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::ValidateId { gts_id } => {
            let result = GtsOps::validate_id(&gts_id);
            print_result(&result)?;
        }
        Commands::ParseId { gts_id } => {
            let result = GtsOps::parse_id(&gts_id);
            print_result(&result)?;
        }
        Commands::MatchIdPattern { pattern, candidate } => {
            let result = GtsOps::match_id_pattern(&candidate, &pattern);
            print_result(&result)?;
        }
        Commands::Uuid { gts_id, scope: _ } => {
            let result = GtsOps::uuid(&gts_id);
            print_result(&result)?;
        }
        Commands::ValidateInstance { gts_id } => {
            let result = ops.validate_instance(&gts_id);
            print_result(&result)?;
        }
        Commands::ValidateSchema { schema_id } => {
            let result = ops.validate_schema(&schema_id);
            print_result(&result)?;
        }
        Commands::ValidateEntity { gts_id } => {
            let result = ops.validate_entity(&gts_id);
            print_result(&result)?;
        }
        Commands::ResolveRelationships { gts_id } => {
            let result = ops.schema_graph(&gts_id);
            print_result(&result)?;
        }
        Commands::Compatibility {
            old_schema_id,
            new_schema_id,
        } => {
            let result = ops.compatibility(&old_schema_id, &new_schema_id);
            print_result(&result)?;
        }
        Commands::Cast {
            from_id,
            to_schema_id,
        } => {
            let result = ops.cast(&from_id, &to_schema_id);
            print_result(&result)?;
        }
        Commands::Query { expr, limit } => {
            let result = ops.query(&expr, limit);
            print_result(&result)?;
        }
        Commands::Attr { gts_with_path } => {
            let result = ops.attr(&gts_with_path);
            print_result(&result)?;
        }
        Commands::List { limit } => {
            let result = ops.get_entities(limit);
            print_result(&result)?;
        }
        Commands::GenerateFromRust {
            source,
            output,
            exclude,
            mode,
        } => match mode {
            GenerateMode::Schemas => {
                generate_schemas_from_rust(&source, output.as_deref(), &exclude, cli.verbose)?;
            }
            GenerateMode::Instances => {
                generate_instances_from_rust(&source, output.as_deref(), &exclude, cli.verbose)?;
            }
            GenerateMode::All => {
                generate_schemas_from_rust(&source, output.as_deref(), &exclude, cli.verbose)?;
                generate_instances_from_rust(&source, output.as_deref(), &exclude, cli.verbose)?;
            }
        },
    }

    Ok(())
}

fn print_result<T: serde::Serialize>(value: &T) -> Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    writeln!(handle)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_validate_id() {
        let args = vec!["gts", "validate-id", "--gts-id", "test:schema:v1"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.verbose, 0);
        assert!(cli.config.is_none());
        assert!(cli.path.is_none());

        match cli.command {
            Commands::ValidateId { gts_id } => {
                assert_eq!(gts_id, "test:schema:v1");
            }
            _ => panic!("Expected ValidateId command"),
        }
    }

    #[test]
    fn test_cli_parse_with_verbose() {
        let args = vec!["gts", "-vv", "parse-id", "--gts-id", "test:schema:v1"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.verbose, 2);

        match cli.command {
            Commands::ParseId { gts_id } => {
                assert_eq!(gts_id, "test:schema:v1");
            }
            _ => panic!("Expected ParseId command"),
        }
    }

    #[test]
    fn test_cli_parse_with_config_and_path() {
        let args = vec![
            "gts",
            "--config",
            "/path/to/config.json",
            "--path",
            "/path/to/data",
            "list",
            "--limit",
            "50",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.config, Some("/path/to/config.json".to_owned()));
        assert_eq!(cli.path, Some("/path/to/data".to_owned()));

        match cli.command {
            Commands::List { limit } => {
                assert_eq!(limit, 50);
            }
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn test_cli_parse_generate_from_rust() {
        let args = vec![
            "gts",
            "generate-from-rust",
            "--source",
            "/src/path",
            "--output",
            "/out/path",
            "--exclude",
            "tests/*",
            "--exclude",
            "examples/*",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::GenerateFromRust {
                source,
                output,
                exclude,
                mode,
            } => {
                assert_eq!(source, "/src/path");
                assert_eq!(output, Some("/out/path".to_owned()));
                assert_eq!(exclude, vec!["tests/*", "examples/*"]);
                assert_eq!(mode, GenerateMode::Schemas);
            }
            _ => panic!("Expected GenerateFromRust command"),
        }
    }

    #[test]
    fn test_cli_parse_server_command() {
        let args = vec!["gts", "server", "--host", "0.0.0.0", "--port", "3000"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Server { host, port } => {
                assert_eq!(host, "0.0.0.0");
                assert_eq!(port, 3000);
            }
            _ => panic!("Expected Server command"),
        }
    }

    #[test]
    fn test_cli_parse_match_id_pattern() {
        let args = vec![
            "gts",
            "match-id-pattern",
            "--pattern",
            "test:*:v1",
            "--candidate",
            "test:schema:v1",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::MatchIdPattern { pattern, candidate } => {
                assert_eq!(pattern, "test:*:v1");
                assert_eq!(candidate, "test:schema:v1");
            }
            _ => panic!("Expected MatchIdPattern command"),
        }
    }

    #[test]
    fn test_print_result_json() {
        use serde_json::json;

        // Test that print_result can serialize a value
        let test_value = json!({
            "ok": true,
            "message": "test"
        });

        // Just verify it doesn't panic
        // We can't easily capture stdout in this test, but we can verify it compiles and runs
        let result = print_result(&test_value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cli_parse_uuid_command() {
        let args = vec!["gts", "uuid", "--gts-id", "test:schema:v1"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Uuid { gts_id, scope } => {
                assert_eq!(gts_id, "test:schema:v1");
                assert_eq!(scope, "major");
            }
            _ => panic!("Expected Uuid command"),
        }
    }

    #[test]
    fn test_cli_parse_uuid_with_scope() {
        let args = vec![
            "gts",
            "uuid",
            "--gts-id",
            "test:schema:v1",
            "--scope",
            "minor",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Uuid { gts_id, scope } => {
                assert_eq!(gts_id, "test:schema:v1");
                assert_eq!(scope, "minor");
            }
            _ => panic!("Expected Uuid command"),
        }
    }

    #[test]
    fn test_cli_parse_validate_instance() {
        let args = vec![
            "gts",
            "validate-instance",
            "--gts-id",
            "test:schema:instance:v1",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::ValidateInstance { gts_id } => {
                assert_eq!(gts_id, "test:schema:instance:v1");
            }
            _ => panic!("Expected ValidateInstance command"),
        }
    }

    #[test]
    fn test_cli_parse_resolve_relationships() {
        let args = vec!["gts", "resolve-relationships", "--gts-id", "test:schema:v1"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::ResolveRelationships { gts_id } => {
                assert_eq!(gts_id, "test:schema:v1");
            }
            _ => panic!("Expected ResolveRelationships command"),
        }
    }

    #[test]
    fn test_cli_parse_compatibility() {
        let args = vec![
            "gts",
            "compatibility",
            "--old-schema-id",
            "test:schema:v1",
            "--new-schema-id",
            "test:schema:v2",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Compatibility {
                old_schema_id,
                new_schema_id,
            } => {
                assert_eq!(old_schema_id, "test:schema:v1");
                assert_eq!(new_schema_id, "test:schema:v2");
            }
            _ => panic!("Expected Compatibility command"),
        }
    }

    #[test]
    fn test_cli_parse_cast() {
        let args = vec![
            "gts",
            "cast",
            "--from-id",
            "test:schema:instance:v1",
            "--to-schema-id",
            "test:schema:v2",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Cast {
                from_id,
                to_schema_id,
            } => {
                assert_eq!(from_id, "test:schema:instance:v1");
                assert_eq!(to_schema_id, "test:schema:v2");
            }
            _ => panic!("Expected Cast command"),
        }
    }

    #[test]
    fn test_cli_parse_query() {
        let args = vec!["gts", "query", "--expr", "test:*", "--limit", "25"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Query { expr, limit } => {
                assert_eq!(expr, "test:*");
                assert_eq!(limit, 25);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_cli_parse_query_default_limit() {
        let args = vec!["gts", "query", "--expr", "test:*"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Query { expr, limit } => {
                assert_eq!(expr, "test:*");
                assert_eq!(limit, 100);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_cli_parse_attr() {
        let args = vec![
            "gts",
            "attr",
            "--gts-with-path",
            "test:schema:instance:v1@field.nested",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Attr { gts_with_path } => {
                assert_eq!(gts_with_path, "test:schema:instance:v1@field.nested");
            }
            _ => panic!("Expected Attr command"),
        }
    }

    #[test]
    fn test_cli_parse_list_default_limit() {
        let args = vec!["gts", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::List { limit } => {
                assert_eq!(limit, 100);
            }
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn test_cli_parse_openapi_spec() {
        let args = vec![
            "gts",
            "openapi-spec",
            "--out",
            "/path/to/openapi.json",
            "--host",
            "api.example.com",
            "--port",
            "443",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::OpenapiSpec { out, host, port } => {
                assert_eq!(out, "/path/to/openapi.json");
                assert_eq!(host, "api.example.com");
                assert_eq!(port, 443);
            }
            _ => panic!("Expected OpenapiSpec command"),
        }
    }

    #[test]
    fn test_cli_parse_openapi_spec_defaults() {
        let args = vec!["gts", "openapi-spec", "--out", "/path/to/openapi.json"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::OpenapiSpec { out, host, port } => {
                assert_eq!(out, "/path/to/openapi.json");
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 8000);
            }
            _ => panic!("Expected OpenapiSpec command"),
        }
    }

    #[test]
    fn test_cli_parse_generate_from_rust_minimal() {
        let args = vec!["gts", "generate-from-rust", "--source", "/src/path"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::GenerateFromRust {
                source,
                output,
                exclude,
                mode,
            } => {
                assert_eq!(source, "/src/path");
                assert_eq!(output, None);
                assert!(exclude.is_empty());
                assert_eq!(mode, GenerateMode::Schemas);
            }
            _ => panic!("Expected GenerateFromRust command"),
        }
    }

    #[test]
    fn test_cli_parse_server_defaults() {
        let args = vec!["gts", "server"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Server { host, port } => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 8000);
            }
            _ => panic!("Expected Server command"),
        }
    }

    #[test]
    fn test_cli_multiple_verbose_flags() {
        let args = vec!["gts", "-vvv", "validate-id", "--gts-id", "test:schema:v1"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.verbose, 3);
    }

    #[test]
    fn test_cli_global_config_option() {
        let args = vec![
            "gts",
            "--config",
            "/path/to/config.json",
            "validate-id",
            "--gts-id",
            "test:schema:v1",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.config, Some("/path/to/config.json".to_owned()));
    }

    #[test]
    fn test_cli_global_path_option() {
        let args = vec!["gts", "--path", "/path/to/data", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.path, Some("/path/to/data".to_owned()));
    }
}
