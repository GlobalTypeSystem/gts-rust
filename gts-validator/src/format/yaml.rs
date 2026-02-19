//! YAML file scanner for GTS identifiers.
//!
//! Uses tree-walking to scan string values (not keys by default).

use std::path::Path;

use serde_json::Value;

use crate::error::{ScanError, ScanErrorKind, ValidationError};
use crate::format::json::walk_json_value;

fn split_yaml_documents(content: &str) -> Vec<String> {
    let mut documents = Vec::new();
    let mut current_doc: Vec<&str> = Vec::new();

    for line in content.lines() {
        if line.trim() == "---" {
            let doc = current_doc.join("\n");
            if !doc.trim().is_empty() {
                documents.push(doc);
            }
            current_doc.clear();
            continue;
        }
        current_doc.push(line);
    }

    let doc = current_doc.join("\n");
    if !doc.trim().is_empty() {
        documents.push(doc);
    }

    documents
}

/// Scan YAML content for GTS identifiers.
///
/// Returns `(validation_errors, scan_errors)`:
/// - `validation_errors`: GTS ID validation failures found in successfully-parsed documents.
/// - `scan_errors`: per-document parse failures in multi-document streams, or a single
///   file-level parse failure if no document could be parsed at all.
///
/// This separation ensures malformed YAML documents are counted in `failed_files`
/// and never silently mixed into the validation error layer.
pub fn scan_yaml_content(
    content: &str,
    path: &Path,
    vendor: Option<&str>,
    scan_keys: bool,
) -> (Vec<ValidationError>, Vec<ScanError>) {
    let mut validation_errors = Vec::new();
    let mut scan_errors = Vec::new();

    // Parse all documents with the YAML stream parser first.
    // If this fails (e.g., one malformed document in the stream), fall back to per-document
    // parsing so valid sibling documents are still validated.
    let documents: Vec<Value> = match serde_saphyr::from_multiple(content) {
        Ok(docs) => docs,
        Err(stream_err) => {
            let segments = split_yaml_documents(content);
            let mut any_parsed = false;

            for (idx, segment) in segments.iter().enumerate() {
                match serde_saphyr::from_str::<Value>(segment) {
                    Ok(doc) => {
                        any_parsed = true;
                        walk_json_value(&doc, path, vendor, &mut validation_errors, "$", scan_keys);
                    }
                    Err(doc_err) => {
                        // Per-document parse failure → ScanError (not ValidationError)
                        scan_errors.push(ScanError {
                            file: path.to_owned(),
                            kind: ScanErrorKind::YamlParseError,
                            message: format!(
                                "YAML parse error in document {} of multi-document stream: {doc_err}",
                                idx + 1
                            ),
                        });
                    }
                }
            }

            if !any_parsed {
                // No document parsed at all — replace per-doc errors with a single file-level error
                scan_errors.clear();
                scan_errors.push(ScanError {
                    file: path.to_owned(),
                    kind: ScanErrorKind::YamlParseError,
                    message: format!("YAML parse error: {stream_err}"),
                });
            }

            return (validation_errors, scan_errors);
        }
    };

    for value in documents {
        walk_json_value(&value, path, vendor, &mut validation_errors, "$", scan_keys);
    }

    (validation_errors, scan_errors)
}

/// Scan a YAML file for GTS identifiers (file-based convenience wrapper for tests).
///
/// Returns `Err` if the file cannot be read or if any scan-level error occurred.
/// Returns `Ok(validation_errors)` if the file was parsed (even partially for multi-doc streams).
#[cfg(test)]
pub fn scan_yaml_file(
    path: &Path,
    vendor: Option<&str>,
    max_file_size: u64,
    scan_keys: bool,
) -> Result<Vec<ValidationError>, ScanError> {
    use crate::strategy::fs::{ScanResult, read_file_bounded};

    let content = match read_file_bounded(path, max_file_size) {
        ScanResult::Ok(c) => c,
        ScanResult::Err(e) => return Err(e),
    };

    let (val_errs, scan_errs) = scan_yaml_content(&content, path, vendor, scan_keys);
    if let Some(first_scan_err) = scan_errs.into_iter().next() {
        return Err(first_scan_err);
    }
    Ok(val_errs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_yaml(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_scan_yaml_valid_id() {
        let content = r"
$id: gts://gts.x.core.events.type.v1~
";
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false).unwrap();
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }

    #[test]
    fn test_scan_yaml_invalid_id() {
        let content = r"
$id: gts.invalid
";
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false).unwrap();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_scan_yaml_xgts_ref_wildcard() {
        let content = r"
x-gts-ref: gts.x.core.*
";
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false).unwrap();
        assert!(
            errors.is_empty(),
            "Wildcards in x-gts-ref should be allowed"
        );
    }

    #[test]
    fn test_scan_yaml_xgts_ref_bare_wildcard() {
        let content = r#"
x-gts-ref: "*"
"#;
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false).unwrap();
        assert!(
            errors.is_empty(),
            "Bare wildcard in x-gts-ref should be skipped"
        );
    }

    #[test]
    fn test_scan_yaml_nested_values() {
        let content = r"
properties:
  type:
    x-gts-ref: gts.x.core.events.type.v1~
";
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false).unwrap();
        assert!(
            errors.is_empty(),
            "Nested values should be found and validated"
        );
    }

    #[test]
    fn test_scan_yaml_array_values() {
        let content = r"
capabilities:
  - gts.x.core.events.type.v1~
  - gts.x.core.events.topic.v1~
";
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false).unwrap();
        assert!(
            errors.is_empty(),
            "Array values should be found and validated"
        );
    }

    #[test]
    fn test_scan_yaml_invalid_yaml_is_scan_error() {
        // Completely invalid YAML (not parseable as any document) must be a ScanError
        let content = ": : :\n  - [unclosed\n";
        let file = create_temp_yaml(content);
        let result = scan_yaml_file(file.path(), None, 10_485_760, false);
        assert!(
            result.is_err(),
            "Completely invalid YAML must produce a ScanError, not silent success"
        );
        let err = result.unwrap_err();
        assert_eq!(err.kind, crate::error::ScanErrorKind::YamlParseError);
    }

    #[test]
    fn test_scan_yaml_multi_document_all_validated() {
        // All documents in a multi-document stream must be validated.
        let content = "\
$id: gts.x.core.events.type.v1~
---
$id: gts.invalid
";
        let (val_errs, scan_errs) =
            scan_yaml_content(content, Path::new("multi.yaml"), None, false);
        assert!(
            scan_errs.is_empty(),
            "No scan errors expected for well-formed stream: {scan_errs:?}"
        );
        // Both documents are parsed — gts.invalid in doc 2 must produce an error
        assert!(
            !val_errs.is_empty(),
            "Multi-document YAML: second document with invalid ID should be caught, got no errors"
        );
    }

    #[test]
    fn test_scan_yaml_multi_document_malformed_doc_does_not_suppress_valid_doc() {
        // A malformed document must be skipped, but valid documents around it must still be validated.
        let content = "\
$id: gts.y.core.pkg.mytype.v1~
---
invalid: yaml: syntax:
---
$id: gts.y.core.pkg.mytype.v1~
";
        // With vendor "x", both valid docs should produce vendor-mismatch errors.
        // The malformed middle doc must produce a ScanError, not suppress valid docs.
        let (val_errs, scan_errs) =
            scan_yaml_content(content, Path::new("multi.yaml"), Some("x"), false);
        assert!(
            !val_errs.is_empty(),
            "Valid documents must be validated even when a sibling document is malformed, got no errors"
        );
        assert!(
            !scan_errs.is_empty(),
            "Malformed document must produce a ScanError, got none"
        );
        assert_eq!(
            scan_errs[0].kind,
            crate::error::ScanErrorKind::YamlParseError,
            "Malformed doc scan error must have YamlParseError kind"
        );
    }
}
