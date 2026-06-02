//! Generic golden-file harness for macro-generated JSON Schemas.
//!
//! Each case lives in `tests/golden/<area>_<case>.rs` (structs + a `schemas()`
//! function returning `(type_id, generated_schema)` pairs) with its expected
//! JSON Schemas in the sibling directory
//! `tests/golden/<area>_<case>/<type_id>.schema.json`. Generated schemas are
//! compared **semantically** (parsed `serde_json::Value`) against the goldens.
//!
//! Bless (regenerate) after an intended change:
//!
//! ```bash
//! GTS_GOLDEN=overwrite cargo test -p gts-macros --test golden_tests
//! ```
//!
//! Add a case: drop `tests/golden/<area>_<case>.rs` (with `pub fn schemas()`)
//! and add its name to the `golden_cases!` list below. The area prefix
//! (`traits_`, `inheritance_`, …) groups cases by subject.
//!
//! NOTE: case modules are still listed explicitly because an integration-test
//! crate root cannot glob modules. Once golden cases proliferate, the clean move
//! is a dedicated `gts-macros-tests` crate (`publish = false`) with a `build.rs`
//! that auto-discovers `golden/*.rs` — and *all* macro tests (golden +
//! behavioural + compile-fail) relocate there together, rather than splitting
//! them across crates. Until then, keep everything here and list cases below.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::Path;

/// Resolves `gts://<type_id>` `$ref`s against the schemas generated within the
/// same golden case, so a derived document's `allOf[0].$ref` to its base (and
/// any `x-gts-traits-schema` `$ref` to a sibling trait type) compiles. Mirrors
/// the production `GtsRetriever` (`gts::store`) but scoped to one case.
#[derive(Clone)]
struct CaseRetriever {
    by_uri: HashMap<String, serde_json::Value>,
}

impl jsonschema::Retrieve for CaseRetriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        self.by_uri
            .get(uri.as_str())
            .cloned()
            .ok_or_else(|| format!("schema not found for $ref: {}", uri.as_str()).into())
    }
}

/// Assert every generated document in a case compiles as a JSON Schema.
///
/// Compiled with a retriever populated from the case's own schemas, so chain
/// `$ref`s resolve. Inline `x-gts-traits-schema` object subschemas are compiled
/// separately — a custom keyword the validator ignores, yet which must itself be
/// valid JSON Schema. The *structural* check; [`assert_registry_valid`] adds the
/// *semantic* GTS checks (OP#12 chain + OP#13 traits).
fn assert_schemas_valid(case: &str, schemas: &[(String, serde_json::Value)]) {
    let by_uri: HashMap<String, serde_json::Value> = schemas
        .iter()
        .map(|(id, schema)| (format!("gts://{id}"), schema.clone()))
        .collect();
    let retriever = CaseRetriever { by_uri };

    for (type_id, schema) in schemas {
        jsonschema::options()
            .with_retriever(retriever.clone())
            .build(schema)
            .unwrap_or_else(|e| {
                panic!(
                    "case '{case}': generated schema '{type_id}' is not a valid JSON Schema: {e}"
                )
            });

        if let Some(ts) = schema.get("x-gts-traits-schema")
            && ts.is_object()
        {
            jsonschema::options()
                .with_retriever(retriever.clone())
                .build(ts)
                .unwrap_or_else(|e| {
                    panic!(
                        "case '{case}': inline x-gts-traits-schema of '{type_id}' is not a valid \
                         JSON Schema: {e}"
                    )
                });
        }
    }
}

/// Assert the whole case registers and validates in a real GTS registry.
///
/// Registers every generated schema and runs OP#12 (chain) + OP#13 (traits) on
/// each — the semantic checks compilation can't perform: trait completeness on
/// non-abstract types, `const`/enum/`additionalProperties` enforcement against
/// the merged effective trait-schema, and resolution of `$ref`-ed trait types.
/// Every golden case must therefore be a registry-valid set.
fn assert_registry_valid(case: &str, schemas: &[(String, serde_json::Value)]) {
    let refs: Vec<&serde_json::Value> = schemas.iter().map(|(_, s)| s).collect();
    gts::testing::validate_all(&refs)
        .unwrap_or_else(|e| panic!("case '{case}': schemas are not valid in a GTS registry: {e}"));
}

/// Declare golden cases with one identifier each.
///
/// For every `<name>`, includes `tests/golden/<name>.rs` as a module and emits
/// a `#[test]` comparing its `schemas()` against `tests/golden/<name>/`. This
/// collapses the per-case `#[path] mod … + #[test] fn …` boilerplate to a
/// single entry. (`include!(concat!(…))` is used rather than `#[path]` because
/// `#[path]` requires a string literal, while the module name is built from the
/// identifier.)
macro_rules! golden_cases {
    ($($name:ident),+ $(,)?) => {
        $(
            mod $name {
                include!(concat!("golden/", stringify!($name), ".rs"));
            }

            #[test]
            fn $name() {
                let schemas = $name::schemas();
                assert_schemas_valid(stringify!($name), &schemas);
                assert_registry_valid(stringify!($name), &schemas);
                check_golden(stringify!($name), schemas);
            }
        )+
    };
}

golden_cases!(
    traits_inline_chain,
    traits_bool_true,
    traits_bool_false,
    traits_generic_child,
    traits_schema_narrowing,
    traits_referenced_chain,
    traits_struct_literal,
);

/// Compare each generated `(type_id, schema)` against
/// `tests/golden/<case>/<type_id>.schema.json`, or rewrite the goldens when
/// `GTS_GOLDEN=overwrite` is set.
fn check_golden(case: &str, schemas: Vec<(String, serde_json::Value)>) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(case);
    let overwrite = std::env::var("GTS_GOLDEN").is_ok_and(|v| v == "overwrite");

    for (type_id, schema) in schemas {
        let path = dir.join(format!("{type_id}.schema.json"));

        if overwrite {
            std::fs::create_dir_all(&dir).expect("create golden dir");
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize schema") + "\n";
            std::fs::write(&path, pretty).expect("write golden");
            continue;
        }

        assert!(
            path.exists(),
            "missing golden file {} for case '{case}' — run \
             `GTS_GOLDEN=overwrite cargo test -p gts-macros --test golden_tests` to create it",
            path.display(),
        );
        let expected_str = std::fs::read_to_string(&path).expect("read golden");
        let expected: serde_json::Value =
            serde_json::from_str(&expected_str).expect("parse golden json");

        assert_eq!(
            schema,
            expected,
            "golden mismatch for '{type_id}' in case '{case}' \
             (run GTS_GOLDEN=overwrite to update)\n--- actual ---\n{}",
            serde_json::to_string_pretty(&schema).unwrap(),
        );
    }
}
