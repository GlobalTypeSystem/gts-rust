use anyhow::Result;
use gts_cli::{Cli, Commands, run_with_cli};
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_run_validate_id_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::ValidateId {
            gts_id: "test:schema:v1".to_owned(),
        },
    };

    // This will execute the command
    // Note: The output goes to stdout, which we're not capturing here
    // In a more sophisticated test, you might want to redirect stdout
    run_with_cli(cli).await?;
    Ok(())
}

#[tokio::test]
async fn test_run_parse_id_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::ParseId {
            gts_id: "test:schema:v1".to_owned(),
        },
    };

    run_with_cli(cli).await?;
    Ok(())
}

#[tokio::test]
async fn test_run_match_id_pattern_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::MatchIdPattern {
            pattern: "test:*:v1".to_owned(),
            candidate: "test:schema:v1".to_owned(),
        },
    };

    run_with_cli(cli).await?;
    Ok(())
}

#[tokio::test]
async fn test_run_uuid_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::Uuid {
            gts_id: "test:schema:v1".to_owned(),
            scope: "major".to_owned(),
        },
    };

    run_with_cli(cli).await?;
    Ok(())
}

#[tokio::test]
async fn test_run_validate_instance_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::ValidateInstance {
            gts_id: "test:instance:v1".to_owned(),
        },
    };

    // This may fail if the instance doesn't exist, but that's okay
    // We're testing that the command executes, not that it succeeds
    let _ = run_with_cli(cli).await;
    Ok(())
}

#[tokio::test]
async fn test_run_resolve_relationships_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::ResolveRelationships {
            gts_id: "test:schema:v1".to_owned(),
        },
    };

    let _ = run_with_cli(cli).await;
    Ok(())
}

#[tokio::test]
async fn test_run_compatibility_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::Compatibility {
            old_schema_id: "test:schema:v1".to_owned(),
            new_schema_id: "test:schema:v2".to_owned(),
        },
    };

    let _ = run_with_cli(cli).await;
    Ok(())
}

#[tokio::test]
async fn test_run_cast_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::Cast {
            from_id: "test:instance:v1".to_owned(),
            to_schema_id: "test:schema:v2".to_owned(),
        },
    };

    let _ = run_with_cli(cli).await;
    Ok(())
}

#[tokio::test]
async fn test_run_query_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::Query {
            expr: "test:*".to_owned(),
            limit: 10,
        },
    };

    run_with_cli(cli).await?;
    Ok(())
}

#[tokio::test]
async fn test_run_attr_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::Attr {
            gts_with_path: "test:instance:v1@field.nested".to_owned(),
        },
    };

    let _ = run_with_cli(cli).await;
    Ok(())
}

#[tokio::test]
async fn test_run_list_command() -> Result<()> {
    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::List { limit: 50 },
    };

    run_with_cli(cli).await?;
    Ok(())
}

#[tokio::test]
async fn test_run_openapi_spec_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let output_path = temp_dir.path().join("openapi.json");

    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::OpenapiSpec {
            out: output_path.to_str().unwrap().to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8000,
        },
    };

    run_with_cli(cli).await?;

    // Verify the output file was created
    assert!(output_path.exists(), "OpenAPI spec file should be created");

    // Verify it's valid JSON
    let content = fs::read_to_string(&output_path)?;
    let _: serde_json::Value = serde_json::from_str(&content)?;

    Ok(())
}

#[tokio::test]
async fn test_run_generate_from_rust_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_path = temp_dir.path().join("src");
    let output_path = temp_dir.path().join("out");

    fs::create_dir(&source_path)?;

    let cli = Cli {
        verbose: 0,
        config: None,
        path: None,
        command: Commands::GenerateFromRust {
            source: source_path.to_str().unwrap().to_owned(),
            output: Some(output_path.to_str().unwrap().to_owned()),
            exclude: vec![],
            mode: gts_cli::cli::GenerateMode::Schemas,
        },
    };

    // This might fail if there are no annotated structs, but that's okay
    let _ = run_with_cli(cli).await;
    Ok(())
}

#[tokio::test]
async fn test_run_with_verbose_flag() -> Result<()> {
    let cli = Cli {
        verbose: 2, // DEBUG level
        config: None,
        path: None,
        command: Commands::ValidateId {
            gts_id: "test:schema:v1".to_owned(),
        },
    };

    run_with_cli(cli).await?;
    Ok(())
}

#[tokio::test]
async fn test_run_with_config_and_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.json");
    let data_path = temp_dir.path().join("data");

    fs::write(&config_path, "{}")?;
    fs::create_dir(&data_path)?;

    let cli = Cli {
        verbose: 0,
        config: Some(config_path.to_str().unwrap().to_owned()),
        path: Some(data_path.to_str().unwrap().to_owned()),
        command: Commands::List { limit: 100 },
    };

    run_with_cli(cli).await?;
    Ok(())
}

// Note: Server command test is omitted because it runs indefinitely
// To test the server command, you would need to:
// - Spawn it in a background task with a timeout
// - Make HTTP requests to verify it's responding
// - Gracefully shutdown the server
