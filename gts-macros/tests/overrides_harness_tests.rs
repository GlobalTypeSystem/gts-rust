#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Runs `cargo test` on the `overrides_harness/` fixture crate to prove the
//! `#[test]` generated for `overrides(...)` is discovered and executed by the
//! real libtest harness, that an invalid override fails that test, not the
//! build, and that a type the harness cannot reach fails the test build.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

const GENERIC: &str = "valid::gts_overrides_GenericV1";
const VALID_UNIT: &str = "valid::gts_overrides_UnitV1";
const INVALID_UNIT: &str = "invalid::gts_overrides_UnitV1";
const INVALID_PARENT: &str = "invalid::gts_overrides_InvalidParentV1";
const ADMITTED: &str = "admission::gts_overrides_NarrowV1";
const NOT_ADMITTED: &str = "admission::gts_overrides_ClosedV1";

/// Writes the fixture's manifest (path deps on this workspace, the
/// workspace's lock file) and returns it.
fn manifest() -> &'static Path {
    static MANIFEST: OnceLock<PathBuf> = OnceLock::new();
    MANIFEST.get_or_init(|| {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.parent().unwrap();
        let project = Path::new(env!("CARGO_TARGET_TMPDIR")).join("overrides_harness");
        std::fs::create_dir_all(&project).unwrap();
        let manifest = format!(
            r#"[package]
name = "gts-overrides-harness"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
path = '{lib}'

[features]
local = []
shadow = []

[dependencies]
gts = {{ path = '{gts}' }}
gts-macros = {{ path = '{macros}' }}
schemars = {{ version = "1.2", features = ["uuid1"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
jsonschema = {{ version = "0.58.6", default-features = false }}

[workspace]
"#,
            lib = crate_dir.join("tests/overrides_harness/lib.rs").display(),
            gts = root.join("gts").display(),
            macros = crate_dir.display(),
        );
        std::fs::write(project.join("Cargo.toml"), manifest).unwrap();
        std::fs::copy(root.join("Cargo.lock"), project.join("Cargo.lock")).unwrap();
        project.join("Cargo.toml")
    })
}

/// `cargo test --lib <cargo_args> -- <test_args>` on the fixture.
fn cargo_test(cargo_args: &[&str], test_args: &[&str]) -> (Output, String) {
    let manifest = manifest();
    let output = Command::new(env!("CARGO"))
        .args(["test", "--lib", "--manifest-path"])
        .arg(manifest)
        .arg("--target-dir")
        .arg(manifest.parent().unwrap().join("target"))
        .args(cargo_args)
        .arg("--")
        .args(test_args)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output, text)
}

#[test]
fn generated_tests_run_under_cargo_test() {
    // Listing builds the crate: an invalid override is not a build error.
    let (output, text) = cargo_test(&[], &["--list"]);
    assert!(output.status.success(), "{text}");
    for name in [
        GENERIC,
        VALID_UNIT,
        INVALID_UNIT,
        INVALID_PARENT,
        ADMITTED,
        NOT_ADMITTED,
    ] {
        assert!(text.contains(&format!("{name}: test")), "{text}");
    }
    // Types without `overrides(...)` do not get a generated test.
    for name in ["PlainV1", "SubjectV1"] {
        let test_name = format!("gts_overrides_{name}: test");
        assert!(!text.lines().any(|line| line == test_name), "{text}");
    }

    let (output, text) = cargo_test(&[], &["--exact", GENERIC, VALID_UNIT, ADMITTED]);
    assert!(output.status.success(), "{text}");
    for name in [GENERIC, VALID_UNIT, ADMITTED] {
        assert!(text.contains(&format!("test {name} ... ok")), "{text}");
    }
    assert!(text.contains("3 passed; 0 failed"), "{text}");

    let (output, text) = cargo_test(
        &[],
        &[
            "--skip",
            "chain::",
            "--skip",
            "nullable::",
            "--skip",
            "entity_refs::",
        ],
    );
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains(&format!("test {INVALID_UNIT} ... FAILED")),
        "{text}"
    );
    assert!(
        text.contains("invalid overrides on 'gts.x.test.ovr_harness.subject.v1~x.test.ovr_harness.invalid_unit.v1~'"),
        "{text}"
    );
    assert!(text.contains("field '/id'"), "{text}");
    assert!(
        text.contains("test invalid::schema_generation_and_validation_are_separate ... ok"),
        "{text}"
    );
    assert!(
        text.contains(&format!("test {INVALID_PARENT} ... FAILED")),
        "{text}"
    );
    // Strict replacement admits it; the registry's derivation admission does not.
    assert!(
        text.contains(&format!("test {NOT_ADMITTED} ... FAILED")),
        "{text}"
    );
    assert!(
        text.contains("the registry's derivation admission rejects the chain"),
        "{text}"
    );
    assert!(text.contains("property 'pair.b'"), "{text}");
    assert!(
        text.contains("test admission::descendants_validate_their_whole_chain ... ok"),
        "{text}"
    );
    assert!(text.contains("6 passed; 3 failed"), "{text}");
}

#[test]
fn generated_test_in_a_fn_body_fails_the_test_build() {
    let (output, text) = cargo_test(&["--features", "local", "--no-run"], &[]);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("cannot find type `LocalV1` in module `self`"),
        "{text}"
    );
    assert!(text.contains("due to 1 previous error"), "{text}");

    // A same-named module-level type does not satisfy the guard.
    let (output, text) = cargo_test(&["--features", "shadow", "--no-run"], &[]);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("mismatched types"), "{text}");
    assert!(text.contains("due to 1 previous error"), "{text}");
}

/// The negative fixtures intentionally fail their generated tests. Keep them
/// outside the workspace test targets and assert the exact failure set here;
/// ordinary behaviour tests in the same crate must still pass.
#[test]
fn override_fixtures_reject_every_invalid_declaration() {
    let expected: BTreeSet<_> = [
        "chain::gts_overrides_AnyPatternV1",
        "chain::gts_overrides_BadEnumV1",
        "chain::gts_overrides_DisabledIdV1",
        "chain::gts_overrides_DroppedBoundsV1",
        "chain::gts_overrides_EmptyRangeV1",
        "chain::gts_overrides_EnumHelperV1",
        "chain::gts_overrides_FlattenV1",
        "chain::gts_overrides_LevelDefaultV1",
        "chain::gts_overrides_NestedUnknownV1",
        "chain::gts_overrides_NickUnboundedV1",
        "chain::gts_overrides_NoFieldsV1",
        "chain::gts_overrides_NullableIdV1",
        "chain::gts_overrides_NumericIdV1",
        "chain::gts_overrides_OtherFormatV1",
        "chain::gts_overrides_PartialAddressV1",
        "chain::gts_overrides_RecursiveV1",
        "chain::gts_overrides_SlotV1",
        "chain::gts_overrides_UnknownFieldV1",
        "chain::gts_overrides_UsUnderEuV1",
        "chain::gts_overrides_WideAgainV1",
        "chain::gts_overrides_WideIdV1",
        "chain::gts_overrides_WideLevelV1",
        "chain::multiple_of::gts_overrides_BackToTwoV1",
        "entity_refs::gts_overrides_BackToBaseV1",
        "entity_refs::gts_overrides_UnconstrainedV1",
        "entity_refs::gts_overrides_WideV1",
        "nullable::gts_overrides_WideCityV1",
    ]
    .into_iter()
    .collect();
    let mut failed = BTreeSet::new();
    for module in ["chain::", "nullable::", "entity_refs::"] {
        let (output, text) = cargo_test(&[], &[module]);
        assert_eq!(output.status.code(), Some(101), "{text}");
        assert!(text.contains("test result: FAILED."), "{text}");
        for line in text.lines() {
            if let Some(name) = line
                .strip_prefix("test ")
                .and_then(|line| line.strip_suffix(" ... FAILED"))
            {
                failed.insert(name.to_owned());
            }
        }
    }
    let expected: BTreeSet<_> = expected.into_iter().map(str::to_owned).collect();
    assert_eq!(failed, expected);
}
