use gts::GTS_ID_PREFIX;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn declaration(type_id: &str) -> String {
    format!(
        r#"mod models {{
            #[struct_to_gts_schema(
                base = true,
                type_id = {type_id},
                description = "Entity (with details)",
                properties = "id,name,tags"
            )]
            pub struct EntityV1 {{
                pub id: GtsInstanceId,
                pub name: Option<String>,
                pub tags: Vec<String>,
            }}
        }}"#
    )
}

fn generate(
    cwd: &Path,
    source: &Path,
    output: Option<&Path>,
) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gts"));
    command
        .current_dir(cwd)
        .args(["generate-from-rust", "--source"])
        .arg(source);
    if let Some(output) = output {
        command.arg("--output").arg(output);
    }
    command.output()
}

#[test]
fn source_file_uses_default_output_relative_to_cwd() {
    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("src");
    fs::create_dir(&source_dir).unwrap();
    let source = source_dir.join("models.rs");
    fs::write(
        &source,
        declaration(r#"gts_macros::gts_id!("x.test.output.entity.v1~")"#).replace(
            "#[struct_to_gts_schema",
            "#[gts_macros::struct_to_gts_schema",
        ),
    )
    .unwrap();

    let result = generate(temp.path(), &source, None).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let filename = format!("{GTS_ID_PREFIX}x.test.output.entity.v1~.schema.json");
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join("schemas").join(filename)).unwrap(),
    )
    .unwrap();
    assert_eq!(schema["description"], "Entity (with details)");
    assert_eq!(
        schema["properties"]["name"]["type"],
        json!(["string", "null"])
    );
    assert_eq!(schema["required"], json!(["id", "tags"]));
    assert_eq!(schema["properties"]["tags"]["type"], "array");
    assert_eq!(schema["properties"]["tags"]["items"]["type"], "string");
    assert!(!source_dir.join("schemas").exists());
}

#[test]
fn explicit_output_is_final_directory_outside_source() {
    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("src");
    fs::create_dir(&source_dir).unwrap();
    let id = format!("{GTS_ID_PREFIX}x.test.output.entity.v1~");
    fs::write(
        source_dir.join("models.rs"),
        declaration(&format!("{id:?}")),
    )
    .unwrap();

    for output in [temp.path().join("absolute"), "relative".into()] {
        let result = generate(temp.path(), &source_dir, Some(&output)).unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let resolved = temp.path().join(&output);
        assert!(resolved.join(format!("{id}.schema.json")).is_file());
        assert!(!resolved.join("schemas").exists());
    }
}

#[test]
fn removed_dir_path_reports_the_cli_replacement() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("models.rs");
    let content = declaration(r#"gts_id!("x.test.output.entity.v1~")"#)
        .replace("base = true,", r#"dir_path = "schemas", base = true,"#);
    fs::write(&source, content).unwrap();

    let result = generate(temp.path(), &source, None).unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("dir_path has been removed; use --output DIR")
    );
    assert!(!temp.path().join("schemas").exists());
}

#[test]
fn invalid_ids_fail_before_creating_output_directories() {
    for id in ["../../escaped", "/absolute/escaped", "gts.invalid"] {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("models.rs");
        fs::write(&source, declaration(&format!("{id:?}"))).unwrap();
        let output = temp.path().join("new/output");

        let result = generate(temp.path(), &source, Some(&output)).unwrap();
        assert!(!result.status.success());
        assert!(!temp.path().join("new").exists());
        assert!(!temp.path().join("escaped.schema.json").exists());
    }
}

#[test]
fn conflicting_ids_fail_before_writing_and_name_both_sources() {
    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("src");
    fs::create_dir(&source_dir).unwrap();
    let content = declaration(r#"gts_id!("x.test.output.entity.v1~")"#);
    fs::write(source_dir.join("first.rs"), &content).unwrap();
    fs::write(
        source_dir.join("second.rs"),
        content.replace("EntityV1", "OtherV1"),
    )
    .unwrap();

    let result = generate(temp.path(), &source_dir, None).unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    for expected in ["first.rs", "second.rs", "EntityV1", "OtherV1"] {
        assert!(error.contains(expected), "{error}");
    }
    assert!(!temp.path().join("schemas").exists());
}

#[test]
fn identical_declarations_are_written_once_and_regular_files_can_be_regenerated() {
    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("src");
    fs::create_dir(&source_dir).unwrap();
    let content = declaration(r#"gts_id!("x.test.output.entity.v1~")"#);
    fs::write(source_dir.join("first.rs"), &content).unwrap();
    fs::write(source_dir.join("second.rs"), &content).unwrap();

    for _ in 0..2 {
        let result = generate(temp.path(), &source_dir, None).unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("Schemas generated: 1"));
        assert_eq!(
            fs::read_dir(temp.path().join("schemas")).unwrap().count(),
            1
        );
    }
}

#[test]
fn source_read_and_syntax_errors_fail_before_writing() {
    for invalid in [b"pub struct Broken {".as_slice(), b"\xff"] {
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("src");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("valid.rs"),
            declaration(r#"gts_id!("x.test.output.entity.v1~")"#),
        )
        .unwrap();
        fs::write(source_dir.join("invalid.rs"), invalid).unwrap();

        let result = generate(temp.path(), &source_dir, None).unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("invalid.rs"));
        assert!(!temp.path().join("schemas").exists());
    }
}

#[cfg(unix)]
#[test]
fn directory_walk_errors_fail_before_writing() {
    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("src");
    fs::create_dir(&source_dir).unwrap();
    fs::write(
        source_dir.join("valid.rs"),
        declaration(r#"gts_id!("x.test.output.entity.v1~")"#),
    )
    .unwrap();
    std::os::unix::fs::symlink(&source_dir, source_dir.join("loop")).unwrap();

    let result = generate(temp.path(), &source_dir, None).unwrap();
    assert!(!result.status.success());
    assert!(!temp.path().join("schemas").exists());
}

#[cfg(unix)]
#[test]
fn output_file_symlinks_are_rejected_before_any_write() {
    use std::os::unix::fs::symlink;

    for dangling in [false, true] {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("models.rs");
        let content = declaration(r#"gts_id!("x.test.output.aaa.v1~")"#)
            + &declaration(r#"gts_id!("x.test.output.entity.v1~")"#)
                .replace("mod models", "mod other");
        fs::write(&source, content).unwrap();
        let output = temp.path().join("out");
        fs::create_dir(&output).unwrap();
        let target = temp.path().join("outside.json");
        if !dangling {
            fs::write(&target, "unchanged").unwrap();
        }
        symlink(
            &target,
            output.join(format!(
                "{GTS_ID_PREFIX}x.test.output.entity.v1~.schema.json"
            )),
        )
        .unwrap();

        let result = generate(temp.path(), &source, Some(&output)).unwrap();
        assert!(!result.status.success());
        assert!(
            !output
                .join(format!("{GTS_ID_PREFIX}x.test.output.aaa.v1~.schema.json"))
                .exists()
        );
        if dangling {
            assert!(!target.exists());
        } else {
            assert_eq!(fs::read_to_string(&target).unwrap(), "unchanged");
        }
    }
}
