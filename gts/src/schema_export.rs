//! Schema export for `gts generate-from-rust`.
//!
//! `#[struct_to_gts_schema]` emits a `#[cfg(test)]` test per exportable type
//! that calls [`export_if_requested`]. The CLI runs those tests with
//! [`EXPORT_DIR_ENV`], [`EXPORT_FILES_ENV`] and [`EXPORT_ROOT_ENV`] set;
//! otherwise they do nothing.

use std::path::{Path, PathBuf};

use crate::schema::GtsSchema;

/// Directory the CLI collects export records from.
pub const EXPORT_DIR_ENV: &str = "GTS_SCHEMA_EXPORT_DIR";
/// Newline-separated canonical paths of the source files to export.
pub const EXPORT_FILES_ENV: &str = "GTS_SCHEMA_EXPORT_FILES";
/// Cargo workspace root, which relative `file!()` paths start from.
pub const EXPORT_ROOT_ENV: &str = "GTS_SCHEMA_EXPORT_WORKSPACE_ROOT";
/// Version of the record layout.
pub const EXPORT_FORMAT: u64 = 1;

/// Where an annotated type is declared, as seen by the macro.
#[derive(Debug, Clone, Copy)]
pub struct ExportSource<'a> {
    pub struct_name: &'a str,
    /// `module_path!()` at the declaration.
    pub module_path: &'a str,
    /// `file!()` at the declaration.
    pub file: &'a str,
    /// `env!("CARGO_MANIFEST_DIR")` of the declaring crate.
    pub manifest_dir: &'a str,
    /// The macro's `dir_path` argument.
    pub dir_path: &'a str,
}

/// Dispatches a macro-emitted export test to its type; the test lives in a
/// module and cannot name a function-local type itself.
#[doc(hidden)]
pub trait Exporter {
    /// Runs [`export_if_requested`] for the annotated type.
    ///
    /// # Errors
    ///
    /// See [`export_if_requested`].
    fn export() -> Result<(), String>;
}

/// Writes `T`'s schema record when the CLI requested this source file.
/// Without [`EXPORT_DIR_ENV`] it returns before touching anything.
///
/// # Errors
///
/// Invalid overrides on `T`, or a failure to write the record.
pub fn export_if_requested<T: GtsSchema>(source: ExportSource<'_>) -> Result<(), String> {
    let Some(dir) = std::env::var_os(EXPORT_DIR_ENV) else {
        return Ok(());
    };
    let files = std::env::var(EXPORT_FILES_ENV).unwrap_or_default();
    let root = std::env::var_os(EXPORT_ROOT_ENV).map(PathBuf::from);
    export_to::<T>(Path::new(&dir), &files, root.as_deref(), source)
}

fn export_to<T: GtsSchema>(
    dir: &Path,
    files: &str,
    workspace_root: Option<&Path>,
    source: ExportSource<'_>,
) -> Result<(), String> {
    let Some(file) = resolve_source(source.file, workspace_root, Path::new(source.manifest_dir))
    else {
        return Ok(());
    };
    if !files.lines().any(|selected| Path::new(selected) == file) {
        return Ok(());
    }
    T::validate_overrides().map_err(|e| format!("invalid overrides on '{}': {e}", T::TYPE_ID))?;
    let record = serde_json::json!({
        "format": EXPORT_FORMAT,
        "struct": source.struct_name,
        "module": source.module_path,
        "file": file,
        "dir_path": source.dir_path,
        "type_id": T::TYPE_ID,
        "schema": T::gts_schema_with_refs(),
    });
    // A file compiled into several test binaries yields one record per process.
    let name = format!(
        "{}-{}-{}.json",
        std::process::id(),
        source.module_path.replace("::", "."),
        source.struct_name
    );
    let text = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&name), text).map_err(|e| {
        format!(
            "cannot write schema export {}: {e}",
            dir.join(name).display()
        )
    })
}

/// Canonical path of `file!()`: absolute, or relative to the directory rustc ran
/// in, which is the workspace root (or, outside a workspace, the manifest dir).
fn resolve_source(
    file: &str,
    workspace_root: Option<&Path>,
    manifest_dir: &Path,
) -> Option<PathBuf> {
    let file = Path::new(file);
    let candidates = if file.is_absolute() {
        vec![file.to_path_buf()]
    } else {
        workspace_root
            .into_iter()
            .chain([manifest_dir])
            .map(|base| base.join(file))
            .collect()
    };
    candidates
        .into_iter()
        .find_map(|path| path.canonicalize().ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    struct Plain;
    impl GtsSchema for Plain {
        const TYPE_ID: &'static str = "gts.x.test.export.plain.v1~";
        fn gts_schema_with_refs() -> serde_json::Value {
            serde_json::json!({ "type": "object" })
        }
    }

    /// A workspace with `pkg/src/events.rs` and a same-suffix decoy file.
    fn workspace() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let events = root.path().join("pkg/src/events.rs");
        let decoy = root.path().join("pkg/fixtures/pkg/src/events.rs");
        for file in [&events, &decoy] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "").unwrap();
        }
        (
            root,
            events.canonicalize().unwrap(),
            decoy.canonicalize().unwrap(),
        )
    }

    fn source<'a>(file: &'a str, manifest_dir: &'a str) -> ExportSource<'a> {
        ExportSource {
            struct_name: "Plain",
            module_path: "pkg::events",
            file,
            manifest_dir,
            dir_path: "schemas",
        }
    }

    fn records(dir: &Path) -> Vec<serde_json::Value> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| serde_json::from_slice(&std::fs::read(e.unwrap().path()).unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn unselected_file_writes_nothing() {
        let (root, events, decoy) = workspace();
        let manifest_dir = root.path().join("pkg");
        let out = tempfile::tempdir().unwrap();
        let src = source("pkg/src/events.rs", manifest_dir.to_str().unwrap());
        // The decoy ends with the same relative path but is a different file.
        export_to::<Plain>(out.path(), decoy.to_str().unwrap(), Some(root.path()), src).unwrap();
        export_to::<Plain>(out.path(), "", Some(root.path()), src).unwrap();
        assert!(records(out.path()).is_empty());
        assert_ne!(events, decoy);
    }

    #[test]
    fn selected_file_writes_canonical_record() {
        let (root, events, _) = workspace();
        let manifest_dir = root.path().join("pkg");
        let out = tempfile::tempdir().unwrap();
        let selected = format!("/elsewhere/a.rs\n{}", events.display());
        let src = source("pkg/src/events.rs", manifest_dir.to_str().unwrap());
        export_to::<Plain>(out.path(), &selected, Some(root.path()), src).unwrap();
        let records = records(out.path());
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["format"], EXPORT_FORMAT);
        assert_eq!(records[0]["struct"], "Plain");
        assert_eq!(records[0]["module"], "pkg::events");
        assert_eq!(records[0]["file"], events.to_str().unwrap());
        assert_eq!(records[0]["dir_path"], "schemas");
        assert_eq!(records[0]["type_id"], Plain::TYPE_ID);
        assert_eq!(
            records[0]["schema"],
            serde_json::json!({ "type": "object" })
        );
    }

    #[test]
    fn relative_file_falls_back_to_manifest_dir() {
        let (root, events, _) = workspace();
        let manifest_dir = root.path().join("pkg");
        assert_eq!(
            resolve_source("src/events.rs", None, &manifest_dir),
            Some(events.clone())
        );
        assert_eq!(
            resolve_source(events.to_str().unwrap(), None, Path::new("/nowhere")),
            Some(events)
        );
        assert_eq!(
            resolve_source("src/missing.rs", Some(root.path()), &manifest_dir),
            None
        );
    }
}
