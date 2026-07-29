use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::{gts::GtsId, schema_semantics::boolean_schema_value};

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

#[derive(Debug, Error)]
pub enum SchemaCastError {
    #[error("Internal error: {0}")]
    InternalError(String),
    #[error("Target must be a schema")]
    TargetMustBeSchema,
    #[error("Source schema must be a schema")]
    SourceMustBeSchema,
    #[error("Instance must be an object for casting")]
    InstanceMustBeObject,
    #[error("{0}")]
    CastError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtsEntityCastResult {
    #[serde(rename = "from")]
    pub from_id: String,
    #[serde(rename = "to")]
    pub to_id: String,
    pub old: String,
    pub new: String,
    pub direction: String,
    pub added_properties: Vec<String>,
    pub removed_properties: Vec<String>,
    pub changed_properties: Vec<HashMap<String, String>>,
    pub full_compatibility: CompatibilityVerdict,
    pub backward_compatibility: CompatibilityVerdict,
    pub forward_compatibility: CompatibilityVerdict,
    pub incompatibility_reasons: Vec<String>,
    pub backward_errors: Vec<String>,
    pub forward_errors: Vec<String>,
    #[serde(default = "specification_version")]
    pub specification_version: String,
    #[serde(default = "implementation_version")]
    pub implementation_version: String,
    pub casted_entity: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn specification_version() -> String {
    crate::GTS_SPECIFICATION_VERSION.to_owned()
}

fn implementation_version() -> String {
    crate::GTS_IMPLEMENTATION_VERSION.to_owned()
}

/// Content model of one object level of a **resolved** effective schema.
///
/// Classified per gts-spec §4.4, which requires the level to be judged after
/// `$ref` resolution and `allOf` composition rather than from a single authored
/// keyword. Use [`GtsEntityCastResult::classify_object_levels`] to obtain the
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

const UNPROVEN_INTERSECTION: &str = "x-gts-internal-unproven-intersection";

fn merge_schema_map(target: &mut Map<String, Value>, candidate: &Map<String, Value>) {
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
                if let (Some(current_map), Some(candidate_map)) =
                    (current.as_object_mut(), candidate_value.as_object())
                {
                    for (name, candidate_schema) in candidate_map {
                        if let Some(current_schema) = current_map.get_mut(name) {
                            merge_schema_intersection(current_schema, candidate_schema);
                        } else {
                            current_map.insert(name.clone(), candidate_schema.clone());
                        }
                    }
                } else {
                    record_unproven_intersection(
                        target,
                        format!("'{keyword}' has incompatible representations"),
                    );
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
            "additionalProperties"
            | "unevaluatedProperties"
            | "items"
            | "propertyNames"
            | "contains" => merge_schema_intersection(current, candidate_value),
            "enum" => {
                if let (Some(current_values), Some(candidate_values)) =
                    (current.as_array_mut(), candidate_value.as_array())
                {
                    current_values.retain(|value| candidate_values.contains(value));
                    if current_values.is_empty() {
                        record_unproven_intersection(
                            target,
                            "allOf enum intersection is empty".to_owned(),
                        );
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
                    let reason = format!(
                        "allOf has incompatible type constraints {current} and {candidate_value}"
                    );
                    record_unproven_intersection(target, reason);
                }
            }
            _ => record_unproven_intersection(
                target,
                format!("allOf has differing '{keyword}' constraints"),
            ),
        }
    }
}

fn merge_schema_intersection(target: &mut Value, candidate: &Value) {
    match (&mut *target, candidate) {
        (Value::Bool(false), _) | (_, Value::Bool(true)) => {}
        (Value::Bool(true), value) => *target = value.clone(),
        (_, Value::Bool(false)) => *target = Value::Bool(false),
        (Value::Object(target_map), Value::Object(candidate_map)) => {
            merge_schema_map(target_map, candidate_map);
        }
        _ => {
            *target = Value::Object(Map::from_iter([(
                UNPROVEN_INTERSECTION.to_owned(),
                Value::Array(vec![target.clone(), candidate.clone()]),
            )]));
        }
    }
}

fn record_unproven_intersection(schema: &mut Map<String, Value>, reason: String) {
    let marker = schema
        .entry(UNPROVEN_INTERSECTION)
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(reasons) = marker.as_array_mut() {
        reasons.push(Value::String(reason));
    } else {
        *marker = Value::Array(vec![Value::String(reason)]);
    }
}

impl GtsEntityCastResult {
    /// Casts an instance from one schema to another.
    ///
    /// # Errors
    /// Returns `SchemaCastError` if the cast fails.
    pub fn cast(
        from_instance_id: &str,
        to_type_id: &str,
        from_instance_content: &Value,
        from_schema_content: &Value,
        to_schema_content: &Value,
        _resolver: Option<&()>,
    ) -> Result<Self, SchemaCastError> {
        // Flatten target schema to merge allOf and get all properties including const values
        let target_schema = Self::flatten_schema(to_schema_content);

        // Determine direction by IDs
        let direction = Self::infer_direction(from_instance_id, to_type_id);

        // Both directions use the same schema order for compatibility checks
        let (old_schema, new_schema) = (from_schema_content, to_schema_content);

        // Check compatibility
        let (backward_compatibility, backward_errors) =
            Self::check_backward_compatibility(old_schema, new_schema);
        let (forward_compatibility, forward_errors) =
            Self::check_forward_compatibility(old_schema, new_schema);
        let full_compatibility =
            CompatibilityVerdict::full(backward_compatibility, forward_compatibility);

        // Apply casting rules to the instance
        let instance_obj = from_instance_content
            .as_object()
            .ok_or(SchemaCastError::InstanceMustBeObject)?;

        let (casted, added, removed, incompatibility_reasons) =
            match Self::cast_instance_to_schema(instance_obj, &target_schema, "") {
                Ok(result) => result,
                Err(e) => {
                    return Ok(GtsEntityCastResult {
                        from_id: from_instance_id.to_owned(),
                        to_id: to_type_id.to_owned(),
                        old: from_instance_id.to_owned(),
                        new: to_type_id.to_owned(),
                        direction,
                        added_properties: Vec::new(),
                        removed_properties: Vec::new(),
                        changed_properties: Vec::new(),
                        full_compatibility,
                        backward_compatibility,
                        forward_compatibility,
                        incompatibility_reasons: vec![e.to_string()],
                        backward_errors,
                        forward_errors,
                        specification_version: specification_version(),
                        implementation_version: implementation_version(),
                        casted_entity: None,
                        error: None,
                    });
                }
            };

        let reasons = incompatibility_reasons;

        // TODO: Add full jsonschema validation with GTS ID tolerance

        let mut added_sorted: Vec<String> = added.into_iter().collect();
        added_sorted.sort();
        added_sorted.dedup();

        let mut removed_sorted: Vec<String> = removed.into_iter().collect();
        removed_sorted.sort();
        removed_sorted.dedup();

        Ok(GtsEntityCastResult {
            from_id: from_instance_id.to_owned(),
            to_id: to_type_id.to_owned(),
            old: from_instance_id.to_owned(),
            new: to_type_id.to_owned(),
            direction,
            added_properties: added_sorted,
            removed_properties: removed_sorted,
            changed_properties: Vec::new(),
            full_compatibility,
            backward_compatibility,
            forward_compatibility,
            incompatibility_reasons: reasons,
            backward_errors,
            forward_errors,
            specification_version: specification_version(),
            implementation_version: implementation_version(),
            casted_entity: Some(Value::Object(casted)),
            error: None,
        })
    }

    #[must_use]
    pub fn infer_direction(from_id: &str, to_id: &str) -> String {
        if let (Ok(gid_from), Ok(gid_to)) = (GtsId::try_new(from_id), GtsId::try_new(to_id))
            && let (Some(from_seg), Some(to_seg)) =
                (gid_from.segments().last(), gid_to.segments().last())
            && let (Some(from_minor), Some(to_minor)) = (from_seg.ver_minor(), to_seg.ver_minor())
        {
            if to_minor > from_minor {
                return "up".to_owned();
            }
            if to_minor < from_minor {
                return "down".to_owned();
            }
            return "none".to_owned();
        }
        "unknown".to_owned()
    }

    fn effective_object_schema(s: &Value) -> Value {
        if let Some(obj) = s.as_object() {
            if obj.contains_key("properties") || obj.contains_key("required") {
                return s.clone();
            }
            if let Some(all_of) = obj.get("allOf")
                && let Some(arr) = all_of.as_array()
            {
                for part in arr {
                    if let Some(part_obj) = part.as_object()
                        && (part_obj.contains_key("properties")
                            || part_obj.contains_key("required"))
                    {
                        return part.clone();
                    }
                }
            }
        }
        s.clone()
    }

    #[allow(
        clippy::type_complexity,
        clippy::too_many_lines,
        clippy::cognitive_complexity
    )]
    fn cast_instance_to_schema(
        instance: &Map<String, Value>,
        schema: &Value,
        base_path: &str,
    ) -> Result<(Map<String, Value>, Vec<String>, Vec<String>, Vec<String>), SchemaCastError> {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut incompatibility_reasons = Vec::new();

        let schema_obj = schema
            .as_object()
            .ok_or_else(|| SchemaCastError::CastError("Schema must be an object".to_owned()))?;

        let target_props = schema_obj
            .get("properties")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();

        let required: HashSet<String> = schema_obj
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        let additional = schema_obj
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let mut result = instance.clone();

        // 1) Ensure required properties exist (fill defaults if provided)
        for prop in &required {
            if !result.contains_key(prop)
                && let Some(p_schema) = target_props.get(prop)
                && let Some(p_obj) = p_schema.as_object()
            {
                if let Some(default) = p_obj.get("default") {
                    result.insert(prop.clone(), default.clone());
                    let path = if base_path.is_empty() {
                        prop.clone()
                    } else {
                        format!("{base_path}.{prop}")
                    };
                    added.push(path);
                } else {
                    let path = if base_path.is_empty() {
                        prop.clone()
                    } else {
                        format!("{base_path}.{prop}")
                    };
                    incompatibility_reasons.push(format!(
                        "Missing required property '{path}' and no default is defined"
                    ));
                }
            }
        }

        // 2) For optional properties with defaults, set if missing
        for (prop, p_schema) in &target_props {
            if required.contains(prop) {
                continue;
            }
            if !result.contains_key(prop)
                && let Some(p_obj) = p_schema.as_object()
                && let Some(default) = p_obj.get("default")
            {
                result.insert(prop.clone(), default.clone());
                let path = if base_path.is_empty() {
                    prop.clone()
                } else {
                    format!("{base_path}.{prop}")
                };
                added.push(path);
            }
        }

        // 2.5) Update const values to match target schema
        for (prop, p_schema) in &target_props {
            if let Some(p_obj) = p_schema.as_object()
                && let Some(const_value) = p_obj.get("const")
                && let Some(old_value) = result.get(prop)
                && let (Some(const_str), Some(old_str)) = (const_value.as_str(), old_value.as_str())
                && GtsId::is_valid(const_str)
                && GtsId::is_valid(old_str)
                && old_str != const_str
            {
                result.insert(prop.clone(), const_value.clone());
            }
        }

        // 3) Remove properties not present in target schema when additionalProperties is false
        if !additional {
            let keys: Vec<String> = result.keys().cloned().collect();
            for prop in keys {
                if !target_props.contains_key(&prop) {
                    result.remove(&prop);
                    let path = if base_path.is_empty() {
                        prop.clone()
                    } else {
                        format!("{base_path}.{prop}")
                    };
                    removed.push(path);
                }
            }
        }

        // 4) Recurse into nested object properties
        for (prop, p_schema) in &target_props {
            if let Some(val) = result.get(prop)
                && let Some(p_obj) = p_schema.as_object()
                && let Some(p_type) = p_obj.get("type").and_then(|t| t.as_str())
            {
                if p_type == "object" {
                    if let Some(val_obj) = val.as_object() {
                        let nested_schema = Self::effective_object_schema(p_schema);
                        let new_base = if base_path.is_empty() {
                            prop.clone()
                        } else {
                            format!("{base_path}.{prop}")
                        };
                        let (new_obj, add_sub, rem_sub, new_reasons) =
                            Self::cast_instance_to_schema(val_obj, &nested_schema, &new_base)?;
                        result.insert(prop.clone(), Value::Object(new_obj));
                        added.extend(add_sub);
                        removed.extend(rem_sub);
                        incompatibility_reasons.extend(new_reasons);
                    }
                } else if p_type == "array"
                    && let Some(val_arr) = val.as_array()
                    && let Some(items_schema) = p_obj.get("items")
                    && let Some(items_obj) = items_schema.as_object()
                    && items_obj.get("type").and_then(|t| t.as_str()) == Some("object")
                {
                    let nested_schema = Self::effective_object_schema(items_schema);
                    let mut new_list = Vec::new();
                    for (idx, item) in val_arr.iter().enumerate() {
                        if let Some(item_obj) = item.as_object() {
                            let new_base = if base_path.is_empty() {
                                format!("{prop}[{idx}]")
                            } else {
                                format!("{base_path}.{prop}[{idx}]")
                            };
                            let (new_item, add_sub, rem_sub, new_reasons) =
                                Self::cast_instance_to_schema(item_obj, &nested_schema, &new_base)?;
                            new_list.push(Value::Object(new_item));
                            added.extend(add_sub);
                            removed.extend(rem_sub);
                            incompatibility_reasons.extend(new_reasons);
                        } else {
                            new_list.push(item.clone());
                        }
                    }
                    result.insert(prop.clone(), Value::Array(new_list));
                }
            }
        }

        Ok((result, added, removed, incompatibility_reasons))
    }

    #[must_use]
    pub fn flatten_schema(schema: &Value) -> Value {
        let Some(schema_map) = schema.as_object() else {
            return schema.clone();
        };
        let mut result = Value::Bool(true);
        if let Some(all_of) = schema_map.get("allOf").and_then(Value::as_array) {
            for branch in all_of {
                merge_schema_intersection(&mut result, &Self::flatten_schema(branch));
            }
        }
        let direct = Value::Object(
            schema_map
                .iter()
                .filter(|(keyword, _)| keyword.as_str() != "allOf")
                .map(|(keyword, value)| (keyword.clone(), value.clone()))
                .collect(),
        );
        merge_schema_intersection(&mut result, &direct);
        result
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
        errors.extend(Self::check_non_numeric_bound(
            path, old_schema, new_schema, min_key,
        ));
        errors.extend(Self::check_non_numeric_bound(
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
        let inclusive = match schema.get(inclusive_key) {
            Some(value) => Some((value.as_f64().ok_or(())?, false)),
            None => None,
        };
        let exclusive = match schema.get(exclusive_key) {
            Some(Value::Bool(is_exclusive)) => inclusive.map(|(value, _)| (value, *is_exclusive)),
            Some(value) => Some((value.as_f64().ok_or(())?, true)),
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

            let old_bound =
                Self::effective_numeric_bound(old_schema, inclusive_key, exclusive_key, is_lower);
            let new_bound =
                Self::effective_numeric_bound(new_schema, inclusive_key, exclusive_key, is_lower);
            let (Ok(old_bound), Ok(new_bound)) = (old_bound, new_bound) else {
                if old_schema.get(inclusive_key) != new_schema.get(inclusive_key)
                    || old_schema.get(exclusive_key) != new_schema.get(exclusive_key)
                {
                    diagnostics.push(CompatibilityDiagnostic::new(
                        path,
                        CompatibilityFinding::NotProvable,
                        format!(
                            "changes non-numeric '{inclusive_key}'/'{exclusive_key}' constraints"
                        ),
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
            Self::check_numeric_bounds(path, old_prop_schema, new_prop_schema, check_tightening);
        diagnostics.extend(
            BOUNDS
                .iter()
                .filter(|(min_key, max_key)| {
                    [min_key, max_key].iter().any(|key| {
                        old_prop_schema.contains_key(**key) || new_prop_schema.contains_key(**key)
                    })
                })
                .flat_map(|(min_key, max_key)| {
                    Self::check_min_max_constraint(
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
                Value::Number(number)
                    if number.is_i64()
                        || number.is_u64()
                        || number
                            .as_f64()
                            .is_some_and(|value| value.fract().abs() < f64::EPSILON) =>
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
                    let values: Option<Vec<&Value>> = if let Some(value) = schema.get("const") {
                        Some(vec![value])
                    } else {
                        schema
                            .get("enum")
                            .and_then(Value::as_array)
                            .map(|values| values.iter().collect())
                    };
                    values.map_or(TypeSet::Any, |values| {
                        let mut names = Vec::new();
                        for value in values {
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
                        || (name == "integer"
                            && target_names.iter().any(|target| target == "number"))
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

    fn check_enum_compatibility(
        path: &str,
        old_schema: &Map<String, Value>,
        new_schema: &Map<String, Value>,
        check_backward: bool,
    ) -> Vec<CompatibilityDiagnostic> {
        let old_enum = old_schema.get("enum").and_then(Value::as_array);
        let new_enum = new_schema.get("enum").and_then(Value::as_array);

        let incompatible_values: Vec<&Value> = match (old_enum, new_enum, check_backward) {
            // Backward checks Valid(old) ⊆ Valid(new); forward checks the
            // reverse inclusion. Expanding an enum is therefore backward-only.
            (Some(old), Some(new), true) => {
                old.iter().filter(|value| !new.contains(value)).collect()
            }
            (Some(old), Some(new), false) => {
                new.iter().filter(|value| !old.contains(value)).collect()
            }
            (None, Some(_), true) | (Some(_), None, false) => {
                return vec![CompatibilityDiagnostic::new(
                    path,
                    CompatibilityFinding::EnumChanged,
                    format!(
                        "{} enum constraint",
                        if old_enum.is_some() {
                            "removes"
                        } else {
                            "adds"
                        }
                    ),
                )];
            }
            _ => Vec::new(),
        };

        if incompatible_values.is_empty() {
            Vec::new()
        } else {
            vec![CompatibilityDiagnostic::new(
                path,
                CompatibilityFinding::EnumChanged,
                format!("changes enum incompatibly: {incompatible_values:?}"),
            )]
        }
    }

    fn check_exact_constraints(
        path: &str,
        old_schema: &Map<String, Value>,
        new_schema: &Map<String, Value>,
    ) -> Vec<CompatibilityDiagnostic> {
        // Keywords whose two values cannot be ordered by inclusion, so equality
        // is the only thing that can be proven. Numeric bounds live in
        // [`Self::check_constraint_compatibility`] and keywords that merely
        // narrow when present live in [`Self::check_narrowing_constraints`];
        // listing either here would report both directions as incompatible and
        // contradict the "Relaxing / Tightening constraints" rows of sec 4.5.
        //
        // `patternProperties`, `unevaluatedProperties` and `propertyNames` stay
        // here on purpose: they also decide the content model in
        // [`Self::classify_content_model`], and a level whose classification can
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

    fn check_const_compatibility(
        path: &str,
        old_schema: &Map<String, Value>,
        new_schema: &Map<String, Value>,
        check_backward: bool,
    ) -> Vec<CompatibilityDiagnostic> {
        let old_const = old_schema.get("const");
        let new_const = new_schema.get("const");
        if old_const == new_const {
            return Vec::new();
        }

        let (source, target) = if check_backward {
            (old_const, new_const)
        } else {
            (new_const, old_const)
        };
        // A source constrained to one value is included in an unconstrained
        // target. The reverse is not; two different singleton sets are
        // disjoint and therefore incompatible in either direction.
        if source.is_some() && target.is_none() {
            return Vec::new();
        }

        vec![CompatibilityDiagnostic::new(
            path,
            CompatibilityFinding::ConstraintChanged,
            "changes 'const' constraint incompatibly".to_owned(),
        )]
    }

    /// Reports a `$ref` that survived resolution.
    ///
    /// `$defs`/`definitions` are deliberately absent from
    /// [`Self::check_exact_constraints`]: in every dialect they are containers
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
        old_supports_unevaluated: bool,
        new_supports_unevaluated: bool,
        errors: &mut Vec<CompatibilityDiagnostic>,
    ) {
        let old_effective = if old_schema.get("allOf").is_some() {
            Self::flatten_schema(old_schema)
        } else {
            old_schema.clone()
        };
        let new_effective = if new_schema.get("allOf").is_some() {
            Self::flatten_schema(new_schema)
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
        if old_map.contains_key(UNPROVEN_INTERSECTION)
            || new_map.contains_key(UNPROVEN_INTERSECTION)
        {
            errors.push(CompatibilityDiagnostic::new(
                path,
                CompatibilityFinding::NotProvable,
                "contains an allOf intersection that the compatibility checker cannot prove"
                    .to_owned(),
            ));
            return;
        }

        errors.extend(Self::check_type_compatibility(
            path,
            old_map,
            new_map,
            check_backward,
        ));
        errors.extend(Self::check_enum_compatibility(
            path,
            old_map,
            new_map,
            check_backward,
        ));
        errors.extend(Self::check_const_compatibility(
            path,
            old_map,
            new_map,
            check_backward,
        ));
        errors.extend(Self::check_exact_constraints(path, old_map, new_map));
        errors.extend(Self::check_unresolved_ref(path, old_map, new_map));
        errors.extend(Self::check_narrowing_constraints(
            path,
            old_map,
            new_map,
            check_backward,
        ));
        errors.extend(Self::check_constraint_compatibility(
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
            Self::check_object_compatibility(
                old_map,
                new_map,
                path,
                check_backward,
                old_supports_unevaluated,
                new_supports_unevaluated,
                errors,
            );
        }

        match (old_map.get("items"), new_map.get("items")) {
            (Some(old_items), Some(new_items)) => Self::check_schema_node_compatibility(
                old_items,
                new_items,
                &format!("{path}[]"),
                check_backward,
                old_supports_unevaluated,
                new_supports_unevaluated,
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
        old_supports_unevaluated: bool,
        new_supports_unevaluated: bool,
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

        let old_model = Self::classify_content_model(old_schema, old_supports_unevaluated);
        let new_model = Self::classify_content_model(new_schema, new_supports_unevaluated);
        let (source_model, target_model) = if check_backward {
            (old_model, new_model)
        } else {
            (new_model, old_model)
        };
        let partial_constraints_equal = Self::partial_content_constraints_equal(
            old_schema,
            new_schema,
            old_supports_unevaluated,
            new_supports_unevaluated,
        );
        if !Self::content_model_is_subset(source_model, target_model) {
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
                Self::check_schema_node_compatibility(
                    old_property,
                    new_property,
                    &property_path,
                    check_backward,
                    old_supports_unevaluated,
                    new_supports_unevaluated,
                    errors,
                );
            } else {
                let incompatible_model = if check_backward {
                    new_model != ContentModel::Open
                } else {
                    new_model != ContentModel::Closed
                };
                if incompatible_model {
                    errors.push(Self::property_change_error(path, name, true, new_model));
                }
            }
        }

        for name in new_props
            .keys()
            .filter(|name| !old_props.contains_key(*name))
        {
            let incompatible_model = if check_backward {
                old_model != ContentModel::Closed
            } else {
                old_model != ContentModel::Open
            };
            if incompatible_model {
                errors.push(Self::property_change_error(path, name, false, old_model));
            }
        }
    }

    fn classify_content_model(
        schema: &Map<String, Value>,
        supports_unevaluated: bool,
    ) -> ContentModel {
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
            } else if fallback_model == Some(true)
                && patterns_all_open
                && !constrains_property_names
            {
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
        old_supports_unevaluated: bool,
        new_supports_unevaluated: bool,
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
            && normalize_unevaluated(old_schema, old_supports_unevaluated)
                == normalize_unevaluated(new_schema, new_supports_unevaluated)
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
    /// calls this. Prefer [`Self::check_backward_diagnostics`] when the caller
    /// needs the offending schema location rather than prose.
    #[must_use]
    pub fn check_backward_compatibility(
        old_schema: &Value,
        new_schema: &Value,
    ) -> (CompatibilityVerdict, Vec<String>) {
        let (verdict, diagnostics) = Self::check_backward_diagnostics(old_schema, new_schema);
        (verdict, render_diagnostics(&diagnostics))
    }

    /// Checks `Valid(new) ⊆ Valid(old)` and renders each reason as a string.
    ///
    /// See [`Self::check_backward_compatibility`] for the resolution
    /// requirement.
    #[must_use]
    pub fn check_forward_compatibility(
        old_schema: &Value,
        new_schema: &Value,
    ) -> (CompatibilityVerdict, Vec<String>) {
        let (verdict, diagnostics) = Self::check_forward_diagnostics(old_schema, new_schema);
        (verdict, render_diagnostics(&diagnostics))
    }

    /// Checks `Valid(old) ⊆ Valid(new)`, reporting each reason with its schema
    /// location.
    #[must_use]
    pub fn check_backward_diagnostics(
        old_schema: &Value,
        new_schema: &Value,
    ) -> (CompatibilityVerdict, Vec<CompatibilityDiagnostic>) {
        Self::check_schema_compatibility(old_schema, new_schema, true)
    }

    /// Checks `Valid(new) ⊆ Valid(old)`, reporting each reason with its schema
    /// location.
    #[must_use]
    pub fn check_forward_diagnostics(
        old_schema: &Value,
        new_schema: &Value,
    ) -> (CompatibilityVerdict, Vec<CompatibilityDiagnostic>) {
        Self::check_schema_compatibility(old_schema, new_schema, false)
    }

    fn check_schema_compatibility(
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
        let supports_unevaluated = |dialect: Option<&str>| {
            dialect.is_some_and(|value| value.contains("2019-09") || value.contains("2020-12"))
        };
        Self::check_schema_node_compatibility(
            old_schema,
            new_schema,
            "$",
            check_backward,
            supports_unevaluated(effective_old),
            supports_unevaluated(effective_new),
            &mut errors,
        );
        (CompatibilityVerdict::from_diagnostics(&errors), errors)
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
        let supports_unevaluated =
            dialect.is_some_and(|value| value.contains("2019-09") || value.contains("2020-12"));
        let mut levels = Vec::new();
        Self::collect_object_levels(schema, "$", supports_unevaluated, &mut levels);
        levels
    }

    fn collect_object_levels(
        schema: &Value,
        path: &str,
        supports_unevaluated: bool,
        levels: &mut Vec<ObjectLevel>,
    ) {
        let effective = if schema.get("allOf").is_some() {
            Self::flatten_schema(schema)
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
                content_model: Self::classify_content_model(map, supports_unevaluated),
            });
        }

        if let Some(properties) = map.get("properties").and_then(Value::as_object) {
            for (name, property) in properties {
                let property_path = if path == "$" {
                    format!("$.{name}")
                } else {
                    format!("{path}.{name}")
                };
                Self::collect_object_levels(property, &property_path, supports_unevaluated, levels);
            }
        }
        if let Some(items) = map.get("items") {
            Self::collect_object_levels(items, &format!("{path}[]"), supports_unevaluated, levels);
        }
    }
}

fn render_diagnostics(diagnostics: &[CompatibilityDiagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper struct for compatibility results
    #[derive(Debug, Default)]
    #[allow(clippy::struct_field_names)]
    struct CompatibilityResult {
        backward_compatibility: CompatibilityVerdict,
        forward_compatibility: CompatibilityVerdict,
        full_compatibility: CompatibilityVerdict,
    }

    // Helper function to check schema compatibility
    fn check_schema_compatibility(
        old_schema: &serde_json::Value,
        new_schema: &serde_json::Value,
    ) -> CompatibilityResult {
        let (backward_compatibility, _) =
            GtsEntityCastResult::check_backward_compatibility(old_schema, new_schema);
        let (forward_compatibility, _) =
            GtsEntityCastResult::check_forward_compatibility(old_schema, new_schema);
        let full_compatibility =
            CompatibilityVerdict::full(backward_compatibility, forward_compatibility);

        CompatibilityResult {
            backward_compatibility,
            forward_compatibility,
            full_compatibility,
        }
    }

    #[test]
    fn test_schema_cast_error_display() {
        let error = SchemaCastError::InternalError("test error".to_owned());
        assert!(error.to_string().contains("test error"));

        let error = SchemaCastError::CastError("cast error".to_owned());
        assert!(error.to_string().contains("cast error"));
    }

    #[test]
    fn test_compatibility_verdict_serialization_and_full_derivation() {
        assert_eq!(
            serde_json::to_value(CompatibilityVerdict::Compatible).expect("serialize verdict"),
            json!("compatible")
        );
        assert_eq!(
            serde_json::to_value(CompatibilityVerdict::Incompatible).expect("serialize verdict"),
            json!("incompatible")
        );
        assert_eq!(
            serde_json::to_value(CompatibilityVerdict::Unknown).expect("serialize verdict"),
            json!("unknown")
        );
        assert_eq!(CompatibilityVerdict::Unknown.to_string(), "unknown");

        assert_eq!(
            CompatibilityVerdict::full(
                CompatibilityVerdict::Compatible,
                CompatibilityVerdict::Compatible
            ),
            CompatibilityVerdict::Compatible
        );
        assert_eq!(
            CompatibilityVerdict::full(
                CompatibilityVerdict::Compatible,
                CompatibilityVerdict::Unknown
            ),
            CompatibilityVerdict::Unknown
        );
        assert_eq!(
            CompatibilityVerdict::full(
                CompatibilityVerdict::Unknown,
                CompatibilityVerdict::Incompatible
            ),
            CompatibilityVerdict::Incompatible
        );
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_up() {
        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
            "gts.vendor.package.namespace.type.v1.1~abc.app.custom.event.v1.1", // v1.1 has higher minor version
        );
        assert_eq!(direction, "up");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_down() {
        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.1~abc.app.custom.event.v1.1", // v1.1 has higher minor version
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
        );
        assert_eq!(direction, "down");
    }

    #[test]
    fn test_json_entity_cast_result_infer_direction_none() {
        // Same minor version returns "none"
        let direction = GtsEntityCastResult::infer_direction(
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
            "gts.vendor.package.namespace.type.v1.0~abc.app.custom.event.v1.0",
        );
        assert_eq!(direction, "none");
    }

    #[test]
    fn test_json_entity_cast_result_serialization() {
        let result = GtsEntityCastResult {
            from_id: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            to_id: "gts.vendor.package.namespace.type.v2.0".to_owned(),
            old: "gts.vendor.package.namespace.type.v1.0".to_owned(),
            new: "gts.vendor.package.namespace.type.v2.0".to_owned(),
            direction: "up".to_owned(),
            added_properties: vec![],
            removed_properties: vec![],
            changed_properties: vec![],
            full_compatibility: CompatibilityVerdict::Incompatible,
            backward_compatibility: CompatibilityVerdict::Compatible,
            forward_compatibility: CompatibilityVerdict::Incompatible,
            incompatibility_reasons: vec![],
            backward_errors: vec![],
            forward_errors: vec![],
            specification_version: specification_version(),
            implementation_version: implementation_version(),
            casted_entity: None,
            error: None,
        };

        let json_value = serde_json::to_value(&result).expect("test");
        let json = json_value.as_object().expect("test");
        assert_eq!(
            json.get("from").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v1.0"
        );
        assert_eq!(
            json.get("to").expect("test").as_str().expect("test"),
            "gts.vendor.package.namespace.type.v2.0"
        );
        assert_eq!(
            json.get("direction").expect("test").as_str().expect("test"),
            "up"
        );
        assert_eq!(
            json.get("specification_version").and_then(Value::as_str),
            Some(crate::GTS_SPECIFICATION_VERSION)
        );
        assert_eq!(
            json.get("implementation_version").and_then(Value::as_str),
            Some(crate::GTS_IMPLEMENTATION_VERSION)
        );
    }

    #[test]
    fn test_check_schema_compatibility_identical() {
        let schema1 = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&schema1, &schema1);
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_compatible());
        assert!(result.full_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_added_optional_property() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // An open model already accepted arbitrary `email` values; declaring
        // it narrows that set.
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_added_required_property() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            },
            "required": ["name", "email"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Adding required property is not backward compatible
        assert!(result.backward_compatibility.is_incompatible());
    }

    #[test]
    fn test_check_schema_compatibility_removed_property() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_check_schema_compatibility_enum_expansion() {
        let old_schema = json!({
            "type": "string",
            "enum": ["active", "inactive"]
        });

        let new_schema = json!({
            "type": "string",
            "enum": ["active", "inactive", "pending"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_incompatible());
        assert!(result.full_compatibility.is_incompatible());
    }

    #[test]
    fn test_check_schema_compatibility_enum_reduction() {
        let old_schema = json!({
            "type": "string",
            "enum": ["active", "inactive", "pending"]
        });

        let new_schema = json!({
            "type": "string",
            "enum": ["active", "inactive"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
        assert!(result.full_compatibility.is_incompatible());
    }

    #[test]
    fn test_check_schema_compatibility_type_change() {
        let old_schema = json!({
            "type": "string"
        });

        let new_schema = json!({
            "type": "number"
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_check_schema_compatibility_constraint_tightening() {
        let old_schema = json!({
            "type": "number",
            "minimum": 0
        });

        let new_schema = json!({
            "type": "number",
            "minimum": 10
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_constraint_relaxing() {
        let old_schema = json!({
            "type": "number",
            "maximum": 100
        });

        let new_schema = json!({
            "type": "number",
            "maximum": 200
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Relaxing maximum is backward compatible
        assert!(result.backward_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_nested_objects() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            }
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "email": {"type": "string"}
                    }
                }
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_string_length_constraints() {
        let old_schema = json!({
            "type": "string",
            "minLength": 1,
            "maxLength": 100
        });

        let new_schema = json!({
            "type": "string",
            "minLength": 5,
            "maxLength": 50
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_array_length_constraints() {
        let old_schema = json!({
            "type": "array",
            "minItems": 1,
            "maxItems": 10
        });

        let new_schema = json!({
            "type": "array",
            "minItems": 2,
            "maxItems": 5
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
    }

    #[test]
    fn test_compatibility_result_default() {
        let result = CompatibilityResult::default();
        assert!(result.backward_compatibility.is_unknown());
        assert!(result.forward_compatibility.is_unknown());
        assert!(result.full_compatibility.is_unknown());
    }

    #[test]
    fn test_compatibility_result_fully_compatible() {
        let result = CompatibilityResult {
            backward_compatibility: CompatibilityVerdict::Compatible,
            forward_compatibility: CompatibilityVerdict::Compatible,
            full_compatibility: CompatibilityVerdict::Compatible,
        };
        assert!(result.full_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_enum_reordered() {
        let old_schema = json!({
            "type": "string",
            "enum": ["a", "b", "c"]
        });

        let new_schema = json!({
            "type": "string",
            "enum": ["c", "a", "b"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_compatible());
        assert!(result.full_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_nested_required_added() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    },
                    "required": ["name"]
                }
            },
            "required": ["user"]
        });

        let new_schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "email": {"type": "string"}
                    },
                    "required": ["name", "email"]
                }
            },
            "required": ["user"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Adding nested required is not backward compatible
        assert!(result.backward_compatibility.is_incompatible());
    }

    #[test]
    fn test_check_schema_compatibility_allof_flatten_equivalence() {
        let direct = json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "value": {"type": "number"}
            },
            "required": ["id"]
        });

        let via_allof = json!({
            "allOf": [
                {
                    "type": "object",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"]
                },
                {
                    "type": "object",
                    "properties": {"value": {"type": "number"}}
                }
            ]
        });

        // Either direction should be fully compatible
        let r1 = check_schema_compatibility(&direct, &via_allof);
        assert!(r1.backward_compatibility.is_compatible());
        assert!(r1.forward_compatibility.is_compatible());
        assert!(r1.full_compatibility.is_compatible());

        let r2 = check_schema_compatibility(&via_allof, &direct);
        assert!(r2.backward_compatibility.is_compatible());
        assert!(r2.forward_compatibility.is_compatible());
        assert!(r2.full_compatibility.is_compatible());
    }

    #[test]
    fn test_check_schema_compatibility_removed_required() {
        let old_schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "required": ["name"]
        });

        let new_schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string"}}
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        // Removing required is forward-incompatible
        assert!(result.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_cast_adds_defaults_and_updates_gtsid_const() {
        // Instance is missing optional 'region' and has an outdated GTS id const in 'typeRef'
        let from_instance_id = "gts.vendor.pkg.ns.type.v1.0";
        let from_instance = json!({
            "name": "alice",
            "typeRef": "gts.vendor.pkg.ns.subtype.v1.0~"
        });

        // From schema (minimal)
        let from_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "typeRef": {"type": "string"}
            }
        });

        // To schema has default for optional 'region' and const for 'typeRef' to a newer ID
        let to_type_id = "gts.vendor.pkg.ns.type.v1.1";
        let to_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "region": {"type": "string", "default": "us-east"},
                "typeRef": {"type": "string", "const": "gts.vendor.pkg.ns.subtype.v1.1~"}
            }
        });

        let cast = GtsEntityCastResult::cast(
            from_instance_id,
            to_type_id,
            &from_instance,
            &from_schema,
            &to_schema,
            None,
        )
        .expect("cast ok");

        // Defaults should be added
        assert!(cast.added_properties.iter().any(|p| p == "region"));

        let casted = cast.casted_entity.expect("casted entity");
        assert_eq!(
            casted.get("region").and_then(|v| v.as_str()),
            Some("us-east")
        );
        // typeRef should be updated to the const GTS ID
        assert_eq!(
            casted.get("typeRef").and_then(|v| v.as_str()),
            Some("gts.vendor.pkg.ns.subtype.v1.1~")
        );
    }

    #[test]
    fn test_cast_removes_additional_properties_when_disallowed() {
        let from_instance_id = "gts.vendor.pkg.ns.type.v1.0";
        let from_instance = json!({
            "name": "alice",
            "extra": 123
        });

        let from_schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string"}}
        });

        let to_type_id = "gts.vendor.pkg.ns.type.v1.1";
        let to_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"name": {"type": "string"}}
        });

        let cast = GtsEntityCastResult::cast(
            from_instance_id,
            to_type_id,
            &from_instance,
            &from_schema,
            &to_schema,
            None,
        )
        .expect("cast ok");

        // 'extra' should be removed
        let casted = cast.casted_entity.expect("casted entity");
        assert!(casted.get("extra").is_none());
        assert!(cast.removed_properties.iter().any(|p| p == "extra"));
    }

    #[test]
    fn test_closed_model_optional_addition_is_not_fully_compatible() {
        let old_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "name": {"type": "string"}
            }
        });
        let new_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_incompatible());
        assert!(result.full_compatibility.is_incompatible());
    }

    #[test]
    fn test_additional_properties_change_without_declared_properties_is_detected() {
        let old_schema = json!({"type": "object"});
        let new_schema = json!({
            "type": "object",
            "additionalProperties": false
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
        assert!(result.full_compatibility.is_incompatible());
    }

    #[test]
    fn test_required_change_without_declared_properties_is_detected() {
        let old_schema = json!({"type": "object"});
        let new_schema = json!({
            "type": "object",
            "required": ["value"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
        assert!(result.full_compatibility.is_incompatible());
    }

    #[test]
    fn test_removing_enum_constraint_is_not_fully_compatible() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "enum": ["active", "inactive"]}
            }
        });
        let new_schema = json!({
            "type": "object",
            "properties": {
                "status": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_incompatible());
        assert!(result.full_compatibility.is_incompatible());
    }

    #[test]
    fn test_adding_enum_constraint_is_forward_only() {
        let old_schema = json!({"type": "string"});
        let new_schema = json!({
            "type": "string",
            "enum": ["active", "inactive"]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
        assert!(result.full_compatibility.is_incompatible());
    }

    #[test]
    fn test_adding_and_removing_const_are_directional() {
        let added = property_change(
            json!({"type": "integer"}),
            json!({"type": "integer", "const": 1}),
        );
        assert!(added.backward_compatibility.is_incompatible());
        assert!(added.forward_compatibility.is_compatible());

        let removed = property_change(
            json!({"type": "integer", "const": 1}),
            json!({"type": "integer"}),
        );
        assert!(removed.backward_compatibility.is_compatible());
        assert!(removed.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_boolean_schemas_follow_set_inclusion() {
        let narrowed = check_schema_compatibility(&json!(true), &json!(false));
        assert!(narrowed.backward_compatibility.is_incompatible());
        assert!(narrowed.forward_compatibility.is_compatible());

        let widened = check_schema_compatibility(&json!(false), &json!(true));
        assert!(widened.backward_compatibility.is_compatible());
        assert!(widened.forward_compatibility.is_incompatible());

        // Object spellings of the boolean schemas have identical semantics.
        let equivalent = check_schema_compatibility(&json!(true), &json!({}));
        assert!(equivalent.full_compatibility.is_compatible());
    }

    #[test]
    fn test_closed_model_optional_removal_is_forward_only() {
        let old_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });
        let new_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "name": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
        assert!(result.full_compatibility.is_incompatible());
    }

    #[test]
    fn test_unevaluated_properties_closes_2020_12_object() {
        let old_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "unevaluatedProperties": false,
            "properties": {"name": {"type": "string"}}
        });
        let new_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "unevaluatedProperties": false,
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_unevaluated_properties_is_ignored_by_draft_07() {
        let old_schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "unevaluatedProperties": false,
            "properties": {"name": {"type": "string"}}
        });
        let new_schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "unevaluatedProperties": false,
            "properties": {
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
    }

    #[test]
    fn test_partial_content_model_change_is_conservative_and_names_path() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "details": {
                    "type": "object",
                    "additionalProperties": {"type": "string"}
                }
            }
        });
        let new_schema = json!({
            "type": "object",
            "properties": {
                "details": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "properties": {"count": {"type": "integer"}}
                }
            }
        });

        let (backward, backward_errors) =
            GtsEntityCastResult::check_backward_compatibility(&old_schema, &new_schema);
        let (forward, forward_errors) =
            GtsEntityCastResult::check_forward_compatibility(&old_schema, &new_schema);
        assert_eq!(backward, CompatibilityVerdict::Unknown);
        assert_eq!(forward, CompatibilityVerdict::Unknown);
        assert!(
            backward_errors
                .iter()
                .any(|error| error.contains("$.details") && error.contains("partially open"))
        );
        assert!(
            forward_errors
                .iter()
                .any(|error| error.contains("$.details") && error.contains("partially open"))
        );
    }

    #[test]
    fn test_dialect_change_is_not_proven_compatible() {
        let old_schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "string"
        });
        let new_schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "string"
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_unknown());
        assert!(result.forward_compatibility.is_unknown());
    }

    #[test]
    fn test_all_of_inherited_closure_controls_property_addition() {
        let old_schema = json!({
            "allOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {"name": {"type": "string"}}
                }
            ]
        });
        let new_schema = json!({
            "allOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "name": {"type": "string"},
                        "email": {"type": "string"}
                    }
                }
            ]
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_all_of_intersects_duplicate_property_schemas() {
        let schema = json!({
            "allOf": [
                {
                    "type": "object",
                    "properties": {
                        "value": {"type": "string", "minLength": 1}
                    }
                },
                {
                    "type": "object",
                    "properties": {
                        "value": {"type": "string", "maxLength": 10}
                    }
                }
            ]
        });

        let flattened = GtsEntityCastResult::flatten_schema(&schema);
        assert_eq!(
            flattened.pointer("/properties/value/minLength"),
            Some(&json!(1))
        );
        assert_eq!(
            flattened.pointer("/properties/value/maxLength"),
            Some(&json!(10))
        );
    }

    #[test]
    fn test_definitions_container_change_alone_is_fully_compatible() {
        // `definitions` is reachable only through `$ref` and never contributes
        // to Valid(S), so adding an entry nothing references changes nothing.
        let old_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "definitions": {
                "Used": {"type": "object", "additionalProperties": false}
            },
            "properties": {"u": {"type": "object", "additionalProperties": false}}
        });
        let new_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "definitions": {
                "Used": {"type": "object", "additionalProperties": false},
                "NeverReferenced": {"type": "string"}
            },
            "properties": {"u": {"type": "object", "additionalProperties": false}}
        });

        let result = check_schema_compatibility(&old_schema, &new_schema);
        assert!(result.full_compatibility.is_compatible());
    }

    #[test]
    fn test_resolved_nested_definition_addition_is_backward_only() {
        // The shape `resolve_schema_refs` produces for a macro-generated
        // document: the referenced level is inlined and closed, and the
        // residual `definitions` container must not double-count the change.
        let level = |extra: bool| {
            let mut props = json!({"label": {"type": "string"}});
            if extra {
                props["note"] = json!({"type": "string"});
            }
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": props,
                "required": ["label"]
            })
        };
        let document = |extra: bool| {
            json!({
                "type": "object",
                "additionalProperties": false,
                "definitions": {"Nested": level(extra)},
                "properties": {"nested": level(extra)},
                "required": ["nested"]
            })
        };

        let result = check_schema_compatibility(&document(false), &document(true));
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_differing_unresolved_ref_is_reported_as_unresolved() {
        let old_schema = json!({
            "type": "object",
            "properties": {"target": {"$ref": "gts://gts.x.core.a.b.v1~"}}
        });
        let new_schema = json!({
            "type": "object",
            "properties": {"target": {"$ref": "gts://gts.x.core.a.b.v2~"}}
        });

        let (is_backward, backward_errors) =
            GtsEntityCastResult::check_backward_compatibility(&old_schema, &new_schema);
        assert!(is_backward.is_unknown());
        assert!(
            backward_errors
                .iter()
                .any(|error| error.contains("$.target") && error.contains("unresolved '$ref'")),
            "{backward_errors:?}"
        );
    }

    #[test]
    fn test_identical_unresolved_ref_needs_no_resolution() {
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"target": {"$ref": "gts://gts.x.core.a.b.v1~"}}
        });

        let result = check_schema_compatibility(&schema, &schema);
        assert!(result.full_compatibility.is_compatible());
    }

    fn property_change(old_property: Value, new_property: Value) -> CompatibilityResult {
        let document = |property: Value| {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {"value": property},
                "required": ["value"]
            })
        };
        check_schema_compatibility(&document(old_property), &document(new_property))
    }

    /// Bound keywords must be compared whenever present, never gated on `type`.
    /// Gating on the old schema's `type` reported a real narrowing as fully
    /// compatible whenever `type` was absent or written as an array.
    #[test]
    fn test_numeric_bounds_are_checked_without_a_type_keyword() {
        let result = property_change(json!({"minimum": 0}), json!({"minimum": 5}));
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());

        let result = property_change(
            json!({"type": ["integer"], "minimum": 0}),
            json!({"type": ["integer"], "minimum": 5}),
        );
        assert!(result.backward_compatibility.is_incompatible());
        assert!(result.forward_compatibility.is_compatible());
    }

    #[test]
    fn test_exclusive_and_size_bounds_are_directional() {
        for (min_key, max_key) in [
            ("exclusiveMinimum", "exclusiveMaximum"),
            ("minProperties", "maxProperties"),
        ] {
            let relaxed = property_change(json!({max_key: 10}), json!({max_key: 100}));
            assert!(
                relaxed.backward_compatibility.is_compatible(),
                "relaxing {max_key}"
            );
            assert!(
                relaxed.forward_compatibility.is_incompatible(),
                "relaxing {max_key}"
            );

            let tightened = property_change(json!({min_key: 1}), json!({min_key: 5}));
            assert!(
                tightened.backward_compatibility.is_incompatible(),
                "tightening {min_key}"
            );
            assert!(
                tightened.forward_compatibility.is_compatible(),
                "tightening {min_key}"
            );
        }
    }

    #[test]
    fn test_inclusive_and_exclusive_bounds_are_compared_together() {
        let lower = property_change(
            json!({"type": "number", "minimum": 0}),
            json!({"type": "number", "exclusiveMinimum": 0}),
        );
        assert!(lower.backward_compatibility.is_incompatible());
        assert!(lower.forward_compatibility.is_compatible());

        let upper = property_change(
            json!({"type": "number", "maximum": 10}),
            json!({"type": "number", "exclusiveMaximum": 10}),
        );
        assert!(upper.backward_compatibility.is_incompatible());
        assert!(upper.forward_compatibility.is_compatible());
    }

    /// Draft-04 spells `exclusiveMinimum` as a boolean modifier, which a numeric
    /// comparison would silently ignore.
    #[test]
    fn test_boolean_exclusive_minimum_is_not_silently_ignored() {
        let result = property_change(
            json!({"type": "integer", "minimum": 1, "exclusiveMinimum": false}),
            json!({"type": "integer", "minimum": 1, "exclusiveMinimum": true}),
        );
        assert!(result.backward_compatibility.is_unknown());
        assert!(result.forward_compatibility.is_unknown());
    }

    #[test]
    fn test_type_is_compared_as_a_set() {
        // Dropping `null` from an `Option<T>` union narrows the accepted set.
        let narrowed = property_change(
            json!({"type": ["string", "null"]}),
            json!({"type": "string"}),
        );
        assert!(narrowed.backward_compatibility.is_incompatible());
        assert!(narrowed.forward_compatibility.is_compatible());

        // Member order carries no meaning.
        let reordered = property_change(
            json!({"type": ["string", "null"]}),
            json!({"type": ["null", "string"]}),
        );
        assert!(reordered.full_compatibility.is_compatible());

        // Widening a union accepts everything the old union did.
        let widened = property_change(
            json!({"type": "string"}),
            json!({"type": ["string", "null"]}),
        );
        assert!(widened.backward_compatibility.is_compatible());
        assert!(widened.forward_compatibility.is_incompatible());

        // `integer` remains a subset of `number` inside a union.
        let promoted = property_change(
            json!({"type": ["integer", "null"]}),
            json!({"type": ["number", "null"]}),
        );
        assert!(promoted.backward_compatibility.is_compatible());
        assert!(promoted.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_enum_and_const_imply_effective_types() {
        let enum_narrowed = property_change(json!({"type": "string"}), json!({"enum": ["a"]}));
        assert!(enum_narrowed.backward_compatibility.is_incompatible());
        assert!(enum_narrowed.forward_compatibility.is_compatible());

        let const_narrowed = property_change(json!({"type": "string"}), json!({"const": "a"}));
        assert!(const_narrowed.backward_compatibility.is_incompatible());
        assert!(const_narrowed.forward_compatibility.is_compatible());

        // JSON Schema treats mathematically integral JSON numbers as integers,
        // regardless of whether the source text contains a decimal point.
        let integral_number = property_change(json!({"type": "integer"}), json!({"const": 1.0}));
        assert!(integral_number.backward_compatibility.is_incompatible());
        assert!(integral_number.forward_compatibility.is_compatible());
    }

    #[test]
    fn test_narrowing_keyword_presence_is_directional() {
        for keyword in ["pattern", "format", "multipleOf"] {
            let value = if keyword == "multipleOf" {
                json!(5)
            } else if keyword == "format" {
                json!("date-time")
            } else {
                json!("^a+$")
            };

            let added = property_change(json!({}), json!({keyword: value.clone()}));
            assert!(
                added.backward_compatibility.is_incompatible(),
                "adding {keyword}"
            );
            assert!(
                added.forward_compatibility.is_compatible(),
                "adding {keyword}"
            );

            let removed = property_change(json!({keyword: value}), json!({}));
            assert!(
                removed.backward_compatibility.is_compatible(),
                "removing {keyword}"
            );
            assert!(
                removed.forward_compatibility.is_incompatible(),
                "removing {keyword}"
            );
        }
    }

    /// Two different regexes cannot be ordered by inclusion, so neither
    /// direction is provable - and the diagnostic must say so rather than imply
    /// the change is breaking.
    #[test]
    fn test_changed_pattern_is_reported_as_unprovable() {
        let (_, errors) = GtsEntityCastResult::check_backward_compatibility(
            &json!({"type": "string", "pattern": "^a+$"}),
            &json!({"type": "string", "pattern": "^[ab]+$"}),
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("cannot be proven")),
            "{errors:?}"
        );
    }

    #[test]
    fn test_unique_items_defaults_to_false() {
        let enabled = property_change(
            json!({"type": "array"}),
            json!({"type": "array", "uniqueItems": true}),
        );
        assert!(enabled.backward_compatibility.is_incompatible());
        assert!(enabled.forward_compatibility.is_compatible());

        let disabled = property_change(
            json!({"type": "array", "uniqueItems": true}),
            json!({"type": "array", "uniqueItems": false}),
        );
        assert!(disabled.backward_compatibility.is_compatible());
        assert!(disabled.forward_compatibility.is_incompatible());

        // Spelling out the default changes no accepted instance.
        let no_op = property_change(
            json!({"type": "array", "uniqueItems": false}),
            json!({"type": "array"}),
        );
        assert!(no_op.full_compatibility.is_compatible());
    }

    /// An omitted `$schema` means "the dialect the implementation applies", so
    /// starting to declare a dialect that was already in effect is not a change.
    #[test]
    fn test_declaring_a_previously_omitted_dialect_is_compatible() {
        let result = check_schema_compatibility(
            &json!({"type": "object", "additionalProperties": false}),
            &json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "additionalProperties": false
            }),
        );
        assert!(result.full_compatibility.is_compatible());
    }

    /// The `unevaluatedProperties` decision must follow the dialect that is in
    /// effect, including when only one definition spells it out.
    #[test]
    fn test_omitted_dialect_inherits_unevaluated_support() {
        let result = check_schema_compatibility(
            &json!({
                "type": "object",
                "unevaluatedProperties": false,
                "properties": {"name": {"type": "string"}}
            }),
            &json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "unevaluatedProperties": false,
                "properties": {
                    "name": {"type": "string"},
                    "email": {"type": "string"}
                }
            }),
        );
        assert!(result.backward_compatibility.is_compatible());
        assert!(result.forward_compatibility.is_incompatible());
    }

    #[test]
    fn test_boolean_equivalent_property_schemas_classify_semantically() {
        let additional_open = json!({
            "type": "object",
            "additionalProperties": {}
        });
        let additional_closed = json!({
            "type": "object",
            "additionalProperties": {"not": {}}
        });
        let property_names_open = json!({
            "type": "object",
            "propertyNames": {}
        });
        let property_names_closed = json!({
            "type": "object",
            "propertyNames": {"not": {}}
        });
        let closed_fallback_with_name_constraint = json!({
            "type": "object",
            "additionalProperties": {"not": {}},
            "propertyNames": {"type": "string"}
        });
        let closed_names_with_pattern = json!({
            "type": "object",
            "propertyNames": {"not": {}},
            "patternProperties": {".*": {}}
        });
        let open_pattern = json!({
            "type": "object",
            "patternProperties": {"^x-": {}}
        });
        let closed_pattern = json!({
            "type": "object",
            "additionalProperties": {"not": {}},
            "patternProperties": {"^x-": {"not": {}}}
        });
        let explicit_open_additional_precedes_unevaluated = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": {},
            "unevaluatedProperties": {"not": {}}
        });

        for (schema, expected) in [
            (additional_open, ContentModel::Open),
            (additional_closed, ContentModel::Closed),
            (property_names_open, ContentModel::Open),
            (property_names_closed, ContentModel::Closed),
            (closed_fallback_with_name_constraint, ContentModel::Closed),
            (closed_names_with_pattern, ContentModel::Closed),
            (open_pattern, ContentModel::Open),
            (closed_pattern, ContentModel::Closed),
            (
                explicit_open_additional_precedes_unevaluated,
                ContentModel::Open,
            ),
        ] {
            let levels = GtsEntityCastResult::classify_object_levels(&schema);
            assert_eq!(
                levels.first().map(|level| level.content_model),
                Some(expected)
            );
        }
    }

    #[test]
    fn test_boolean_equivalent_additional_properties_drive_compatibility() {
        let added_property = |additional_properties: Value| {
            check_schema_compatibility(
                &json!({
                    "type": "object",
                    "additionalProperties": additional_properties
                }),
                &json!({
                    "type": "object",
                    "additionalProperties": additional_properties,
                    "properties": {"name": {"type": "string"}}
                }),
            )
        };

        let open = added_property(json!({}));
        assert!(open.backward_compatibility.is_incompatible());
        assert!(open.forward_compatibility.is_compatible());

        let closed = added_property(json!({"not": {}}));
        assert!(closed.backward_compatibility.is_compatible());
        assert!(closed.forward_compatibility.is_incompatible());
    }

    /// §4.4 requires the content model to be read per object level from the
    /// resolved effective schema, and §4.4.1's closed-envelope shape puts the
    /// level that decides evolvability inside an extension container rather
    /// than at the document root.
    #[test]
    fn test_classify_object_levels_reports_every_level() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "envelope_field": {"type": "string"},
                "payload": {
                    "type": "object",
                    "properties": {
                        "own": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {"a": {"type": "string"}}
                        }
                    }
                },
                "labels": {
                    "type": "object",
                    "additionalProperties": {"type": "string"}
                },
                "closed_by_unevaluated": {
                    "type": "object",
                    "unevaluatedProperties": false,
                    "properties": {"b": {"type": "string"}}
                },
                "rows": {
                    "type": "array",
                    "items": {"type": "object", "properties": {"c": {"type": "string"}}}
                }
            }
        });

        let levels: HashMap<String, ContentModel> =
            GtsEntityCastResult::classify_object_levels(&schema)
                .into_iter()
                .map(|level| (level.path, level.content_model))
                .collect();

        assert_eq!(levels.get("$"), Some(&ContentModel::Closed));
        assert_eq!(levels.get("$.payload"), Some(&ContentModel::Open));
        assert_eq!(levels.get("$.payload.own"), Some(&ContentModel::Closed));
        assert_eq!(levels.get("$.labels"), Some(&ContentModel::Partial));
        assert_eq!(
            levels.get("$.closed_by_unevaluated"),
            Some(&ContentModel::Closed)
        );
        assert_eq!(levels.get("$.rows[]"), Some(&ContentModel::Open));
        // A scalar property is not an object level.
        assert!(!levels.contains_key("$.envelope_field"));

        // Evolvability is exactly closure.
        assert!(ContentModel::Closed.is_evolvable_in_place());
        assert!(!ContentModel::Open.is_evolvable_in_place());
        assert!(!ContentModel::Partial.is_evolvable_in_place());
    }

    /// A level closed only through `allOf` composition must classify as closed,
    /// not as the open level it looks like in isolation.
    #[test]
    fn test_classify_object_levels_uses_the_effective_schema() {
        let schema = json!({
            "allOf": [
                {"type": "object", "additionalProperties": false},
                {"type": "object", "properties": {"a": {"type": "string"}}}
            ]
        });

        let levels = GtsEntityCastResult::classify_object_levels(&schema);
        assert_eq!(
            levels.first().map(|level| level.content_model),
            Some(ContentModel::Closed)
        );
    }

    #[test]
    fn test_diagnostics_carry_the_schema_location_and_kind() {
        let old_schema = json!({
            "type": "object",
            "properties": {
                "payload": {"type": "object", "properties": {"a": {"type": "string"}}}
            }
        });
        let new_schema = json!({
            "type": "object",
            "properties": {
                "payload": {
                    "type": "object",
                    "properties": {"a": {"type": "string"}, "b": {"type": "string"}}
                }
            }
        });

        let (compatible, diagnostics) =
            GtsEntityCastResult::check_backward_diagnostics(&old_schema, &new_schema);
        assert!(compatible.is_incompatible());
        let finding = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.path == "$.payload")
            .expect("the offending level must be named, not the document root");
        assert_eq!(finding.finding, CompatibilityFinding::PropertyAdded);
        assert_eq!(
            finding.to_string(),
            "Schema at '$.payload' adds property 'b' in a open model"
        );
    }

    /// A caller that fails closed treats both alike, but an owner needs to tell
    /// "we cannot decide this" from "this is known to break".
    #[test]
    fn test_undecidable_changes_are_reported_as_not_provable() {
        let (_, diagnostics) = GtsEntityCastResult::check_backward_diagnostics(
            &json!({"type": "string", "pattern": "^a+$"}),
            &json!({"type": "string", "pattern": "^[ab]+$"}),
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.finding == CompatibilityFinding::NotProvable),
            "{diagnostics:?}"
        );
    }
}
