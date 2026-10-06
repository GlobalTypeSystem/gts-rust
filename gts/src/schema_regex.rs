//! Checks the regular expressions of schema documents against the GTS
//! profile ([`crate::regex_profile`], README §11.0.1).
//!
//! Checks active and inactive schema positions, root `x-gts-traits-schema`,
//! and initial targets of `$ref` and dialect-specific recursive/dynamic refs.
//! Literal data and unknown keywords are visited only through references;
//! Draft-07 and earlier ignore `$ref` siblings. Unresolved refs are left to
//! validator compilation.

use std::collections::HashSet;

use jsonschema::{Draft, Registry};
use serde_json::{Map, Value};

use crate::schema_traits::X_GTS_TRAITS_SCHEMA;

/// Where `jsonschema` places a root document without an `$id`.
pub const DEFAULT_BASE_URI: &str = "json-schema:///";

/// Checks every expression in the schema positions of `documents`, each
/// given with the URI `registry` knows it by.
///
/// # Errors
/// The first unsupported expression, with its location.
pub fn check(registry: &Registry<'_>, documents: &[(&str, &Value)]) -> Result<(), String> {
    let mut walk = Walk {
        roots: documents
            .iter()
            .filter_map(|(_, document)| document.as_object())
            .map(std::ptr::from_ref)
            .collect(),
        visited: HashSet::new(),
        checked: HashSet::new(),
    };
    for (uri, document) in documents {
        let resolver = jsonschema::uri::from_str(uri.trim_end_matches('#'))
            .ok()
            .map(|base| registry.resolver(base));
        let draft = Draft::default().detect(document);
        walk.node(document, uri, draft, resolver.as_ref())?;
    }
    Ok(())
}

type NodeKey = *const Map<String, Value>;

/// How a keyword holds subschemas.
enum Holds {
    /// One subschema.
    Schema,
    /// An array of subschemas.
    Array,
    /// An object whose values are subschemas.
    Map,
    /// `items` before 2020-12: a subschema or an array of them.
    SchemaOrArray,
    /// Draft-07 `dependencies`: subschemas, excluding property lists.
    Dependencies,
}

/// The subschema keywords `draft` defines.
fn holds(draft: Draft, keyword: &str) -> Option<Holds> {
    let legacy = matches!(draft, Draft::Draft4 | Draft::Draft6 | Draft::Draft7);
    let modern = !legacy;
    let since_6 = draft != Draft::Draft4;
    let since_7 = since_6 && draft != Draft::Draft6;
    let since_2020 = matches!(draft, Draft::Draft202012 | Draft::Unknown);
    Some(match keyword {
        "not" | "additionalProperties" => Holds::Schema,
        "propertyNames" | "contains" if since_6 => Holds::Schema,
        "if" | "then" | "else" if since_7 => Holds::Schema,
        "allOf" | "anyOf" | "oneOf" => Holds::Array,
        "properties" | "patternProperties" => Holds::Map,
        "items" if since_2020 => Holds::Schema,
        "items" => Holds::SchemaOrArray,
        "additionalItems" if !since_2020 => Holds::Schema,
        "prefixItems" if since_2020 => Holds::Array,
        "definitions" if legacy => Holds::Map,
        "dependencies" if legacy => Holds::Dependencies,
        "$defs" | "dependentSchemas" if modern => Holds::Map,
        "unevaluatedItems" | "unevaluatedProperties" | "contentSchema" if modern => Holds::Schema,
        _ => return None,
    })
}

struct Walk {
    /// Document roots: only there is `x-gts-traits-schema` a schema position.
    roots: HashSet<NodeKey>,
    /// Nodes visited, with the dialect they were read under.
    visited: HashSet<(NodeKey, Draft)>,
    /// Expressions already found supported.
    checked: HashSet<String>,
}

impl Walk {
    /// Visits `value`, entered structurally from its parent's scope.
    fn node<'r>(
        &mut self,
        value: &'r Value,
        location: &str,
        draft: Draft,
        resolver: Option<&referencing::Resolver<'r>>,
    ) -> Result<(), String> {
        let draft = draft.detect(value);
        // `$id` changes the reference base; invalid scopes fail compilation.
        let resolver = resolver.map(|resolver| {
            resolver
                .in_subresource(draft.create_resource_ref(value))
                .unwrap_or_else(|_| resolver.clone())
        });
        self.scoped(value, location, draft, resolver.as_ref())
    }

    /// Visits `value` with `resolver` already in its scope.
    fn scoped<'r>(
        &mut self,
        value: &'r Value,
        location: &str,
        draft: Draft,
        resolver: Option<&referencing::Resolver<'r>>,
    ) -> Result<(), String> {
        let Value::Object(map) = value else {
            return Ok(());
        };
        let draft = draft.detect(value);
        let node = std::ptr::from_ref(map);
        if !self.visited.insert((node, draft)) {
            return Ok(());
        }

        if self.roots.contains(&std::ptr::from_ref(map))
            && let Some(traits) = map.get(X_GTS_TRAITS_SCHEMA)
        {
            self.node(
                traits,
                &join(location, X_GTS_TRAITS_SCHEMA),
                draft,
                resolver,
            )?;
        }

        let legacy = matches!(draft, Draft::Draft4 | Draft::Draft6 | Draft::Draft7);
        for (keyword, defined) in [
            ("$ref", true),
            ("$recursiveRef", draft == Draft::Draft201909),
            (
                "$dynamicRef",
                matches!(draft, Draft::Draft202012 | Draft::Unknown),
            ),
        ] {
            if let (true, Some(Value::String(reference)), Some(resolver)) =
                (defined, map.get(keyword), resolver)
            {
                self.reference(keyword, reference, draft, resolver)?;
            }
        }
        if legacy && map.contains_key("$ref") {
            return Ok(());
        }

        if let Some(Value::String(expression)) = map.get("pattern") {
            self.expression(expression, &join(location, "pattern"))?;
        }
        if let Some(Value::Object(patterns)) = map.get("patternProperties") {
            let at = join(location, "patternProperties");
            for expression in patterns.keys() {
                self.expression(expression, &join(&at, expression))?;
            }
        }

        for (keyword, child) in map {
            let Some(holds) = holds(draft, keyword) else {
                continue;
            };
            let at = join(location, keyword);
            let visit = |walk: &mut Self, schema: &'r Value, at: &str| {
                walk.node(schema, at, draft, resolver)
            };
            match (holds, child) {
                (Holds::Schema | Holds::SchemaOrArray, Value::Object(_)) => {
                    visit(self, child, &at)?;
                }
                (Holds::Array | Holds::SchemaOrArray, Value::Array(items)) => {
                    for (index, item) in items.iter().enumerate() {
                        visit(self, item, &join(&at, &index.to_string()))?;
                    }
                }
                (Holds::Map | Holds::Dependencies, Value::Object(members)) => {
                    for (name, member) in members {
                        visit(self, member, &join(&at, name))?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Visits the initial target of `reference`.
    fn reference(
        &mut self,
        keyword: &str,
        reference: &str,
        draft: Draft,
        resolver: &referencing::Resolver<'_>,
    ) -> Result<(), String> {
        let lookup = if keyword == "$recursiveRef" {
            resolver.lookup_recursive_ref()
        } else {
            resolver.lookup(reference)
        };
        let Ok(found) = lookup else {
            return Ok(());
        };
        let (target, target_resolver, target_draft) = found.into_inner();
        let draft = if matches!(target_draft, Draft::Unknown) {
            draft
        } else {
            target_draft
        };
        // Lookup entered the target's scope; re-entering would apply relative `$id` twice.
        self.scoped(target, reference, draft, Some(&target_resolver))
    }

    fn expression(&mut self, expression: &str, location: &str) -> Result<(), String> {
        const SHOWN: usize = 80;
        if self.checked.contains(expression) {
            return Ok(());
        }
        let shown: String = expression.chars().take(SHOWN).collect();
        let ellipsis = if shown.len() < expression.len() {
            "..."
        } else {
            ""
        };
        crate::regex_profile::check(expression).map_err(|reason| {
            format!(
                "unsupported regular expression '{shown}{ellipsis}' at '{location}': {reason} \
                 (GTS regular-expression profile, README section 11.0.1)"
            )
        })?;
        // An in-profile expression the engine cannot compile is an engine
        // failure, not an unsupported expression; report it with the schema
        // rather than at the first match.
        compile_for_engine(expression).map_err(|reason| {
            format!(
                "regular expression engine failed on '{shown}{ellipsis}' at '{location}': {reason}"
            )
        })?;
        self.checked.insert(expression.to_owned());
        Ok(())
    }
}

/// Compiles `expression` as `jsonschema` does with `PatternOptions::regex()`:
/// its ECMA-262 translation, with the `regex` crate's default limits.
///
/// # Errors
/// Why translation or compilation failed.
pub fn compile_for_engine(expression: &str) -> Result<(), String> {
    let translated = jsonschema_regex::to_rust_regex(expression)
        .map_err(|()| "translation to the regex engine syntax failed".to_owned())?;
    regex::Regex::new(&translated)
        .map(drop)
        .map_err(|error| error.to_string())
}

fn join(location: &str, segment: &str) -> String {
    let segment = segment.replace('~', "~0").replace('/', "~1");
    if location.contains('#') {
        format!("{location}/{segment}")
    } else {
        format!("{location}#/{segment}")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "schema_regex_test.rs"]
mod schema_regex_test;
