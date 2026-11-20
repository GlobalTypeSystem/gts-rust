#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Generated code must not name `gts_macros`, which a crate may rename or reach through a facade.

use std::path::Path;
use std::process::Command;

#[test]
fn configured_derive_resolves_renamed_dependencies_and_late_serializers() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.parent().unwrap();
    let project = Path::new(env!("CARGO_TARGET_TMPDIR")).join("renamed_dependency");
    std::fs::create_dir_all(project.join("src/bin")).unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        format!(
            "[package]\nname = \"gts-renamed-dependency\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\nproc-macro = true\n\n\
             [dependencies]\ngts = {{ path = '{}' }}\n\
             derive_for_gts = {{ package = \"gts-macros\", path = '{}' }}\n\
             quote = \"1\"\nsyn = {{ version = \"2\", features = [\"full\"] }}\nschemars = \"1.2\"\nserde = {{ version = \"1.0\", features = [\"derive\"] }}\nserde_json = \"1.0\"\n\n\
             [workspace]\n",
            root.join("gts").display(),
            crate_dir.display(),
        ),
    )
    .unwrap();
    std::fs::write(
        project.join("src/lib.rs"),
        r#"use proc_macro::TokenStream;
#[proc_macro_attribute]
pub fn as_text(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut item = syn::parse_macro_input!(item as syn::ItemStruct);
    let field = item.fields.iter_mut().find(|f| f.ident.as_ref().unwrap() == "tags").unwrap();
    field.attrs.push(syn::parse_quote!(#[serde(serialize_with = "as_text")]));
    quote::quote!(#item).into()
}"#,
    )
    .unwrap();
    std::fs::write(
        project.join("src/bin/membership.rs"),
        r#"#[derive_for_gts::struct_to_gts_schema(
    dir_path = "schemas",
    type_id = "gts.example.core.groups.membership.v1~",
    description = "Resource types accepted for group membership",
    properties = "resource_type",
    base = true,
    gts_abstract = true
)]
pub struct MembershipV1 {
    pub resource_type: gts::GtsTypeId,
    // A conditional field routes the identity check through the generated derive.
    #[cfg(any())]
    pub legacy: u32,
}
fn main() {}
"#,
    )
    .unwrap();
    std::fs::copy(root.join("Cargo.lock"), project.join("Cargo.lock")).unwrap();

    let output = Command::new(env!("CARGO"))
        .args([
            "check",
            "--offline",
            "--bin",
            "membership",
            "--manifest-path",
        ])
        .arg(project.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(project.join("target"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The attribute below runs after the GTS attribute and changes the wire type.
    std::fs::write(
        project.join("src/bin/shadow.rs"),
        r#"
#[derive_for_gts::struct_to_gts_schema(dir_path = "schemas", base = true,
    type_id = "gts.example.core.groups.tags.v1~", description = "Tags", properties = "id,tags")]
#[gts_renamed_dependency::as_text]
struct TagsV1 {
    id: gts::GtsInstanceId,
    #[serde(rename = "gtsId")]
    tags: Vec<String>,
}
fn as_text<S: serde::Serializer>(tags: &[String], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&tags.join(","))
}
fn main() {}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO"))
        .args(["check", "--offline", "--bin", "shadow", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(project.join("target"))
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() && error.contains("can shadow GTS instance ID"),
        "{error}"
    );
}
