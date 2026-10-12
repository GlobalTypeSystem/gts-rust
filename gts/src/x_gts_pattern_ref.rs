//! `x-gts-pattern-ref`: string fields holding GTS identifier patterns
//! (spec v0.16 §9.6.2, §10; ADR-0007).
//!
//! Syntax and selection-set coverage are provided by `gts-id`. This module
//! resolves the selected-type operand and integrates the shared pattern type
//! with JSON Schema validation, without registry lookups.

use serde_json::{Map, Value};

use crate::gts::{GTS_ID_URI_PREFIX, GtsIdPattern};

/// The keyword name.
pub const X_GTS_PATTERN_REF: &str = "x-gts-pattern-ref";

/// The selected-type self-reference, the one pointer operand allowed.
const SELF_ID_POINTER: &str = "/$id";

/// The pattern an operand denotes, or why the schema is invalid.
///
/// `selected` is the effective leaf type `/$id` names, without `gts://`.
pub fn resolve_operand(declared: &Value, selected: Option<&str>) -> Result<GtsIdPattern, String> {
    let Some(declared) = declared.as_str() else {
        return Err(format!(
            "{X_GTS_PATTERN_REF} operand must be a string, got {declared}"
        ));
    };
    if declared == SELF_ID_POINTER {
        let Some(selected) = selected else {
            return Err(format!(
                "{X_GTS_PATTERN_REF} operand '{SELF_ID_POINTER}' has no selected GTS Type \
                 Schema '$id' to resolve to"
            ));
        };
        return GtsIdPattern::try_new(selected)
            .ok()
            .filter(|_| selected.ends_with('~'))
            .ok_or_else(|| {
                format!(
                    "{X_GTS_PATTERN_REF} operand '{SELF_ID_POINTER}' resolves to '{selected}', \
                     which is not a GTS Type Identifier"
                )
            });
    }
    GtsIdPattern::try_new(declared).map_err(|reason| {
        format!(
            "{X_GTS_PATTERN_REF} operand '{declared}': {}; expected a GTS identifier, a \
             wildcard pattern or '{SELF_ID_POINTER}'",
            reason.cause
        )
    })
}

/// The `/$id` a document names when it is the type being validated.
pub fn self_id(schema: &Value) -> Option<String> {
    schema
        .get("$id")
        .and_then(Value::as_str)
        .map(|id| id.strip_prefix(GTS_ID_URI_PREFIX).unwrap_or(id).to_owned())
}

/// Checks every operand at a schema position of `schema`, including its trait
/// schema, with `/$id` naming the document's own `$id`.
///
/// # Errors
/// The first unusable operand, with its location.
pub fn check_operands(schema: &Value) -> Result<(), String> {
    let selected = self_id(schema);
    let mut first = None;
    crate::schema_modifiers::for_each_schema_node(schema, &mut |node, location| {
        if first.is_some() {
            return;
        }
        if let Some(declared) = node.get(X_GTS_PATTERN_REF)
            && let Err(reason) = resolve_operand(declared, selected.as_deref())
        {
            let location = if location.is_empty() {
                X_GTS_PATTERN_REF.to_owned()
            } else {
                format!("{location}/{X_GTS_PATTERN_REF}")
            };
            first = Some(format!("{location}: {reason}"));
        }
    });
    first.map_or(Ok(()), Err)
}

/// Registers the keyword. Unusable operands fail compilation, so `not` cannot
/// invert them; `/$id` names `selected` wherever it is declared.
pub fn with_x_gts_pattern_ref(
    options: jsonschema::ValidationOptions,
    selected: Option<String>,
) -> jsonschema::ValidationOptions {
    options.with_keyword(X_GTS_PATTERN_REF, move |_parent, declared, _location| {
        let operand = resolve_operand(declared, selected.as_deref())
            .map_err(jsonschema::ValidationError::schema)?;
        let spelling = match declared.as_str() {
            Some(SELF_ID_POINTER) | None => selected.clone().unwrap_or_default(),
            Some(spelling) => spelling.to_owned(),
        };
        Ok(Box::new(PatternRefKeyword { operand, spelling }))
    })
}

/// One compiled declaration.
struct PatternRefKeyword {
    operand: GtsIdPattern,
    /// The operand as reported, `/$id` resolved.
    spelling: String,
}

impl PatternRefKeyword {
    /// Why a string fails; other JSON types are not constrained.
    fn violation(&self, instance: &Value) -> Option<String> {
        let value = instance.as_str()?;
        match GtsIdPattern::try_new(value) {
            Err(reason) => Some(format!(
                "'{value}' is not a valid GTS identifier pattern: {}",
                reason.cause
            )),
            Ok(pattern) if !self.operand.covers(&pattern) => Some(format!(
                "'{value}' selects GTS identifiers outside '{}'",
                self.spelling
            )),
            Ok(_) => None,
        }
    }
}

impl<'i> jsonschema::Keyword<'i> for PatternRefKeyword {
    fn validate(&self, instance: &'i Value) -> Result<(), jsonschema::ValidationError<'i>> {
        match self.violation(instance) {
            Some(reason) => Err(jsonschema::ValidationError::custom(reason)),
            None => Ok(()),
        }
    }

    fn is_valid(&self, instance: &Value) -> bool {
        self.violation(instance).is_none()
    }
}

/// `schema` with every `/$id` operand replaced by the document's own `$id`, so
/// that two effective schemas can be compared by their literal operands. The
/// document is returned unchanged when it has no `$id` to substitute.
pub fn with_resolved_self_operands(schema: &Value) -> std::borrow::Cow<'_, Value> {
    match self_id(schema) {
        Some(selected) => with_self_operands_naming(schema, &selected),
        None => std::borrow::Cow::Borrowed(schema),
    }
}

/// `schema` with every `/$id` operand replaced by `selected`, for a fragment
/// validated on behalf of a type it does not name itself, such as an effective
/// trait schema (README §9.7.5).
pub fn with_self_operands_naming<'s>(
    schema: &'s Value,
    selected: &str,
) -> std::borrow::Cow<'s, Value> {
    let mut holders = std::collections::HashSet::new();
    crate::schema_modifiers::for_each_schema_node(schema, &mut |node, _| {
        if node.get(X_GTS_PATTERN_REF).and_then(Value::as_str) == Some(SELF_ID_POINTER) {
            holders.insert(std::ptr::from_ref(node));
        }
    });
    if holders.is_empty() {
        return std::borrow::Cow::Borrowed(schema);
    }
    std::borrow::Cow::Owned(substitute(schema, &holders, selected))
}

fn substitute(
    value: &Value,
    holders: &std::collections::HashSet<*const Map<String, Value>>,
    selected: &str,
) -> Value {
    match value {
        Value::Object(map) => {
            let mut copy: Map<String, Value> = map
                .iter()
                .map(|(key, child)| (key.clone(), substitute(child, holders, selected)))
                .collect();
            if holders.contains(&std::ptr::from_ref(map)) {
                copy.insert(
                    X_GTS_PATTERN_REF.to_owned(),
                    Value::String(selected.to_owned()),
                );
            }
            Value::Object(copy)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|child| substitute(child, holders, selected))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The literal pattern an already `/$id`-resolved operand denotes.
pub fn literal_operand(declared: &Value) -> Option<GtsIdPattern> {
    declared
        .as_str()
        .and_then(|text| GtsIdPattern::try_new(text).ok())
}

#[cfg(test)]
#[path = "x_gts_pattern_ref_test.rs"]
mod x_gts_pattern_ref_test;
