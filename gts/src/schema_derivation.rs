//! OP#12 - Schema-vs-schema derivation admission.
//!
//! Given a chained GTS schema ID like `gts.A~B~C~`, this module validates that
//! each derived schema may be admitted under its base:
//!
//! - B (derived from A) must be admissible under A
//! - C (derived from A~B) must be admissible under A~B
//!
//! Admission requires `Valid(derived) ⊆ Valid(base)` - every valid instance of
//! the derived schema is also a valid instance of the base. That is the same
//! accepted-instance-set inclusion that schema evolution checks, so this module
//! owns no keyword semantics of its own: it calls the checker in
//! [`crate::schema_evolution`] and adds the two admission rules that inclusion alone
//! does not express.

use crate::schema_evolution::{
    CompatibilityFinding, MAX_RECURSION_DEPTH, check_accepted_set_inclusion, flatten_schema,
};
use crate::schema_semantics::boolean_schema_value;
use serde_json::Value;

const ADDITIONAL_PROPERTIES: &str = "additionalProperties";

/// Validates that a derived JSON Schema value may be admitted under its base.
///
/// Combines set inclusion with the admission rules that inclusion does not
/// express; see [`validate_derivation`] and
/// [`validate_closed_descendant_branches`].
pub(crate) fn validate_derivation_compatibility(
    base_schema: &Value,
    derived_schema: &Value,
    base_id: &str,
    derived_id: &str,
) -> Vec<String> {
    let mut errors = validate_derivation(base_schema, derived_schema, base_id, derived_id);
    errors.extend(validate_closed_descendant_branches(
        base_schema,
        derived_schema,
        base_id,
        derived_id,
    ));
    errors
}

/// Checks that the derived *declaration* is included in the base.
///
/// Inclusion is the backward relation of the evolution checker with the derived
/// definition in the older position, so derivation and evolution decide the
/// same question through one engine.
///
/// What differs is the input. A derived document embeds its base through
/// `allOf`/`$ref`, so intersecting the branches would ask a question whose
/// answer is always yes - composition can never widen the base. GTS instead
/// forbids a derivation from *declaring* a constraint looser than the one it
/// inherits, which is a statement about the most-derived declaration of each
/// property. Both sides are therefore reduced with [`declared_schema`] before
/// the shared checker compares them.
///
/// Admission fails closed: a pair the checker reports as `Unknown` is rejected.
/// Evolution can hand an undecided verdict back to its caller, but admitting a
/// derivation whose inclusion nobody could prove is exactly how an instance of
/// the derived type ends up failing validation against its base.
pub(crate) fn validate_derivation(
    base_schema: &Value,
    derived_schema: &Value,
    base_id: &str,
    derived_id: &str,
) -> Vec<String> {
    let base = declared_schema(base_schema, 0);
    let mut derived = declared_schema(derived_schema, 0);
    let mut errors = Vec::new();

    // An omitted `additionalProperties` is not a declaration: across dialects
    // the base's constraint still applies to the same instance through
    // `allOf`/`$ref` composition, so the derived level inherits it rather than
    // reopening. Declaring a permissive value explicitly is a different act and
    // is reported below.
    if let (Some(derived_map), Some(inherited)) = (
        derived.as_object_mut(),
        base.get(ADDITIONAL_PROPERTIES).cloned(),
    ) && !derived_map.contains_key(ADDITIONAL_PROPERTIES)
    {
        derived_map.insert(ADDITIONAL_PROPERTIES.to_owned(), inherited);
    }
    if base
        .get(ADDITIONAL_PROPERTIES)
        .and_then(boolean_schema_value)
        == Some(false)
        && derived_schema
            .get(ADDITIONAL_PROPERTIES)
            .is_some_and(|declared| boolean_schema_value(declared) != Some(false))
    {
        errors.push(format!(
            "derived schema '{derived_id}' loosens additionalProperties from a closed constraint \
             in base '{base_id}'"
        ));
    }

    // A dialect difference is not an admission failure: GTS pins no draft and
    // sets each schema's dialect from its own `$schema` (spec sec 11), so a
    // derivation may declare a newer draft than the base it tightens. The
    // dialect still governs how the rest of the comparison reads keywords.
    let (_, diagnostics) = check_accepted_set_inclusion(&derived, &base);
    errors.extend(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.finding != CompatibilityFinding::DialectChanged)
            .map(|diagnostic| {
                format!(
                    "derived schema '{derived_id}' is not included in base '{base_id}': \
                     {diagnostic}"
                )
            }),
    );
    collect_disabled_base_properties(&base, &derived, base_id, derived_id, &mut errors);
    errors
}

/// Reduces a schema to what it *declares*, with the innermost branch winning.
///
/// `allOf` branches are folded in order and the last declaration of a property
/// replaces earlier ones, so a derived overlay that restates an inherited
/// property is read as that overlay's constraint rather than as its
/// intersection with the base. `additionalProperties` is the exception: it
/// folds through the closedness-preserving lattice, because a permissive
/// overlay cannot reopen a branch that closed the level.
///
/// Below [`MAX_RECURSION_DEPTH`] the schema is returned as authored. Admission
/// fails closed, so the shared checker reads an unreduced declaration as looser
/// than it is and rejects the derivation rather than admitting it unchecked.
fn declared_schema(schema: &Value, depth: usize) -> Value {
    let Some(map) = schema.as_object() else {
        return schema.clone();
    };
    if depth >= MAX_RECURSION_DEPTH {
        return schema.clone();
    }
    let mut declared = serde_json::Map::new();
    let mut additional_properties = None;

    if let Some(branches) = map.get("allOf").and_then(Value::as_array) {
        for branch in branches {
            if let Some(branch) = declared_schema(branch, depth + 1).as_object() {
                absorb_declaration(&mut declared, &mut additional_properties, branch, depth);
            }
        }
    }
    absorb_declaration(&mut declared, &mut additional_properties, map, depth);

    if let Some(additional_properties) = additional_properties {
        declared.insert(ADDITIONAL_PROPERTIES.to_owned(), additional_properties);
    }
    Value::Object(declared)
}

/// Folds one declaration level into the accumulated one.
fn absorb_declaration(
    declared: &mut serde_json::Map<String, Value>,
    additional_properties: &mut Option<Value>,
    source: &serde_json::Map<String, Value>,
    depth: usize,
) {
    for (keyword, value) in source {
        match keyword.as_str() {
            "allOf" => {}
            ADDITIONAL_PROPERTIES => {
                merge_additional_properties_constraint(additional_properties, value);
            }
            "properties" => {
                let target = declared
                    .entry("properties")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                if let (Some(target), Some(source)) = (target.as_object_mut(), value.as_object()) {
                    for (name, property) in source {
                        let property = declared_schema(property, depth + 1);
                        match target.get_mut(name) {
                            Some(inherited) => absorb_property(inherited, &property, depth + 1),
                            None => {
                                target.insert(name.clone(), property);
                            }
                        }
                    }
                }
            }
            "required" => {
                let target = declared
                    .entry("required")
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let (Some(target), Some(source)) = (target.as_array_mut(), value.as_array()) {
                    for name in source {
                        if !target.contains(name) {
                            target.push(name.clone());
                        }
                    }
                }
            }
            _ => {
                declared.insert(keyword.clone(), value.clone());
            }
        }
    }
}

/// Keywords that describe an object level's structure rather than the values a
/// single property accepts.
const STRUCTURAL_KEYWORDS: &[&str] = &["properties", "required", ADDITIONAL_PROPERTIES];

/// Folds an overlay's declaration of a property into the inherited one.
///
/// A derivation that restates a property redeclares that property's own value
/// constraints: dropping the base's `maxLength` while restating `type` is a
/// looser declaration, not an inheritance of the bound. Object structure
/// composes instead - the nested `properties`, `required` and
/// `additionalProperties` of the base still apply through `allOf`, so an
/// overlay that specifies a nested object without repeating its `required` list
/// is not loosening anything.
fn absorb_property(inherited: &mut Value, overlay: &Value, depth: usize) {
    let (Some(inherited_map), Some(overlay_map)) = (inherited.as_object(), overlay.as_object())
    else {
        *inherited = overlay.clone();
        return;
    };
    if depth >= MAX_RECURSION_DEPTH {
        *inherited = overlay.clone();
        return;
    }

    let mut composed: serde_json::Map<String, Value> = overlay_map
        .iter()
        .filter(|(keyword, _)| !STRUCTURAL_KEYWORDS.contains(&keyword.as_str()))
        .map(|(keyword, value)| (keyword.clone(), value.clone()))
        .collect();
    let mut additional_properties = inherited_map.get(ADDITIONAL_PROPERTIES).cloned();
    for keyword in ["properties", "required"] {
        if let Some(value) = inherited_map.get(keyword) {
            composed.insert(keyword.to_owned(), value.clone());
        }
    }
    absorb_declaration(
        &mut composed,
        &mut additional_properties,
        overlay_map,
        depth,
    );
    if let Some(additional_properties) = additional_properties {
        composed.insert(ADDITIONAL_PROPERTIES.to_owned(), additional_properties);
    }
    *inherited = Value::Object(composed);
}

/// Folds an `additionalProperties` value into an accumulator using a
/// closedness-preserving lattice: schemas equivalent to `false` (closed) are
/// strongest, nontrivial constraining schemas are in the middle, and schemas
/// equivalent to `true` (open) are weakest.
///
/// This mirrors `allOf` composition, where the level stays closed if **any**
/// branch gives `additionalProperties` a false-equivalent schema, so a
/// permissive overlay can never loosen a closed base.
fn merge_additional_properties_constraint(current: &mut Option<Value>, candidate: &Value) {
    if current.as_ref().and_then(boolean_schema_value) == Some(false) {
        return;
    }
    if boolean_schema_value(candidate) == Some(true) && current.is_some() {
        // Intersecting an existing constraint with `true` changes nothing.
        return;
    }
    *current = Some(candidate.clone());
}

/// Rejects a derived schema that switches a base property off with `false`.
///
/// Set inclusion permits it - rejecting every instance that carries the
/// property keeps the derived set inside the base set - but a derivation that
/// makes an inherited property unusable is not a valid specialization of the
/// base contract, so this is an admission rule rather than a compatibility one.
fn collect_disabled_base_properties(
    base_schema: &Value,
    derived_schema: &Value,
    base_id: &str,
    derived_id: &str,
    errors: &mut Vec<String>,
) {
    let base_flat = flatten_schema(base_schema);
    let derived_flat = flatten_schema(derived_schema);
    let base_properties = base_flat.get("properties").and_then(Value::as_object);
    let derived_properties = derived_flat.get("properties").and_then(Value::as_object);
    for (name, derived_property) in derived_properties.into_iter().flatten() {
        if *derived_property == Value::Bool(false)
            && base_properties.is_some_and(|properties| properties.contains_key(name))
        {
            errors.push(format!(
                "property '{name}': derived schema '{derived_id}' disables property defined in \
                base '{base_id}'"
            ));
        }
    }
}

/// Validates branch-scoped closed `additionalProperties` in a descendant schema.
///
/// Flattened compatibility catches closed ancestors that reject new descendant
/// properties, but it cannot see the inverse `allOf` hazard: a descendant
/// branch can close `additionalProperties` without restating an ancestor property
/// at the same object path, making that ancestor property unusable in the
/// composed schema. This walks the raw/resolved descendant branches so that
/// branch ownership is preserved.
pub(crate) fn validate_closed_descendant_branches(
    ancestor_schema: &Value,
    descendant_schema: &Value,
    ancestor_label: &str,
    descendant_label: &str,
) -> Vec<String> {
    let mut errors = Vec::new();
    collect_closed_descendant_branch_errors(
        &flatten_schema(ancestor_schema),
        descendant_schema,
        "",
        0,
        ancestor_label,
        descendant_label,
        &mut errors,
    );
    errors
}

/// `ancestor` is already flattened: every `allOf` branch of the descendant is
/// compared against the same ancestor level, so flattening here would redo the
/// identical merge once per branch at every depth.
fn collect_closed_descendant_branch_errors(
    ancestor: &Value,
    descendant_schema: &Value,
    path: &str,
    depth: usize,
    ancestor_label: &str,
    descendant_label: &str,
    errors: &mut Vec<String>,
) {
    if depth >= MAX_RECURSION_DEPTH {
        errors.push(format!(
            "schema compatibility check exceeded maximum nesting depth of \
             {MAX_RECURSION_DEPTH} at '{path}' between ancestor '{ancestor_label}' \
             and descendant '{descendant_label}'"
        ));
        return;
    }

    let ancestor_props = ancestor.get("properties").and_then(Value::as_object);
    let Some(descendant_obj) = descendant_schema.as_object() else {
        return;
    };
    let descendant_props = descendant_obj.get("properties").and_then(Value::as_object);

    if descendant_obj
        .get("additionalProperties")
        .and_then(boolean_schema_value)
        == Some(false)
    {
        let mut orphaned: Vec<&str> = ancestor_props
            .into_iter()
            .flatten()
            .map(|(name, _)| name.as_str())
            .filter(|name| !descendant_props.is_some_and(|props| props.contains_key(*name)))
            .collect();
        orphaned.sort_unstable();
        for name in orphaned {
            let property_path = join_schema_path(path, name);
            errors.push(format!(
                "property '{property_path}': descendant schema '{descendant_label}' sets \
                 a closed additionalProperties constraint but does not restate property defined in \
                 ancestor '{ancestor_label}', making it unusable under allOf composition"
            ));
        }
    }

    if let Some(props) = descendant_props {
        let mut common: Vec<&str> = props
            .keys()
            .filter(|name| ancestor_props.is_some_and(|ancestor| ancestor.contains_key(*name)))
            .map(String::as_str)
            .collect();
        common.sort_unstable();

        for name in common {
            let Some(ancestor_prop) = ancestor_props.and_then(|props| props.get(name)) else {
                continue;
            };
            let Some(descendant_prop) = props.get(name) else {
                continue;
            };

            let next_path = join_schema_path(path, name);
            collect_closed_descendant_branch_errors(
                &flatten_schema(ancestor_prop),
                descendant_prop,
                &next_path,
                depth + 1,
                ancestor_label,
                descendant_label,
                errors,
            );
        }
    }

    if let Some(Value::Array(all_of)) = descendant_obj.get("allOf") {
        for item in all_of {
            collect_closed_descendant_branch_errors(
                ancestor,
                item,
                path,
                depth + 1,
                ancestor_label,
                descendant_label,
                errors,
            );
        }
    }
}

fn join_schema_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

#[cfg(test)]
#[path = "schema_derivation_test.rs"]
mod schema_derivation_test;
