// gts:ignore - fixtures below must not be picked up by `make generate-schemas`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! `generate-from-rust` exports packages that use `overrides(...)` from compiled
//! code. Each test builds a fixture crate and runs the `gts` binary from outside it.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;
use walkdir::WalkDir;

const BASE_RS: &str = r#"
use gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_cli.event.v1~",
    description = "Event envelope",
    properties = "id,subject_id,subject_type,payload"
)]
#[derive(Debug)]
pub struct EventV1<P> {
    pub id: gts::GtsInstanceId,
    #[schemars(length(min = 1, max = 1024))]
    pub subject_id: String,
    pub subject_type: GtsTypeId,
    pub payload: P,
}
"#;

/// A crate that only builds when its own `.cargo/config.toml` was read. Names
/// must differ: same-named fixtures would share artifacts in the target dir.
fn fixture(name: &str, manifest_extra: &str, files: &[(&str, &str)]) -> TempDir {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{name}"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
gts = {{ path = '{gts}' }}
gts-macros = {{ path = '{macros}' }}
schemars = {{ version = "1.2", features = ["uuid1"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
{manifest_extra}
[workspace]
"#,
            gts = workspace.join("gts").display(),
            macros = workspace.join("gts-macros").display(),
        ),
    )
    .unwrap();
    fs::copy(workspace.join("Cargo.lock"), root.join("Cargo.lock")).unwrap();
    fs::create_dir_all(root.join(".cargo")).unwrap();
    fs::write(
        root.join(".cargo/config.toml"),
        "[env]\nGTS_FIXTURE_CONFIG = \"package\"\n",
    )
    .unwrap();
    for (path, content) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let content = if path.ends_with("src/lib.rs") {
            format!("const _: &str = env!(\"GTS_FIXTURE_CONFIG\");\n{content}")
        } else {
            (*content).to_owned()
        };
        fs::write(path, content).unwrap();
    }
    dir
}

/// Runs `gts generate-from-rust --source <source> <args>` from this crate's
/// directory, outside the fixture.
fn generate(source: &Path, args: &[&str]) -> (Output, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_gts"))
        .args(["generate-from-rust", "--source"])
        .arg(source)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("gts-export-fixtures"),
        )
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output, text)
}

fn schema_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.path().to_path_buf())
        .filter(|p| {
            p.to_string_lossy().ends_with(".schema.json") && !p.starts_with(dir.join("target"))
        })
        .collect();
    files.sort();
    files
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn golden_override_chains_match_compiled_schemas() {
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../gts-macros/tests/golden");
    let cases = [
        "overrides_two_levels",
        "overrides_three_levels",
        "overrides_four_levels",
    ];
    let sources: Vec<(String, String)> = cases
        .iter()
        .map(|case| {
            (
                format!("src/{case}.rs"),
                fs::read_to_string(golden.join(format!("{case}.rs"))).unwrap(),
            )
        })
        .collect();
    let lib = cases.map(|case| format!("pub mod {case};")).join("\n");
    let mut files: Vec<(&str, &str)> = vec![("src/lib.rs", &lib)];
    files.extend(sources.iter().map(|(p, c)| (p.as_str(), c.as_str())));
    let dir = fixture("fixture-golden", "", &files);

    let (output, text) = generate(dir.path(), &[]);
    assert!(output.status.success(), "{text}");

    let mut expected = Vec::new();
    for case in cases {
        for entry in fs::read_dir(golden.join(case)).unwrap() {
            let path = entry.unwrap().path();
            let generated = dir
                .path()
                .join("src/schemas")
                .join(path.file_name().unwrap());
            assert_eq!(
                read_json(&generated),
                read_json(&path),
                "{}",
                path.display()
            );
            expected.push(generated);
        }
    }
    expected.sort();
    assert_eq!(schema_files(dir.path()), expected);
}

const ORDERS_RS: &str = r#"
use gts::GtsTypeId;
use gts_macros::struct_to_gts_schema;
use crate::base::EventV1;

const ORDER_TYPE: &str = gts_macros::gts_id!("x.core.orders.order.v1~");

#[derive(schemars::JsonSchema)]
pub struct UuidSubject {
    #[schemars(length(min = 36, max = 36), extend("format" = "uuid"))]
    pub subject_id: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.placed.v1~",
    description = "Named helper",
    properties = "order_id",
    overrides(EventV1 = UuidSubject)
)]
#[derive(Debug)]
pub struct PlacedV1 {
    pub order_id: String,
}

pub mod inline {
    use super::*;

    #[gts_macros::struct_to_gts_schema(
        dir_path = "schemas",
        base = EventV1,
        type_id = gts_id!("x.test.ovr_cli.event.v1~x.test.ovr_cli.cancelled.v1~"),
        description = "Inline fields",
        properties = "reason",
        overrides(EventV1 = {
            #[schemars(length(min = 1, max = 64))]
            subject_id: String,
            #[schemars(extend("x-gts-type-ref" = ORDER_TYPE))]
            subject_type: GtsTypeId,
        })
    )]
    #[derive(Debug)]
    pub struct CancelledV1 {
        pub reason: String,
    }
}
"#;

/// Exported from the default-enabled feature's test target.
const FEATURED_RS: &str = r#"
use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_cli.featured.v1~",
    description = "Built with default features",
    properties = "id"
)]
#[derive(Debug)]
pub struct FeaturedV1 {
    pub id: gts::GtsInstanceId,
}
"#;

const MANUAL_RS: &str = r#"
fn main() {
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/MAIN_RAN"), "").unwrap();
}
"#;

const SPLIT_MANIFEST: &str = r#"
[features]
default = ["extra"]
extra = []

[[test]]
name = "featured"
required-features = ["extra"]

[[test]]
name = "manual"
harness = false
"#;

fn split_fixture(name: &str) -> TempDir {
    fixture(
        name,
        SPLIT_MANIFEST,
        &[
            ("src/lib.rs", "pub mod base;\npub mod orders;\n"),
            ("src/base.rs", BASE_RS),
            ("src/orders.rs", ORDERS_RS),
            ("tests/featured.rs", FEATURED_RS),
            ("tests/manual.rs", MANUAL_RS),
        ],
    )
}

#[test]
fn split_files_export_base_and_named_and_inline_overrides() {
    let dir = split_fixture("fixture-split");
    let (output, text) = generate(dir.path(), &[]);
    assert!(output.status.success(), "{text}");
    assert!(
        !dir.path().join("MAIN_RAN").exists(),
        "harness = false main ran"
    );

    let schemas = dir.path().join("src/schemas");
    // The base file has no overrides but is exported compiled, constraints included.
    let base = read_json(&schemas.join("gts.x.test.ovr_cli.event.v1~.schema.json"));
    assert_eq!(base["properties"]["subject_id"]["minLength"], 1);
    assert_eq!(base["properties"]["subject_id"]["maxLength"], 1024);

    let placed = read_json(
        &schemas.join("gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.placed.v1~.schema.json"),
    );
    assert_eq!(
        placed["allOf"][0]["$ref"],
        "gts://gts.x.test.ovr_cli.event.v1~"
    );
    let own = &placed["allOf"][1];
    assert_eq!(
        own["properties"]["subject_id"],
        serde_json::json!({ "type": "string", "format": "uuid", "minLength": 36, "maxLength": 36 })
    );
    assert_eq!(own["required"], serde_json::json!(["subject_id"]));
    assert_eq!(
        own["properties"]["payload"]["required"],
        serde_json::json!(["order_id"])
    );

    let cancelled = read_json(
        &schemas.join("gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.cancelled.v1~.schema.json"),
    );
    let own = &cancelled["allOf"][1];
    assert_eq!(own["properties"]["subject_id"]["maxLength"], 64);
    assert_eq!(
        own["properties"]["subject_type"]["x-gts-type-ref"],
        "gts.x.core.orders.order.v1~"
    );
    assert_eq!(
        own["required"],
        serde_json::json!(["subject_id", "subject_type"])
    );

    assert!(
        dir.path()
            .join("tests/schemas/gts.x.test.ovr_cli.featured.v1~.schema.json")
            .exists()
    );
    assert_eq!(schema_files(dir.path()).len(), 4, "{text}");
}

#[test]
fn single_overriding_file_is_exported_within_its_package() {
    let dir = split_fixture("fixture-single-file");
    let (output, text) = generate(&dir.path().join("src/orders.rs"), &[]);
    assert!(output.status.success(), "{text}");
    let names: Vec<String> = schema_files(dir.path())
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.cancelled.v1~.schema.json",
            "gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.placed.v1~.schema.json",
        ]
    );
}

const WIDENING_RS: &str = r#"
use gts_macros::struct_to_gts_schema;
use crate::base::EventV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EventV1,
    type_id = "gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.wide.v1~",
    description = "Widens subject_id",
    properties = "note",
    overrides(EventV1 = { #[schemars(length(max = 4096))] subject_id: String })
)]
#[derive(Debug)]
pub struct WideV1 {
    pub note: String,
}
"#;

#[test]
fn invalid_override_fails_and_excluded_files_are_not_exported() {
    let dir = fixture(
        "fixture-invalid",
        "",
        &[
            (
                "src/lib.rs",
                "pub mod base;\npub mod orders;\npub mod widening;\n",
            ),
            ("src/base.rs", BASE_RS),
            ("src/orders.rs", ORDERS_RS),
            ("src/widening.rs", WIDENING_RS),
        ],
    );
    let (output, text) = generate(dir.path(), &[]);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("invalid overrides on 'gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.wide.v1~'"),
        "{text}"
    );
    assert_eq!(schema_files(dir.path()), Vec::<PathBuf>::new());

    let (output, text) = generate(dir.path(), &["--exclude", "widening.rs"]);
    assert!(output.status.success(), "{text}");
    assert_eq!(schema_files(dir.path()).len(), 3, "{text}");
}

const DUPLICATE_RS: &str = r#"
use gts_macros::struct_to_gts_schema;
use crate::base::EventV1;

pub mod narrow {
    use super::*;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = EventV1,
        type_id = "gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.same.v1~",
        description = "Same type ID",
        properties = "note",
        overrides(EventV1 = { #[schemars(length(min = 1, max = 16))] subject_id: String })
    )]
    #[derive(Debug)]
    pub struct SameV1 {
        pub note: String,
    }
}

pub mod narrower {
    use super::*;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = EventV1,
        type_id = "gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.same.v1~",
        description = "Same type ID",
        properties = "note",
        overrides(EventV1 = { #[schemars(length(min = 1, max = 8))] subject_id: String })
    )]
    #[derive(Debug)]
    pub struct SameV1 {
        pub note: String,
    }
}
"#;

#[test]
fn one_type_id_with_different_schemas_fails_without_writing() {
    let dir = fixture(
        "fixture-duplicate",
        "",
        &[
            ("src/lib.rs", "pub mod base;\npub mod duplicate;\n"),
            ("src/base.rs", BASE_RS),
            ("src/duplicate.rs", DUPLICATE_RS),
        ],
    );
    let (output, text) = generate(dir.path(), &[]);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("conflicting schemas"), "{text}");
    assert!(text.contains("x.test.ovr_cli.same.v1~"), "{text}");
    assert_eq!(schema_files(dir.path()), Vec::<PathBuf>::new());
}

const UNEXPORTABLE_RS: &str = r#"
use gts_macros::struct_to_gts_schema;

pub trait Marker {}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_cli.bounded.v1~",
    description = "Bounded",
    properties = "id,payload"
)]
#[derive(Debug)]
pub struct BoundedV1<P: Marker> {
    pub id: gts::GtsInstanceId,
    pub payload: P,
}

pub fn build() {
    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = "gts.x.test.ovr_cli.local.v1~",
        description = "Local",
        properties = "id"
    )]
    #[derive(Debug)]
    struct LocalV1 {
        id: gts::GtsInstanceId,
    }
}
"#;

#[test]
fn unexportable_declarations_are_reported() {
    let dir = fixture(
        "fixture-unexportable",
        "",
        &[
            (
                "src/lib.rs",
                "pub mod base;\npub mod orders;\npub mod unexportable;\n",
            ),
            ("src/base.rs", BASE_RS),
            ("src/orders.rs", ORDERS_RS),
            ("src/unexportable.rs", UNEXPORTABLE_RS),
            // Not part of any target.
            ("src/orphan.rs", FEATURED_RS),
        ],
    );
    let (output, text) = generate(dir.path(), &[]);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("`LocalV1` is declared inside a function or block"),
        "{text}"
    );
    assert!(
        text.contains("`BoundedV1` has its own generic bounds"),
        "{text}"
    );
    assert_eq!(schema_files(dir.path()), Vec::<PathBuf>::new());

    let (output, text) = generate(dir.path(), &["--exclude", "unexportable.rs"]);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("these declarations were not exported"),
        "{text}"
    );
    assert!(text.contains("orphan.rs:"), "{text}");
    assert!(text.contains("`FeaturedV1`"), "{text}");
    assert_eq!(schema_files(dir.path()), Vec::<PathBuf>::new());
}

/// Base and child in separate workspace packages: the base package has no
/// overrides but is still exported compiled, constraints included.
#[test]
fn workspace_base_package_keeps_constraints() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let deps = format!(
        r#"gts = {{ path = '{gts}' }}
gts-macros = {{ path = '{macros}' }}
schemars = {{ version = "1.2", features = ["uuid1"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
"#,
        gts = workspace.join("gts").display(),
        macros = workspace.join("gts-macros").display(),
    );
    let package = |name: &str, extra: &str| {
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
             [dependencies]\n{deps}{extra}"
        )
    };
    let probe = "const _: &str = env!(\"GTS_FIXTURE_CONFIG\");\n";
    let files = [
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"base\", \"orders\"]\nresolver = \"3\"\n".to_owned(),
        ),
        (
            ".cargo/config.toml",
            "[env]\nGTS_FIXTURE_CONFIG = \"workspace\"\n".to_owned(),
        ),
        ("base/Cargo.toml", package("fixture-ws-base", "")),
        ("base/src/lib.rs", format!("{probe}{BASE_RS}")),
        (
            "orders/Cargo.toml",
            package(
                "fixture-ws-orders",
                "fixture-ws-base = { path = \"../base\" }\n",
            ),
        ),
        (
            "orders/src/lib.rs",
            format!(
                "{probe}{}",
                ORDERS_RS.replace("use crate::base::EventV1;", "use fixture_ws_base::EventV1;")
            ),
        ),
    ];
    for (path, content) in &files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fs::copy(workspace.join("Cargo.lock"), root.join("Cargo.lock")).unwrap();

    let (output, text) = generate(root, &[]);
    assert!(output.status.success(), "{text}");
    let base = read_json(&root.join("base/src/schemas/gts.x.test.ovr_cli.event.v1~.schema.json"));
    assert_eq!(base["properties"]["subject_id"]["minLength"], 1, "{text}");
    assert_eq!(
        base["properties"]["subject_id"]["maxLength"], 1024,
        "{text}"
    );
    let placed = read_json(&root.join(
        "orders/src/schemas/gts.x.test.ovr_cli.event.v1~x.test.ovr_cli.placed.v1~.schema.json",
    ));
    assert_eq!(
        placed["allOf"][1]["properties"]["subject_id"]["format"],
        "uuid"
    );
    assert_eq!(schema_files(root).len(), 3, "{text}");
}

#[test]
fn declarations_outside_packages_fail_when_overrides_are_in_use() {
    let dir = fixture(
        "fixture-outside",
        "",
        &[
            ("pkg/src/lib.rs", "pub mod base;\npub mod orders;\n"),
            ("pkg/src/base.rs", BASE_RS),
            ("pkg/src/orders.rs", ORDERS_RS),
            ("loose/featured.rs", FEATURED_RS),
        ],
    );
    // The fixture manifest sits at the root; move the package one level down so
    // that `loose/` is outside it.
    fs::rename(
        dir.path().join("Cargo.toml"),
        dir.path().join("pkg/Cargo.toml"),
    )
    .unwrap();
    fs::rename(
        dir.path().join("Cargo.lock"),
        dir.path().join("pkg/Cargo.lock"),
    )
    .unwrap();
    let (output, text) = generate(dir.path(), &[]);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("not inside a Cargo package"), "{text}");
    let loose_file = Path::new("loose").join("featured.rs");
    assert!(text.contains(loose_file.to_str().unwrap()), "{text}");
    assert_eq!(schema_files(dir.path()), Vec::<PathBuf>::new());
}
