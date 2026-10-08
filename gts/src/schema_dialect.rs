//! The JSON Schema dialects a GTS Type Schema may declare (README §11.0).
//!
//! GTS admits Draft-07, Draft 2019-09 and Draft 2020-12, nothing older and no
//! custom meta-schema. JSON Schema lets a subschema switch dialect with its
//! own `$schema`, and resources that declare different dialects reference
//! each other; GTS allows neither, so no part of a type is read under a
//! vocabulary other than the one its hierarchy selects.

use jsonschema::Draft;
use serde_json::Value;

use crate::gts::GTS_ID_URI_PREFIX;
use crate::schema_modifiers::SchemaScope;
use crate::schema_resolver::SchemaProvider;

/// The dialect a GTS Type Schema declares in its top-level `$schema`.
///
/// A GTS Type Schema always declares one (README §2.4). The `http` and `https`
/// spellings and an empty trailing fragment are equivalent; anything else
/// outside the supported set is refused rather than read as some fallback
/// dialect.
///
/// # Errors
/// Why the document declares no supported dialect.
pub fn document_dialect(schema: &Value) -> Result<Draft, String> {
    let Some(declared) = schema.get("$schema") else {
        return Err("a GTS Type Schema must declare a top-level '$schema'".to_owned());
    };
    let Some(uri) = declared.as_str() else {
        return Err(format!("'$schema' must be a string, got {declared}"));
    };
    let unfragmented = uri.strip_suffix('#').unwrap_or(uri);
    let location = unfragmented
        .strip_prefix("https://")
        .or_else(|| unfragmented.strip_prefix("http://"));
    match location {
        Some("json-schema.org/draft-07/schema") => Ok(Draft::Draft7),
        Some("json-schema.org/draft/2019-09/schema") => Ok(Draft::Draft201909),
        Some("json-schema.org/draft/2020-12/schema") => Ok(Draft::Draft202012),
        Some(location) if is_pre_draft_07(location) => Err(format!(
            "'$schema' declares '{uri}', but Draft-07 is the minimum supported dialect"
        )),
        _ => Err(format!(
            "'$schema' declares '{uri}', which is not a supported dialect \
             (Draft-07, Draft 2019-09 or Draft 2020-12)"
        )),
    }
}

/// Whether `location` is the meta-schema of Draft 6 or an earlier draft.
fn is_pre_draft_07(location: &str) -> bool {
    location
        .strip_prefix("json-schema.org/draft-0")
        .and_then(|rest| rest.strip_suffix("/schema"))
        .is_some_and(|number| matches!(number, "0" | "1" | "2" | "3" | "4" | "5" | "6"))
}

/// Human-readable dialect name for diagnostics.
pub fn dialect_name(draft: Draft) -> &'static str {
    match draft {
        Draft::Draft4 => "Draft 4",
        Draft::Draft6 => "Draft 6",
        Draft::Draft7 => "Draft-07",
        Draft::Draft201909 => "Draft 2019-09",
        Draft::Draft202012 => "Draft 2020-12",
        _ => "an unrecognized dialect",
    }
}

/// Rejects a `$ref` whose target is read under a different dialect than the
/// reference itself.
///
/// Covers same-document JSON Pointers (including ones into an embedded
/// resource that declares its own `$schema`) and `gts://` targets with or
/// without a fragment. A same-document pointer resolves from the root of the
/// schema resource it sits in, so `#` inside an embedded resource with its own
/// `$id` names that resource. A target that does not resolve is skipped:
/// reference resolution reports it.
///
/// # Errors
/// The first cross-dialect `$ref`, by location.
pub fn check_references(schema: &Value, provider: &dyn SchemaProvider) -> Result<(), String> {
    check_references_in_dialect(schema, Draft::default(), provider)
}

/// Checks local and self references in a standalone document or inherited fragment.
///
/// # Errors
/// The first reference whose target declares another dialect.
pub fn check_local_references(schema: &Value, dialect: Draft) -> Result<(), String> {
    struct LocalOnly<'a>(&'a Value);
    impl SchemaProvider for LocalOnly<'_> {
        fn schema_content(&self, type_id: &str) -> Option<&Value> {
            let own_id = self
                .0
                .get("$id")?
                .as_str()?
                .strip_prefix(GTS_ID_URI_PREFIX)?;
            (own_id == type_id).then_some(self.0)
        }
    }
    // Standalone comparisons have no external provider. External references
    // remain the comparison's responsibility; placeholders let us inspect the
    // complete local graph without fetching unrelated documents.
    struct ExternalUnknown;
    impl jsonschema::Retrieve for ExternalUnknown {
        fn retrieve(
            &self,
            _uri: &jsonschema::Uri<String>,
        ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
            Ok(Value::Bool(true))
        }
    }
    check_references_in_dialect(schema, dialect, &LocalOnly(schema))?;
    let dialect = dialect.detect(schema);
    if dialect == Draft::Unknown {
        return Ok(());
    }
    let root = schema
        .get("$id")
        .and_then(Value::as_str)
        .or_else(|| {
            schema
                .get("id")
                .and_then(Value::as_str)
                .filter(|_| dialect == Draft::Draft4)
        })
        .unwrap_or(crate::schema_regex::DEFAULT_BASE_URI);
    let registry = jsonschema::Registry::new()
        .draft(dialect)
        .retriever(ExternalUnknown)
        .add(root, schema)
        .and_then(jsonschema::RegistryBuilder::prepare)
        .map_err(|error| error.to_string())?;
    crate::schema_regex::check_dialect(&registry, &[(root, schema)], dialect)
}

fn check_references_in_dialect(
    schema: &Value,
    dialect: Draft,
    provider: &dyn SchemaProvider,
) -> Result<(), String> {
    let mut mismatch = None;
    let root_dialect = dialect.detect(schema);
    crate::schema_modifiers::for_each_schema_node_in_dialect(
        schema,
        dialect,
        &mut |node, path, scope| {
            if mismatch.is_some() {
                return;
            }
            let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
                return;
            };
            let Some(target) = target_dialect(scope, reference, provider, (schema, root_dialect))
            else {
                return;
            };
            let dialect = scope.dialect;
            if target != dialect {
                let location = if path.is_empty() {
                    "$ref".to_owned()
                } else {
                    format!("{path}/$ref")
                };
                mismatch = Some(format!(
                    "'{location}' ('{reference}') is read under {} but its target declares {}; \
                 a $ref must not cross dialects",
                    dialect_name(dialect),
                    dialect_name(target)
                ));
            }
        },
    );
    mismatch.map_or(Ok(()), Err)
}

/// Rejects a subschema read under another dialect than `dialect`, the one
/// `schema` declares.
///
/// JSON Schema lets any subschema, an embedded resource or not, switch
/// dialect with its own `$schema`. GTS reads every part of a type, its trait
/// schema included, under the dialect of its hierarchy, so a nested `$schema`
/// may only restate that dialect.
///
/// # Errors
/// The first subschema that switches dialect, by location.
pub fn check_subschemas(schema: &Value, dialect: Draft) -> Result<(), String> {
    let mut mismatch = None;
    crate::schema_modifiers::for_each_schema_node_in_dialect(
        schema,
        dialect,
        &mut |_, path, scope| {
            if mismatch.is_none() && scope.dialect != dialect {
                mismatch = Some(format!(
                    "'{path}' declares {} but the type is read under {}; \
                 a subschema must not change dialect",
                    dialect_name(scope.dialect),
                    dialect_name(dialect)
                ));
            }
        },
    );
    mismatch.map_or(Ok(()), Err)
}

/// The dialect in effect at the target of `reference`, made from a node in
/// `scope`, when it resolves.
fn target_dialect(
    scope: SchemaScope<'_>,
    reference: &str,
    provider: &dyn SchemaProvider,
    root: (&Value, Draft),
) -> Option<Draft> {
    match reference.strip_prefix(GTS_ID_URI_PREFIX) {
        Some(target) => {
            let (id, pointer) = target.split_once('#').unwrap_or((target, ""));
            let document = provider.schema_content(id)?;
            let dialect = if std::ptr::eq(document, root.0) {
                root.1
            } else {
                Draft::default().detect(document)
            };
            dialect_at(document, dialect, pointer)
        }
        None => dialect_at(
            scope.resource,
            scope.resource_dialect,
            reference.strip_prefix('#')?,
        ),
    }
}

/// The dialect in effect at `pointer` below `resource`, which is read under
/// `dialect`: the nearest `$schema` on the way down.
fn dialect_at(resource: &Value, dialect: Draft, pointer: &str) -> Option<Draft> {
    let mut dialect = dialect;
    if pointer.is_empty() {
        return Some(dialect);
    }
    let mut node = resource;
    for token in pointer.strip_prefix('/')?.split('/') {
        let token = token.replace("~1", "/").replace("~0", "~");
        node = match node {
            Value::Object(map) => map.get(&token)?,
            Value::Array(items) => items.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
        dialect = dialect.detect(node);
    }
    Some(dialect)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "schema_dialect_test.rs"]
mod schema_dialect_test;
