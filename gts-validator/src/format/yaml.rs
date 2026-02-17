//! YAML file scanner for GTS identifiers.
//!
//! Uses tree-walking to scan string values (not keys by default).

use std::path::Path;

use serde_json::Value;

use crate::error::ValidationError;
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
pub fn scan_yaml_content(
    content: &str,
    path: &Path,
    vendor: Option<&str>,
    scan_keys: bool,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Parse all documents with the YAML stream parser first.
    // If this fails (e.g., one malformed document in the stream), fall back to per-document
    // parsing so valid sibling documents are still validated.
    let documents: Vec<Value> = match serde_saphyr::from_multiple(content) {
        Ok(docs) => docs,
        Err(_e) => {
            for segment in split_yaml_documents(content) {
                let value: Value = match serde_saphyr::from_str(&segment) {
                    Ok(doc) => doc,
                    Err(_segment_err) => continue,
                };

                walk_json_value(&value, path, vendor, &mut errors, "$", scan_keys);
            }

            return errors;
        }
    };

    for value in documents {
        // Reuse the JSON walker since both operate on serde_json::Value
        walk_json_value(&value, path, vendor, &mut errors, "$", scan_keys);
    }

    errors
}

/// Scan a YAML file for GTS identifiers (file-based convenience wrapper).
#[cfg(test)]
pub fn scan_yaml_file(
    path: &Path,
    vendor: Option<&str>,
    max_file_size: u64,
    scan_keys: bool,
) -> Vec<ValidationError> {
    // Check file size
    if let Ok(metadata) = std::fs::metadata(path)
        && metadata.len() > max_file_size
    {
        return vec![];
    }

    // Read as UTF-8; skip file on encoding error
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_e) => return vec![],
    };

    scan_yaml_content(&content, path, vendor, scan_keys)
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
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false);
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }

    #[test]
    fn test_scan_yaml_invalid_id() {
        let content = r"
$id: gts.invalid
";
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_scan_yaml_xgts_ref_wildcard() {
        let content = r"
x-gts-ref: gts.x.core.*
";
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false);
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
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false);
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
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false);
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
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false);
        assert!(
            errors.is_empty(),
            "Array values should be found and validated"
        );
    }

    #[test]
    fn test_scan_yaml_invalid_yaml() {
        let content = r"
invalid: yaml: syntax:
";
        let file = create_temp_yaml(content);
        let errors = scan_yaml_file(file.path(), None, 10_485_760, false);
        assert!(
            errors.is_empty(),
            "Invalid YAML should be skipped with warning"
        );
    }

    #[test]
    fn test_scan_yaml_multi_document_all_validated() {
        // All documents in a multi-document stream must be validated.
        let content = "\
$id: gts.x.core.events.type.v1~
---
$id: gts.invalid
";
        let errors = scan_yaml_content(content, Path::new("multi.yaml"), None, false);
        // Both documents are parsed — gts.invalid in doc 2 must produce an error
        assert!(
            !errors.is_empty(),
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
        // If the malformed middle doc caused an early return, errors would be empty.
        let errors = scan_yaml_content(content, Path::new("multi.yaml"), Some("x"), false);
        assert!(
            !errors.is_empty(),
            "Valid documents must be validated even when a sibling document is malformed, got no errors"
        );
    }
}
