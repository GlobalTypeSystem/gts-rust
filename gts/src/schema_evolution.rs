//! Type Schema Evolution Compatibility (spec sec 4.2, OP#8).
//!
//! Compares two definitions of one type identity by the instances they accept
//! and reports the backward, forward, and full verdicts. The relation is
//! `Valid(old) ⊆ Valid(new)` for backward and the reverse inclusion for
//! forward; sec 4.3 defines the modes.
//!
//! Derivation is a different relation over a different pair of schemas (sec
//! 4.1, `crate::schema_derivation`) and the spec is emphatic that the two must
//! not be conflated. What they share is the inclusion test itself, exposed
//! here as [`check_accepted_set_inclusion`] under a name that belongs to
//! neither relation.

use crate::schema_semantics::boolean_schema_value;
use num_cmp::NumCmp;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashSet};

/// Result of attempting to establish one schema-compatibility relation.
///
/// `Unknown` is deliberately distinct from `Incompatible`: it means the
/// checker could not prove or disprove the required accepted-instance-set
/// inclusion. The caller, not this library, decides how that affects admission.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityVerdict {
    Compatible,
    Incompatible,
    #[default]
    Unknown,
}

impl CompatibilityVerdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compatible => "compatible",
            Self::Incompatible => "incompatible",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn is_compatible(self) -> bool {
        matches!(self, Self::Compatible)
    }

    #[must_use]
    pub const fn is_incompatible(self) -> bool {
        matches!(self, Self::Incompatible)
    }

    #[must_use]
    pub const fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// Derives full compatibility from the two directional verdicts.
    #[must_use]
    pub const fn full(backward: Self, forward: Self) -> Self {
        match (backward, forward) {
            (Self::Compatible, Self::Compatible) => Self::Compatible,
            (Self::Incompatible, _) | (_, Self::Incompatible) => Self::Incompatible,
            _ => Self::Unknown,
        }
    }

    fn from_diagnostics(diagnostics: &[CompatibilityDiagnostic]) -> Self {
        if diagnostics.is_empty() {
            Self::Compatible
        } else if diagnostics
            .iter()
            .all(CompatibilityDiagnostic::is_inconclusive)
        {
            Self::Unknown
        } else {
            Self::Incompatible
        }
    }
}

impl std::fmt::Display for CompatibilityVerdict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
/// Content model of one object level of a **resolved** effective schema.
///
/// Classified per gts-spec §4.4, which requires the level to be judged after
/// `$ref` resolution and `allOf` composition rather than from a single authored
/// keyword. Use [`classify_object_levels`] to obtain the
/// classification of every level of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentModel {
    /// Accepts an undeclared property with any value.
    Open,
    /// Rejects every undeclared property.
    Closed,
    /// Accepts some undeclared property names, or constrains their values - for
    /// example through a nontrivial schema-valued `additionalProperties`,
    /// `patternProperties`, or `propertyNames`.
    Partial,
}

impl ContentModel {
    const fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Partial => "partially open",
        }
    }

    /// Whether a later definition may add an optional property at this level
    /// and stay backward compatible.
    ///
    /// Only a closed level can: an open level already accepted arbitrary values
    /// under the new property name, so declaring it narrows the accepted set
    /// (§4.4). For a partially open level the answer depends on the constraint
    /// that governs undeclared properties, so it is reported as not evolvable
    /// rather than guessed.
    #[must_use]
    pub const fn is_evolvable_in_place(self) -> bool {
        matches!(self, Self::Closed)
    }
}

impl std::fmt::Display for ContentModel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// One object level of a resolved schema, with its content model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectLevel {
    /// Location of the level, `$` for the document root and dotted segments
    /// below it, for example `$.payload` or `$.items[]`.
    pub path: String,
    /// How this level treats undeclared properties.
    pub content_model: ContentModel,
}

/// Machine-readable kind of a [`CompatibilityDiagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityFinding {
    /// A property was declared at a level whose content model does not permit
    /// the addition in this direction.
    PropertyAdded,
    /// A property declaration was dropped at a level whose content model does
    /// not permit the removal in this direction.
    PropertyRemoved,
    /// The set of `required` properties changed.
    RequiredChanged,
    /// The content model of an object level changed.
    ContentModelChanged,
    /// The set of permitted `type` values is not an inclusion in this direction.
    TypeChanged,
    /// The `enum` constraint is not an inclusion in this direction.
    EnumChanged,
    /// A numeric bound moved in the direction this mode forbids.
    BoundChanged,
    /// A keyword that only narrows was added or removed.
    NarrowingConstraintChanged,
    /// A keyword whose values cannot be ordered by inclusion changed.
    ConstraintChanged,
    /// The declared JSON Schema dialect changed, so this checker cannot compare
    /// the two documents under one stable set of keyword semantics.
    DialectChanged,
    /// Inclusion could not be established either way - an unresolved `$ref`, an
    /// `allOf` intersection the checker cannot prove, a partially open level, or
    /// two values of one keyword that this implementation cannot order. It is
    /// reported distinctly so callers can apply their own admission policy.
    NotProvable,
}

/// Evidence explaining an incompatible or unknown directional verdict.
///
/// Carries the schema location separately from the prose so that a caller can
/// report per object level without parsing the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityDiagnostic {
    /// Location of the offending schema node, in the form used by
    /// [`ObjectLevel::path`].
    pub path: String,
    /// What kind of finding this is.
    pub finding: CompatibilityFinding,
    /// Human-readable detail, without the location prefix.
    pub detail: String,
}

impl CompatibilityDiagnostic {
    fn new(path: &str, finding: CompatibilityFinding, detail: String) -> Self {
        Self {
            path: path.to_owned(),
            finding,
            detail,
        }
    }

    const fn is_inconclusive(&self) -> bool {
        matches!(
            self.finding,
            CompatibilityFinding::NotProvable | CompatibilityFinding::DialectChanged
        )
    }
}

impl std::fmt::Display for CompatibilityDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Schema at '{}' {}", self.path, self.detail)
    }
}
/// Locations, relative to the node being flattened, whose `allOf` intersection
/// could not be reduced to an exact single schema.
///
/// The root of the flattened node is the empty string; a property extends the
/// location with `.name` and array items with `[]`, matching the paths
/// [`check_schema_node_compatibility`] descends through.
/// Keywords the checker treats as node-level constraints (`additionalProperties`,
/// `patternProperties`, `propertyNames`, ...) are attributed to their owning
/// node: an intersection this checker cannot prove there makes the whole node
/// unprovable.
type UnprovenPaths = BTreeSet<String>;

/// Whether each side's effective dialect evaluates `unevaluatedProperties`.
#[derive(Debug, Clone, Copy)]
struct DialectSupport {
    old_unevaluated: bool,
    new_unevaluated: bool,
}

/// Narrows `unproven` to the locations inside `child`, rebased so that the
/// empty string denotes `child` itself.
fn unproven_below(unproven: &UnprovenPaths, child: &str) -> UnprovenPaths {
    unproven
        .iter()
        .filter_map(|location| location.strip_prefix(child))
        .filter(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
        .map(ToOwned::to_owned)
        .collect()
}

fn merge_schema_map(
    target: &mut Map<String, Value>,
    candidate: &Map<String, Value>,
    path: &str,
    unproven: &mut UnprovenPaths,
) {
    const ANNOTATIONS: &[&str] = &[
        "$id",
        "$schema",
        "title",
        "description",
        "default",
        "examples",
        "readOnly",
        "writeOnly",
        "deprecated",
        "definitions",
        "$defs",
        "x-gts-abstract",
        "x-gts-final",
        "x-gts-traits",
        "x-gts-traits-schema",
    ];
    const MINIMUMS: &[&str] = &[
        "minimum",
        "exclusiveMinimum",
        "minLength",
        "minItems",
        "minProperties",
        "minContains",
    ];
    const MAXIMUMS: &[&str] = &[
        "maximum",
        "exclusiveMaximum",
        "maxLength",
        "maxItems",
        "maxProperties",
        "maxContains",
    ];

    for (keyword, candidate_value) in candidate {
        if ANNOTATIONS.contains(&keyword.as_str()) {
            target.insert(keyword.clone(), candidate_value.clone());
            continue;
        }
        let Some(current) = target.get_mut(keyword) else {
            target.insert(keyword.clone(), candidate_value.clone());
            continue;
        };
        if current == candidate_value {
            continue;
        }

        match keyword.as_str() {
            "properties" | "patternProperties" => {
                // The checker descends into named properties, so an unprovable
                // property intersection stays local to that property. Pattern
                // properties are compared as a node-level constraint instead.
                let named = keyword == "properties";
                if let (Some(current_map), Some(candidate_map)) =
                    (current.as_object_mut(), candidate_value.as_object())
                {
                    for (name, candidate_schema) in candidate_map {
                        if let Some(current_schema) = current_map.get_mut(name) {
                            let property_path = if named {
                                format!("{path}.{name}")
                            } else {
                                path.to_owned()
                            };
                            merge_schema_intersection(
                                current_schema,
                                candidate_schema,
                                &property_path,
                                unproven,
                            );
                        } else {
                            current_map.insert(name.clone(), candidate_schema.clone());
                        }
                    }
                } else {
                    unproven.insert(path.to_owned());
                }
            }
            "required" => {
                if let (Some(current_items), Some(candidate_items)) =
                    (current.as_array_mut(), candidate_value.as_array())
                {
                    for item in candidate_items {
                        if !current_items.contains(item) {
                            current_items.push(item.clone());
                        }
                    }
                }
            }
            "items" => {
                merge_schema_intersection(current, candidate_value, &format!("{path}[]"), unproven);
            }
            "additionalProperties" | "unevaluatedProperties" | "propertyNames" | "contains" => {
                merge_schema_intersection(current, candidate_value, path, unproven);
            }
            "enum" => {
                if let (Some(current_values), Some(candidate_values)) =
                    (current.as_array_mut(), candidate_value.as_array())
                {
                    current_values.retain(|value| candidate_values.contains(value));
                    if current_values.is_empty() {
                        unproven.insert(path.to_owned());
                    }
                }
            }
            keyword if MINIMUMS.contains(&keyword) => {
                if candidate_value.as_f64() > current.as_f64() {
                    *current = candidate_value.clone();
                }
            }
            keyword if MAXIMUMS.contains(&keyword) => {
                if candidate_value.as_f64() < current.as_f64() {
                    *current = candidate_value.clone();
                }
            }
            "type" => {
                if current.as_str() == Some("number") && candidate_value.as_str() == Some("integer")
                {
                    *current = candidate_value.clone();
                } else if !(current.as_str() == Some("integer")
                    && candidate_value.as_str() == Some("number"))
                {
                    unproven.insert(path.to_owned());
                }
            }
            _ => {
                unproven.insert(path.to_owned());
            }
        }
    }
}

fn merge_schema_intersection(
    target: &mut Value,
    candidate: &Value,
    path: &str,
    unproven: &mut UnprovenPaths,
) {
    match (&mut *target, candidate) {
        (Value::Bool(false), _) | (_, Value::Bool(true)) => {}
        (Value::Bool(true), value) => *target = value.clone(),
        (_, Value::Bool(false)) => *target = Value::Bool(false),
        (Value::Object(target_map), Value::Object(candidate_map)) => {
            merge_schema_map(target_map, candidate_map, path, unproven);
        }
        _ => {
            // Two branches that are not both object schemas have no
            // representable intersection; leave the node unconstrained and let
            // the caller decide what an unprovable location means.
            unproven.insert(path.to_owned());
            *target = Value::Object(Map::new());
        }
    }
}
#[must_use]
pub fn flatten_schema(schema: &Value) -> Value {
    flatten_effective(schema).0
}

/// Flattens `allOf` and reports where the intersection could not be proven.
///
/// The flattened schema is always a usable approximation; the returned
/// [`UnprovenPaths`] tell a compatibility checker which locations it must
/// not draw conclusions about.
fn flatten_effective(schema: &Value) -> (Value, UnprovenPaths) {
    let mut unproven = UnprovenPaths::new();
    let Some(schema_map) = schema.as_object() else {
        return (schema.clone(), unproven);
    };
    let mut result = Value::Bool(true);
    if let Some(all_of) = schema_map.get("allOf").and_then(Value::as_array) {
        for branch in all_of {
            let (flattened_branch, branch_unproven) = flatten_effective(branch);
            unproven.extend(branch_unproven);
            merge_schema_intersection(&mut result, &flattened_branch, "", &mut unproven);
        }
    }
    let direct = Value::Object(
        schema_map
            .iter()
            .filter(|(keyword, _)| keyword.as_str() != "allOf")
            .map(|(keyword, value)| (keyword.clone(), value.clone()))
            .collect(),
    );
    merge_schema_intersection(&mut result, &direct, "", &mut unproven);
    (result, unproven)
}

/// Reports a bound keyword whose value is present but not a number.
///
/// Draft-04 spells `exclusiveMinimum`/`exclusiveMaximum` as booleans that
/// modify `minimum`/`maximum`, so a numeric comparison would silently ignore
/// them. Fall back to exact equality for any non-numeric value rather than
/// guessing which direction it widens.
fn check_non_numeric_bound(
    path: &str,
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
    key: &str,
) -> Option<CompatibilityDiagnostic> {
    let non_numeric = |schema: &Map<String, Value>| {
        schema
            .get(key)
            .is_some_and(|value| value.as_f64().is_none())
    };
    if (non_numeric(old_schema) || non_numeric(new_schema))
        && old_schema.get(key) != new_schema.get(key)
    {
        return Some(CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::NotProvable,
            format!("changes non-numeric '{key}' constraint"),
        ));
    }
    None
}

fn check_min_max_constraint(
    path: &str,
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
    min_key: &str,
    max_key: &str,
    check_tightening: bool,
) -> Vec<CompatibilityDiagnostic> {
    let bound = |detail: String| {
        CompatibilityDiagnostic::new(path, CompatibilityFinding::BoundChanged, detail)
    };
    let mut errors = Vec::new();
    errors.extend(check_non_numeric_bound(
        path, old_schema, new_schema, min_key,
    ));
    errors.extend(check_non_numeric_bound(
        path, old_schema, new_schema, max_key,
    ));

    // Check minimum constraint
    let old_min = old_schema.get(min_key).and_then(Value::as_f64);
    let new_min = new_schema.get(min_key).and_then(Value::as_f64);

    if let (Some(old_m), Some(new_m)) = (old_min, new_min) {
        if check_tightening && new_m > old_m {
            errors.push(bound(format!(
                "{min_key} increased from {old_m} -> {new_m}"
            )));
        } else if !check_tightening && new_m < old_m {
            errors.push(bound(format!(
                "{min_key} decreased from {old_m} -> {new_m}"
            )));
        }
    } else if let (true, None, Some(new_m)) = (check_tightening, old_min, new_min) {
        errors.push(bound(format!("adds {min_key} constraint: {new_m}")));
    } else if !check_tightening && old_min.is_some() && new_min.is_none() {
        errors.push(bound(format!("removes {min_key} constraint")));
    }

    // Check maximum constraint
    let old_max = old_schema.get(max_key).and_then(Value::as_f64);
    let new_max = new_schema.get(max_key).and_then(Value::as_f64);

    if let (Some(old_m), Some(new_m)) = (old_max, new_max) {
        if check_tightening && new_m < old_m {
            errors.push(bound(format!(
                "{max_key} decreased from {old_m} -> {new_m}"
            )));
        } else if !check_tightening && new_m > old_m {
            errors.push(bound(format!(
                "{max_key} increased from {old_m} -> {new_m}"
            )));
        }
    } else if let (true, None, Some(new_m)) = (check_tightening, old_max, new_max) {
        errors.push(bound(format!("adds {max_key} constraint: {new_m}")));
    } else if !check_tightening && old_max.is_some() && new_max.is_none() {
        errors.push(bound(format!("removes {max_key} constraint")));
    }

    errors
}

/// Returns the effective lower or upper numeric bound.
///
/// Draft 6 and later allow an inclusive and an exclusive bound to coexist;
/// their intersection is the stricter of the two (with exclusive winning
/// when the numeric values are equal). Draft 4's boolean
/// `exclusiveMinimum`/`exclusiveMaximum` spelling is handled as a modifier
/// of the corresponding inclusive bound.
fn effective_numeric_bound(
    schema: &Map<String, Value>,
    inclusive_key: &str,
    exclusive_key: &str,
    is_lower: bool,
) -> Result<Option<(f64, bool)>, ()> {
    // `total_cmp` orders `-0.0` below `0.0`, but the two denote the same JSON
    // number and must compare equal, so the sign of zero is dropped as the
    // bound is read.
    let bound_value = |value: &Value| -> Result<f64, ()> {
        let value = value.as_f64().ok_or(())?;
        Ok(if value == 0.0 { 0.0 } else { value })
    };
    let inclusive = match schema.get(inclusive_key) {
        Some(value) => Some((bound_value(value)?, false)),
        None => None,
    };
    let exclusive = match schema.get(exclusive_key) {
        Some(Value::Bool(is_exclusive)) => inclusive.map(|(value, _)| (value, *is_exclusive)),
        Some(value) => Some((bound_value(value)?, true)),
        None => None,
    };

    Ok(match (inclusive, exclusive) {
        (None, bound) | (bound, None) => bound,
        (Some(inclusive), Some(exclusive)) => {
            let ordering = exclusive.0.total_cmp(&inclusive.0);
            let exclusive_is_stricter = if is_lower {
                ordering.is_gt()
            } else {
                ordering.is_lt()
            };
            if exclusive_is_stricter || (ordering.is_eq() && exclusive.1 && !inclusive.1) {
                Some(exclusive)
            } else {
                Some(inclusive)
            }
        }
    })
}

fn check_numeric_bounds(
    path: &str,
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
    check_backward: bool,
) -> Vec<CompatibilityDiagnostic> {
    let mut diagnostics = Vec::new();
    for (inclusive_key, exclusive_key, is_lower) in [
        ("minimum", "exclusiveMinimum", true),
        ("maximum", "exclusiveMaximum", false),
    ] {
        if !old_schema.contains_key(inclusive_key)
            && !old_schema.contains_key(exclusive_key)
            && !new_schema.contains_key(inclusive_key)
            && !new_schema.contains_key(exclusive_key)
        {
            continue;
        }

        if (old_schema.get(exclusive_key).is_some_and(Value::is_boolean)
            || new_schema.get(exclusive_key).is_some_and(Value::is_boolean))
            && (old_schema.get(inclusive_key) != new_schema.get(inclusive_key)
                || old_schema.get(exclusive_key) != new_schema.get(exclusive_key))
        {
            diagnostics.push(CompatibilityDiagnostic::new(
                path,
                CompatibilityFinding::NotProvable,
                format!(
                    "changes Draft-04 boolean '{exclusive_key}' constraint; dialect semantics \
                     cannot be inferred at this node"
                ),
            ));
            continue;
        }

        let old_bound = effective_numeric_bound(old_schema, inclusive_key, exclusive_key, is_lower);
        let new_bound = effective_numeric_bound(new_schema, inclusive_key, exclusive_key, is_lower);
        let (Ok(old_bound), Ok(new_bound)) = (old_bound, new_bound) else {
            if old_schema.get(inclusive_key) != new_schema.get(inclusive_key)
                || old_schema.get(exclusive_key) != new_schema.get(exclusive_key)
            {
                diagnostics.push(CompatibilityDiagnostic::new(
                    path,
                    CompatibilityFinding::NotProvable,
                    format!("changes non-numeric '{inclusive_key}'/'{exclusive_key}' constraints"),
                ));
            }
            continue;
        };

        let (source, target) = if check_backward {
            (old_bound, new_bound)
        } else {
            (new_bound, old_bound)
        };
        let included = match (source, target) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(source), Some(target)) if is_lower => {
                let ordering = source.0.total_cmp(&target.0);
                ordering.is_gt() || (ordering.is_eq() && (!target.1 || source.1))
            }
            (Some(source), Some(target)) => {
                let ordering = source.0.total_cmp(&target.0);
                ordering.is_lt() || (ordering.is_eq() && (!target.1 || source.1))
            }
        };
        if !included {
            diagnostics.push(CompatibilityDiagnostic::new(
                path,
                CompatibilityFinding::BoundChanged,
                format!("changes effective {inclusive_key}/{exclusive_key} bound incompatibly"),
            ));
        }
    }
    diagnostics
}

fn check_constraint_compatibility(
    path: &str,
    old_prop_schema: &Map<String, Value>,
    new_prop_schema: &Map<String, Value>,
    check_tightening: bool,
) -> Vec<CompatibilityDiagnostic> {
    // Every pair is checked whenever either definition carries it, never
    // gated on `type`. Gating on the old schema's `type` missed a real
    // narrowing whenever `type` was absent or written as an array, which
    // reported such a change as fully compatible - the one direction of
    // error a registry cannot tolerate.
    const BOUNDS: &[(&str, &str)] = &[
        ("minLength", "maxLength"),
        ("minItems", "maxItems"),
        ("minProperties", "maxProperties"),
        ("minContains", "maxContains"),
    ];

    let mut diagnostics =
        check_numeric_bounds(path, old_prop_schema, new_prop_schema, check_tightening);
    diagnostics.extend(
        BOUNDS
            .iter()
            .filter(|(min_key, max_key)| {
                [min_key, max_key].iter().any(|key| {
                    old_prop_schema.contains_key(**key) || new_prop_schema.contains_key(**key)
                })
            })
            .flat_map(|(min_key, max_key)| {
                check_min_max_constraint(
                    path,
                    old_prop_schema,
                    new_prop_schema,
                    min_key,
                    max_key,
                    check_tightening,
                )
            }),
    );
    diagnostics
}

/// Handles keywords that only ever narrow `Valid(S)` when present.
///
/// Whether two different values of such a keyword include one another is
/// undecidable in general - no implementation can compare two regexes - but
/// presence alone is decidable: adding the constraint narrows the accepted
/// set, removing it widens it. That is exactly the shape of the "Relaxing /
/// Tightening constraints" rows of gts-spec sec 4.5, so reporting both
/// directions as incompatible (as plain equality does) contradicts the table
/// for the common case of adding or dropping one of these keywords.
fn check_narrowing_constraints(
    path: &str,
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
    check_backward: bool,
) -> Vec<CompatibilityDiagnostic> {
    const NARROWING: &[&str] = &["pattern", "format", "multipleOf"];

    let mut errors: Vec<CompatibilityDiagnostic> = NARROWING
        .iter()
        .filter_map(|keyword| {
            let old_value = old_schema.get(*keyword);
            let new_value = new_schema.get(*keyword);
            match (old_value, new_value) {
                // `multipleOf` is a number, so the two spellings of one
                // mathematical value are not a change.
                (Some(old_value), Some(new_value)) if json_values_equal(old_value, new_value) => {
                    None
                }
                _ if old_value == new_value => None,
                // Added: narrows, so forward-only.
                (None, Some(_)) if check_backward => Some(CompatibilityDiagnostic::new(
                    path,
                    CompatibilityFinding::NarrowingConstraintChanged,
                    format!("adds '{keyword}' constraint"),
                )),
                // Removed: widens, so backward-only.
                (Some(_), None) if !check_backward => Some(CompatibilityDiagnostic::new(
                    path,
                    CompatibilityFinding::NarrowingConstraintChanged,
                    format!("removes '{keyword}' constraint"),
                )),
                // Changed: inclusion between the two values is undecidable.
                (Some(old_value), Some(new_value)) => Some(CompatibilityDiagnostic::new(
                    path,
                    CompatibilityFinding::NotProvable,
                    format!(
                        "changes '{keyword}' from {old_value} to {new_value}; inclusion \
                         between the two cannot be proven"
                    ),
                )),
                // Added in the forward direction, or removed in the
                // backward one: the change widens what this direction
                // requires, so it is permitted.
                (None, Some(_) | None) | (Some(_), None) => None,
            }
        })
        .collect();

    // `uniqueItems` defaults to false, so its presence is not what matters:
    // false -> true narrows and true -> false widens, both decidable.
    let unique_items = |schema: &Map<String, Value>| {
        schema
            .get("uniqueItems")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let old_unique = unique_items(old_schema);
    let new_unique = unique_items(new_schema);
    if old_unique != new_unique && check_backward == new_unique {
        errors.push(CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::NarrowingConstraintChanged,
            format!(
                "{} 'uniqueItems'",
                if new_unique { "enables" } else { "disables" }
            ),
        ));
    }

    errors
}

fn check_type_compatibility(
    path: &str,
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
    check_backward: bool,
) -> Vec<CompatibilityDiagnostic> {
    // `type` is a set of permitted primitive types. When it is absent,
    // `const` and `enum` can still imply a finite set of effective types.
    // Inclusion of the accepted-instance sets therefore follows inclusion of
    // the type sets, which makes member order irrelevant and makes dropping
    // a member - say the `null` of an `Option<T>` - a narrowing rather than
    // an unrelated change.
    enum TypeSet {
        Any,
        Set(Vec<String>),
        Invalid,
    }

    fn value_type(value: &Value) -> &'static str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            // JSON Schema's `integer` matches a number with a zero
            // fractional part, so `1.0` is an integer. The test must be
            // exact: a tolerance would also swallow tiny nonzero fractions
            // such as `1e-20`, which no `integer` schema accepts.
            Value::Number(number)
                if number.is_i64()
                    || number.is_u64()
                    || number.as_f64().is_some_and(|value| value.fract() == 0.0) =>
            {
                "integer"
            }
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    fn type_set(schema: &Map<String, Value>) -> TypeSet {
        match schema.get("type") {
            Some(Value::String(name)) => TypeSet::Set(vec![name.clone()]),
            Some(Value::Array(names)) => names
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<&str>>>()
                .map_or(TypeSet::Invalid, |names| {
                    TypeSet::Set(names.into_iter().map(str::to_owned).collect())
                }),
            Some(_) => TypeSet::Invalid,
            None => {
                // With no `type`, the effective types are those of the
                // values `const` and `enum` accept between them.
                let values = accepted_value_set(schema);
                values.map_or(TypeSet::Any, |values| {
                    let mut names = Vec::new();
                    for value in &values {
                        let name = value_type(value).to_owned();
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                    TypeSet::Set(names)
                })
            }
        }
    }

    let old_type = old_schema.get("type");
    let new_type = new_schema.get("type");
    let (source_schema, target_schema) = if check_backward {
        (old_schema, new_schema)
    } else {
        (new_schema, old_schema)
    };

    let compatible = match (type_set(source_schema), type_set(target_schema)) {
        // A malformed `type` cannot be interpreted; fall back to equality.
        (TypeSet::Invalid, _) | (_, TypeSet::Invalid) => old_type == new_type,
        // An unconstrained target accepts every type the source permits.
        (_, TypeSet::Any) => true,
        // An unconstrained source permits types the target may not.
        (TypeSet::Any, TypeSet::Set(_)) => false,
        (TypeSet::Set(source_names), TypeSet::Set(target_names)) => {
            source_names.iter().all(|name| {
                target_names.contains(name)
                    || (name == "integer" && target_names.iter().any(|target| target == "number"))
            })
        }
    };

    if compatible {
        Vec::new()
    } else {
        vec![CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::TypeChanged,
            format!(
                "changes type incompatibly from {} to {}",
                old_type.map_or_else(|| "any".to_owned(), Value::to_string),
                new_type.map_or_else(|| "any".to_owned(), Value::to_string),
            ),
        )]
    }
}

/// The finite set of instances a level accepts through `const` and `enum`,
/// or `None` when neither keyword constrains it.
///
/// An instance must satisfy every keyword present, so two coexisting
/// keywords accept their intersection - possibly nothing at all.
fn accepted_value_set(schema: &Map<String, Value>) -> Option<Vec<Value>> {
    // A non-array `enum` is not a valid constraint and nothing can be read
    // from it, which is what `as_array` returning `None` expresses here.
    let enumeration = schema.get("enum").and_then(Value::as_array);
    match (schema.get("const"), enumeration) {
        (None, None) => None,
        (Some(constant), None) => Some(vec![constant.clone()]),
        (None, Some(values)) => Some(values.clone()),
        (Some(constant), Some(values)) => Some(
            values
                .iter()
                .filter(|value| json_values_equal(value, constant))
                .cloned()
                .collect(),
        ),
    }
}

/// Proves inclusion by validating a finite accepted-value set.
///
/// A node that enumerates its instances through `const`/`enum` is included
/// in the target exactly when every enumerated value validates against the
/// target schema. Keyword comparison cannot see this: it reads a target
/// `minimum` that the enumerating side simply does not restate as a bound
/// that was widened, and reports a narrowing as a break.
///
/// Only the positive answer is conclusive. A value that fails may still be
/// excluded by another constraint on the same node, so a caller that gets
/// `false` must fall back to comparing keywords.
fn enumerated_source_is_included(source: &Map<String, Value>, target: &Value) -> bool {
    let Some(values) = accepted_value_set(source) else {
        return false;
    };
    let Ok(validator) = jsonschema::validator_for(target) else {
        return false;
    };
    values.iter().all(|value| validator.is_valid(value))
}

/// Compares the value sets `const` and `enum` impose, as one set.
///
/// Both keywords restrict which concrete instances are accepted, so a
/// revision that moves between the two spellings only has a meaning when
/// they are read together: checking each keyword against its own
/// counterpart would read a keyword that is merely absent as an
/// unconstrained target and report the equivalent rewrite of
/// `{"const": 1}` into `{"enum": [1]}` as incompatible in both directions.
fn check_value_set_compatibility(
    path: &str,
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
    check_backward: bool,
) -> Vec<CompatibilityDiagnostic> {
    let old_values = accepted_value_set(old_schema);
    let new_values = accepted_value_set(new_schema);
    // Backward checks Valid(old) ⊆ Valid(new); forward checks the reverse
    // inclusion. Expanding the set is therefore backward-only.
    let (source, target) = if check_backward {
        (old_values.as_deref(), new_values.as_deref())
    } else {
        (new_values.as_deref(), old_values.as_deref())
    };
    let finding = if old_schema.contains_key("enum") || new_schema.contains_key("enum") {
        CompatibilityFinding::EnumChanged
    } else {
        CompatibilityFinding::ConstraintChanged
    };

    match (source, target) {
        // An unconstrained target accepts every value the source permits.
        (_, None) => Vec::new(),
        (None, Some(_)) => vec![CompatibilityDiagnostic::new(
            path,
            finding,
            format!(
                "{} the 'const'/'enum' value constraint",
                if check_backward { "adds" } else { "removes" }
            ),
        )],
        (Some(source), Some(target)) => {
            let incompatible_values: Vec<&Value> = source
                .iter()
                .filter(|value| {
                    !target
                        .iter()
                        .any(|accepted| json_values_equal(value, accepted))
                })
                .collect();
            if incompatible_values.is_empty() {
                Vec::new()
            } else {
                vec![CompatibilityDiagnostic::new(
                    path,
                    finding,
                    format!(
                        "changes the 'const'/'enum' value set incompatibly: \
                         {incompatible_values:?}"
                    ),
                )]
            }
        }
    }
}

fn check_exact_constraints(
    path: &str,
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
) -> Vec<CompatibilityDiagnostic> {
    // Keywords whose two values cannot be ordered by inclusion, so equality
    // is the only thing that can be proven. Numeric bounds live in
    // [`check_constraint_compatibility`] and keywords that merely
    // narrow when present live in [`check_narrowing_constraints`];
    // listing either here would report both directions as incompatible and
    // contradict the "Relaxing / Tightening constraints" rows of sec 4.5.
    //
    // `patternProperties`, `unevaluatedProperties` and `propertyNames` stay
    // here on purpose: they also decide the content model in
    // [`classify_content_model`], and a level whose classification can
    // change between two definitions is not something this checker attempts
    // to reason about.
    const EXACT_CONSTRAINTS: &[&str] = &[
        "additionalItems",
        "prefixItems",
        "patternProperties",
        "unevaluatedProperties",
        "contains",
        "propertyNames",
        "dependentRequired",
        "dependentSchemas",
        "dependencies",
        "oneOf",
        "anyOf",
        "not",
        "if",
        "then",
        "else",
        "contentEncoding",
        "contentMediaType",
    ];

    EXACT_CONSTRAINTS
        .iter()
        .filter(|keyword| old_schema.get(**keyword) != new_schema.get(**keyword))
        .map(|keyword| {
            CompatibilityDiagnostic::new(
                path,
                CompatibilityFinding::ConstraintChanged,
                format!("changes '{keyword}' constraint"),
            )
        })
        .collect()
}

/// Reports a `$ref` that survived resolution.
///
/// `$defs`/`definitions` are deliberately absent from
/// [`check_exact_constraints`]: in every dialect they are containers
/// reachable only through `$ref` and never contribute to `Valid(S)` (§4.3),
/// so comparing them would reject changes that alter no accepted instance.
/// The reference itself is what carries the constraint, and
/// [`crate::store::GtsStore::is_compatible`] resolves references before
/// comparing. A `$ref` that is still present therefore means this node was
/// never resolved and nothing can be proven about its target - unless both
/// definitions name the same reference, which needs no resolution.
fn check_unresolved_ref(
    path: &str,
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
) -> Vec<CompatibilityDiagnostic> {
    let old_ref = old_schema.get("$ref").and_then(Value::as_str);
    let new_ref = new_schema.get("$ref").and_then(Value::as_str);
    if old_ref == new_ref {
        return Vec::new();
    }
    vec![CompatibilityDiagnostic::new(
        path,
        CompatibilityFinding::NotProvable,
        format!(
            "has an unresolved '$ref' ({} vs {}); resolve the reference before comparing, \
             as compatibility depends on the effective resolved schemas",
            old_ref.unwrap_or("none"),
            new_ref.unwrap_or("none"),
        ),
    )]
}

fn check_schema_node_compatibility(
    old_schema: &Value,
    new_schema: &Value,
    path: &str,
    check_backward: bool,
    dialects: DialectSupport,
    inherited_unproven: UnprovenPaths,
    errors: &mut Vec<CompatibilityDiagnostic>,
) {
    // Locations an ancestor could not prove stay unprovable here; add
    // whatever this node's own `allOf` composition leaves undecided.
    let mut unproven = inherited_unproven;
    let old_effective = if old_schema.get("allOf").is_some() {
        let (effective, paths) = flatten_effective(old_schema);
        unproven.extend(paths);
        effective
    } else {
        old_schema.clone()
    };
    let new_effective = if new_schema.get("allOf").is_some() {
        let (effective, paths) = flatten_effective(new_schema);
        unproven.extend(paths);
        effective
    } else {
        new_schema.clone()
    };

    let (source, target) = if check_backward {
        (&old_effective, &new_effective)
    } else {
        (&new_effective, &old_effective)
    };
    let source_boolean = boolean_schema_value(source);
    let target_boolean = boolean_schema_value(target);
    if source_boolean == Some(false) || target_boolean == Some(true) {
        return;
    }
    if source_boolean == Some(true) || target_boolean == Some(false) {
        errors.push(CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::ConstraintChanged,
            "changes boolean schema incompatibly".to_owned(),
        ));
        return;
    }

    let (Some(old_map), Some(new_map)) = (old_effective.as_object(), new_effective.as_object())
    else {
        if old_effective != new_effective {
            errors.push(CompatibilityDiagnostic::new(
                path,
                CompatibilityFinding::ConstraintChanged,
                "changes a schema that is not an object".to_owned(),
            ));
        }
        return;
    };
    if unproven.contains("") {
        errors.push(CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::NotProvable,
            "contains an allOf intersection that the compatibility checker cannot prove".to_owned(),
        ));
        return;
    }

    let source_map = if check_backward { old_map } else { new_map };
    if enumerated_source_is_included(source_map, target) {
        return;
    }

    errors.extend(check_type_compatibility(
        path,
        old_map,
        new_map,
        check_backward,
    ));
    errors.extend(check_value_set_compatibility(
        path,
        old_map,
        new_map,
        check_backward,
    ));
    errors.extend(check_exact_constraints(path, old_map, new_map));
    errors.extend(check_unresolved_ref(path, old_map, new_map));
    errors.extend(check_narrowing_constraints(
        path,
        old_map,
        new_map,
        check_backward,
    ));
    errors.extend(check_constraint_compatibility(
        path,
        old_map,
        new_map,
        check_backward,
    ));

    let is_object_schema = |schema: &Map<String, Value>| {
        schema.get("type").and_then(Value::as_str) == Some("object")
            || schema.contains_key("properties")
            || schema.contains_key("required")
            || schema.contains_key("additionalProperties")
            || schema.contains_key("unevaluatedProperties")
            || schema.contains_key("patternProperties")
            || schema.contains_key("propertyNames")
    };
    if is_object_schema(old_map) || is_object_schema(new_map) {
        check_object_compatibility(
            old_map,
            new_map,
            path,
            check_backward,
            dialects,
            &unproven,
            errors,
        );
    }

    match (old_map.get("items"), new_map.get("items")) {
        (Some(old_items), Some(new_items)) => check_schema_node_compatibility(
            old_items,
            new_items,
            &format!("{path}[]"),
            check_backward,
            dialects,
            unproven_below(&unproven, "[]"),
            errors,
        ),
        (None, Some(_)) if check_backward => {
            errors.push(CompatibilityDiagnostic::new(
                path,
                CompatibilityFinding::ConstraintChanged,
                "adds an array items constraint".to_owned(),
            ));
        }
        (Some(_), None) if !check_backward => errors.push(CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::ConstraintChanged,
            "removes an array items constraint".to_owned(),
        )),
        _ => {}
    }
}

fn check_object_compatibility(
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
    path: &str,
    check_backward: bool,
    dialects: DialectSupport,
    unproven: &UnprovenPaths,
    errors: &mut Vec<CompatibilityDiagnostic>,
) {
    let empty = Map::new();
    let old_props = old_schema
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let new_props = new_schema
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);

    let old_required: HashSet<&str> = old_schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let new_required: HashSet<&str> = new_schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();

    let mut required_difference: Vec<&str> = if check_backward {
        new_required.difference(&old_required).copied().collect()
    } else {
        old_required.difference(&new_required).copied().collect()
    };
    required_difference.sort_unstable();
    if !required_difference.is_empty() {
        errors.push(CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::RequiredChanged,
            format!(
                "{} required properties: {required_difference:?}",
                if check_backward { "adds" } else { "removes" }
            ),
        ));
    }

    let old_model = classify_content_model(old_schema, dialects.old_unevaluated);
    let new_model = classify_content_model(new_schema, dialects.new_unevaluated);
    let (source_model, target_model) = if check_backward {
        (old_model, new_model)
    } else {
        (new_model, old_model)
    };
    let partial_constraints_equal =
        partial_content_constraints_equal(old_schema, new_schema, dialects);
    if !content_model_is_subset(source_model, target_model) {
        errors.push(CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::ContentModelChanged,
            format!(
                "changes the content model incompatibly from {} to {}",
                old_model.label(),
                new_model.label(),
            ),
        ));
    } else if source_model == ContentModel::Partial
        && target_model == ContentModel::Partial
        && !partial_constraints_equal
    {
        errors.push(CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::NotProvable,
            "changes partially open content constraints; inclusion cannot be proven".to_owned(),
        ));
    }

    for (name, old_property) in old_props {
        let property_path = if path == "$" {
            format!("$.{name}")
        } else {
            format!("{path}.{name}")
        };
        if let Some(new_property) = new_props.get(name) {
            check_schema_node_compatibility(
                old_property,
                new_property,
                &property_path,
                check_backward,
                dialects,
                unproven_below(unproven, &format!(".{name}")),
                errors,
            );
        } else if let Some(counterpart) = additional_properties_schema(new_schema) {
            // A partially open counterpart still says something about this
            // name through `additionalProperties`, so compare against that
            // schema rather than reading the property as unmatched.
            check_schema_node_compatibility(
                old_property,
                counterpart,
                &property_path,
                check_backward,
                dialects,
                UnprovenPaths::new(),
                errors,
            );
        } else {
            let incompatible_model = if check_backward {
                new_model != ContentModel::Open
            } else {
                new_model != ContentModel::Closed
            };
            if incompatible_model {
                errors.push(property_change_error(path, name, true, new_model));
            }
        }
    }

    for (name, new_property) in new_props
        .iter()
        .filter(|(name, _)| !old_props.contains_key(*name))
    {
        if let Some(counterpart) = additional_properties_schema(old_schema) {
            let property_path = if path == "$" {
                format!("$.{name}")
            } else {
                format!("{path}.{name}")
            };
            check_schema_node_compatibility(
                counterpart,
                new_property,
                &property_path,
                check_backward,
                dialects,
                UnprovenPaths::new(),
                errors,
            );
            continue;
        }
        let incompatible_model = if check_backward {
            old_model != ContentModel::Closed
        } else {
            old_model != ContentModel::Open
        };
        if incompatible_model {
            errors.push(property_change_error(path, name, false, old_model));
        }
    }
}

/// The schema an object level applies to names it does not declare, when
/// that is an actual schema rather than an open or closed boolean.
fn additional_properties_schema(schema: &Map<String, Value>) -> Option<&Value> {
    schema
        .get("additionalProperties")
        .filter(|value| boolean_schema_value(value).is_none())
}

fn classify_content_model(schema: &Map<String, Value>, supports_unevaluated: bool) -> ContentModel {
    let pattern_properties = schema
        .get("patternProperties")
        .and_then(Value::as_object)
        .filter(|patterns| !patterns.is_empty());
    let patterns_all_open = pattern_properties.is_some_and(|patterns| {
        patterns
            .values()
            .all(|constraint| boolean_schema_value(constraint) == Some(true))
    });
    let patterns_all_closed = pattern_properties.is_some_and(|patterns| {
        patterns
            .values()
            .all(|constraint| boolean_schema_value(constraint) == Some(false))
    });
    let property_names_model = schema.get("propertyNames").and_then(boolean_schema_value);
    if property_names_model == Some(false) {
        return ContentModel::Closed;
    }

    // `unevaluatedProperties` is the fallback only when this level does not
    // already evaluate unmatched names through `additionalProperties`.
    let undeclared_fallback = schema.get("additionalProperties").or_else(|| {
        supports_unevaluated
            .then(|| schema.get("unevaluatedProperties"))
            .flatten()
    });
    let fallback_model = undeclared_fallback.map_or(Some(true), boolean_schema_value);
    let constrains_property_names =
        property_names_model.is_none() && schema.contains_key("propertyNames");
    let constrains_fallback = fallback_model.is_none();

    if pattern_properties.is_some() {
        if fallback_model == Some(false) && patterns_all_closed {
            ContentModel::Closed
        } else if fallback_model == Some(true) && patterns_all_open && !constrains_property_names {
            ContentModel::Open
        } else {
            ContentModel::Partial
        }
    } else if fallback_model == Some(false) {
        ContentModel::Closed
    } else if constrains_property_names || constrains_fallback {
        ContentModel::Partial
    } else {
        ContentModel::Open
    }
}

const fn content_model_is_subset(source: ContentModel, target: ContentModel) -> bool {
    matches!(
        (source, target),
        (ContentModel::Closed, _)
            | (_, ContentModel::Open)
            | (ContentModel::Partial, ContentModel::Partial)
    )
}

fn partial_content_constraints_equal(
    old_schema: &Map<String, Value>,
    new_schema: &Map<String, Value>,
    dialects: DialectSupport,
) -> bool {
    let normalize_additional = |schema: &Map<String, Value>| {
        schema
            .get("additionalProperties")
            .cloned()
            .unwrap_or(Value::Bool(true))
    };
    let normalize_unevaluated = |schema: &Map<String, Value>, supported: bool| {
        if supported {
            schema
                .get("unevaluatedProperties")
                .cloned()
                .unwrap_or(Value::Bool(true))
        } else {
            Value::Bool(true)
        }
    };

    normalize_additional(old_schema) == normalize_additional(new_schema)
        && old_schema.get("patternProperties") == new_schema.get("patternProperties")
        && old_schema.get("propertyNames") == new_schema.get("propertyNames")
        && normalize_unevaluated(old_schema, dialects.old_unevaluated)
            == normalize_unevaluated(new_schema, dialects.new_unevaluated)
}

fn property_change_error(
    path: &str,
    property: &str,
    removed: bool,
    model: ContentModel,
) -> CompatibilityDiagnostic {
    let operation = if removed { "removes" } else { "adds" };
    if model == ContentModel::Partial {
        CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::NotProvable,
            format!(
                "{operation} property '{property}', but compatibility cannot be proven for \
                 the partially open object level"
            ),
        )
    } else {
        CompatibilityDiagnostic::new(
            path,
            if removed {
                CompatibilityFinding::PropertyRemoved
            } else {
                CompatibilityFinding::PropertyAdded
            },
            format!(
                "{operation} property '{property}' in a {} model",
                model.label()
            ),
        )
    }
}

/// Checks `Valid(old) ⊆ Valid(new)` and renders each reason as a string.
///
/// The two schemas MUST already be `$ref`-resolved; see
/// [`crate::store::GtsStore::compare_documents`], which resolves and then
/// calls this. Prefer [`check_backward_diagnostics`] when the caller
/// needs the offending schema location rather than prose.
#[must_use]
pub fn check_backward_compatibility(
    old_schema: &Value,
    new_schema: &Value,
) -> (CompatibilityVerdict, Vec<String>) {
    let (verdict, diagnostics) = check_backward_diagnostics(old_schema, new_schema);
    (verdict, render_diagnostics(&diagnostics))
}

/// Checks `Valid(new) ⊆ Valid(old)` and renders each reason as a string.
///
/// See [`check_backward_compatibility`] for the resolution
/// requirement.
#[must_use]
pub fn check_forward_compatibility(
    old_schema: &Value,
    new_schema: &Value,
) -> (CompatibilityVerdict, Vec<String>) {
    let (verdict, diagnostics) = check_forward_diagnostics(old_schema, new_schema);
    (verdict, render_diagnostics(&diagnostics))
}

/// Checks `Valid(old) ⊆ Valid(new)`, reporting each reason with its schema
/// location.
#[must_use]
pub fn check_backward_diagnostics(
    old_schema: &Value,
    new_schema: &Value,
) -> (CompatibilityVerdict, Vec<CompatibilityDiagnostic>) {
    check_inclusion(old_schema, new_schema, true)
}

/// Checks `Valid(new) ⊆ Valid(old)`, reporting each reason with its schema
/// location.
#[must_use]
pub fn check_forward_diagnostics(
    old_schema: &Value,
    new_schema: &Value,
) -> (CompatibilityVerdict, Vec<CompatibilityDiagnostic>) {
    check_inclusion(old_schema, new_schema, false)
}

/// Checks `Valid(subset) ⊆ Valid(superset)` and reports why it does not hold.
///
/// This is the primitive both compatibility relations are built on, named after
/// neither: evolution reads it as one of its modes (spec sec 4.3) while
/// derivation reads it as the single relation it has (sec 4.1, which states
/// that a derivation is never qualified by a mode name).
#[must_use]
pub fn check_accepted_set_inclusion(
    subset: &Value,
    superset: &Value,
) -> (CompatibilityVerdict, Vec<CompatibilityDiagnostic>) {
    check_inclusion(subset, superset, true)
}

/// Reports inclusion between `old_schema` and `new_schema` in the direction
/// `check_backward` selects.
fn check_inclusion(
    old_schema: &Value,
    new_schema: &Value,
    check_backward: bool,
) -> (CompatibilityVerdict, Vec<CompatibilityDiagnostic>) {
    let mut errors = Vec::new();
    let declared_old = old_schema.get("$schema").and_then(Value::as_str);
    let declared_new = new_schema.get("$schema").and_then(Value::as_str);

    // Only a genuine change of declared dialect is reported. An omitted
    // `$schema` means "whatever dialect the implementation applies" (sec 11
    // makes GTS dialect-agnostic), so it is read as the dialect the other
    // definition declares rather than as a difference - otherwise merely
    // starting to declare a dialect that was already in effect would be
    // reported as incompatible in both directions.
    if let (Some(old_dialect), Some(new_dialect)) = (declared_old, declared_new)
        && old_dialect != new_dialect
    {
        errors.push(CompatibilityDiagnostic::new(
            "$",
            CompatibilityFinding::DialectChanged,
            format!("changes JSON Schema dialect from {old_dialect} to {new_dialect}"),
        ));
    }
    let effective_old = declared_old.or(declared_new);
    let effective_new = declared_new.or(declared_old);
    check_schema_node_compatibility(
        old_schema,
        new_schema,
        "$",
        check_backward,
        DialectSupport {
            old_unevaluated: dialect_supports_unevaluated(effective_old),
            new_unevaluated: dialect_supports_unevaluated(effective_new),
        },
        UnprovenPaths::new(),
        &mut errors,
    );
    (CompatibilityVerdict::from_diagnostics(&errors), errors)
}

/// Whether `unevaluatedProperties` is evaluated under `dialect`.
///
/// The keyword exists from Draft 2019-09 on; earlier dialects ignore it as
/// an unknown annotation. An omitted `$schema` means "whatever dialect the
/// implementation applies" - GTS is dialect-agnostic (sec 11) and names no
/// default - and this implementation validates instances with
/// [`jsonschema::validator_for`], which falls back to Draft 2020-12. Reading
/// an omitted dialect as pre-2019-09 would therefore make this checker
/// contradict the validator running in the same process: a level closed by
/// `unevaluatedProperties: false` would be classified open, which reverses
/// both verdicts for an added optional property.
fn dialect_supports_unevaluated(dialect: Option<&str>) -> bool {
    dialect.is_none_or(|value| value.contains("2019-09") || value.contains("2020-12"))
}

/// Classifies the content model of every object level of a schema.
///
/// The schema MUST already be `$ref`-resolved: gts-spec §4.4 requires the
/// content model to be read from the fully resolved effective schema,
/// because `unevaluatedProperties`, `patternProperties`, `propertyNames`, a
/// nontrivial schema-valued `additionalProperties`, or a conjunctive
/// subschema reached through `allOf` or `$ref` can all decide whether
/// undeclared properties are accepted.
/// [`crate::store::GtsStore::compare_documents`] resolves before calling
/// this.
///
/// A level is reported once, at the location where it appears in the
/// document. Levels reached only through `oneOf`, `anyOf`, `not`, or
/// `if`/`then`/`else` are not reported: an instance satisfies one branch
/// rather than all of them, so such a level has no single content model.
#[must_use]
pub fn classify_object_levels(schema: &Value) -> Vec<ObjectLevel> {
    let dialect = schema.get("$schema").and_then(Value::as_str);
    let supports_unevaluated = dialect_supports_unevaluated(dialect);
    let mut levels = Vec::new();
    collect_object_levels(schema, "$", supports_unevaluated, &mut levels);
    levels
}

fn collect_object_levels(
    schema: &Value,
    path: &str,
    supports_unevaluated: bool,
    levels: &mut Vec<ObjectLevel>,
) {
    let effective = if schema.get("allOf").is_some() {
        flatten_schema(schema)
    } else {
        schema.clone()
    };
    let Some(map) = effective.as_object() else {
        return;
    };

    let declares_object = map.get("type").and_then(Value::as_str) == Some("object")
        || map.contains_key("properties")
        || map.contains_key("additionalProperties")
        || map.contains_key("unevaluatedProperties")
        || map.contains_key("patternProperties")
        || map.contains_key("propertyNames");
    if declares_object {
        levels.push(ObjectLevel {
            path: path.to_owned(),
            content_model: classify_content_model(map, supports_unevaluated),
        });
    }

    if let Some(properties) = map.get("properties").and_then(Value::as_object) {
        for (name, property) in properties {
            let property_path = if path == "$" {
                format!("$.{name}")
            } else {
                format!("{path}.{name}")
            };
            collect_object_levels(property, &property_path, supports_unevaluated, levels);
        }
    }
    if let Some(items) = map.get("items") {
        collect_object_levels(items, &format!("{path}[]"), supports_unevaluated, levels);
    }
}
/// Compares two JSON values the way JSON Schema compares instances.
///
/// `serde_json`'s `PartialEq` distinguishes the integer and float
/// representations of a number, but JSON Schema equality - the relation `const`
/// and `enum` are defined in terms of - compares numbers by mathematical
/// value, so `1` and `1.0` denote the same instance. Composites
/// compare member by member, which makes the numeric rule apply at any depth;
/// every other value type compares as `serde_json` already does.
fn json_values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => json_numbers_equal(left, right),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| json_values_equal(left, right))
        }
        // Object member order carries no meaning, so equal length plus a match
        // for every key of one side is equality.
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, left)| {
                    right
                        .get(key)
                        .is_some_and(|right| json_values_equal(left, right))
                })
        }
        _ => left == right,
    }
}

/// Compares two JSON numbers by mathematical value.
#[allow(
    clippy::float_cmp,
    reason = "JSON Schema equality is exact equality of the mathematical value"
)]
fn json_numbers_equal(left: &serde_json::Number, right: &serde_json::Number) -> bool {
    // Integers are compared as integers: routing them through `f64` would round
    // the 64-bit values a double cannot represent exactly and call two distinct
    // numbers equal.
    if let (Some(left), Some(right)) = (left.as_u64(), right.as_u64()) {
        return left == right;
    }
    if let (Some(left), Some(right)) = (left.as_i64(), right.as_i64()) {
        return left == right;
    }

    let left_integer = left.is_u64() || left.is_i64();
    let right_integer = right.is_u64() || right.is_i64();
    // Two integers that neither comparison above could pair up are one negative
    // value and one above `i64::MAX`, so they are not equal.
    if left_integer && right_integer {
        return false;
    }
    // One integer and one float. The pair is compared exactly rather than by
    // converting both sides to `f64`, which would round `2^53 + 1` down to
    // `2^53` and report two different mathematical values - two different
    // accepted-instance sets - as equal. This is the comparator `jsonschema`
    // applies to a mixed pair when it validates the same instance.
    if left_integer {
        return right
            .as_f64()
            .is_some_and(|right| integer_equals_float(left, right));
    }
    if right_integer {
        return left
            .as_f64()
            .is_some_and(|left| integer_equals_float(right, left));
    }

    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => left == right,
        // Not representable as `f64`, which needs `serde_json`'s
        // `arbitrary_precision`; the stored representation is all that is left
        // to compare.
        _ => left == right,
    }
}

/// Compares an integer-valued JSON number to a float, exactly.
fn integer_equals_float(integer: &serde_json::Number, float: f64) -> bool {
    if let Some(integer) = integer.as_u64() {
        return NumCmp::num_eq(integer, float);
    }
    integer
        .as_i64()
        .is_some_and(|integer| NumCmp::num_eq(integer, float))
}

fn render_diagnostics(diagnostics: &[CompatibilityDiagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

#[cfg(test)]
#[path = "schema_evolution_test.rs"]
mod schema_evolution_test;
