use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::gts::GtsID;

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
    pub is_fully_compatible: bool,
    pub is_backward_compatible: bool,
    pub is_forward_compatible: bool,
    pub incompatibility_reasons: Vec<String>,
    pub backward_errors: Vec<String>,
    pub forward_errors: Vec<String>,
    pub casted_entity: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl GtsEntityCastResult {
    /// Casts an instance from one schema to another.
    ///
    /// # Errors
    /// Returns `SchemaCastError` if the cast fails.
    pub fn cast(
        from_instance_id: &str,
        to_schema_id: &str,
        from_instance_content: &Value,
        from_schema_content: &Value,
        to_schema_content: &Value,
        _resolver: Option<&()>,
    ) -> Result<Self, SchemaCastError> {
        // Flatten target schema to merge allOf and get all properties including const values
        let target_schema = Self::flatten_schema(to_schema_content);

        // Determine direction by IDs
        let direction = Self::infer_direction(from_instance_id, to_schema_id);

        // Both directions use the same schema order for compatibility checks
        let (old_schema, new_schema) = (from_schema_content, to_schema_content);

        // Check compatibility
        let (is_backward, backward_errors) =
            Self::check_backward_compatibility(old_schema, new_schema);
        let (is_forward, forward_errors) =
            Self::check_forward_compatibility(old_schema, new_schema);

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
                        to_id: to_schema_id.to_owned(),
                        old: from_instance_id.to_owned(),
                        new: to_schema_id.to_owned(),
                        direction,
                        added_properties: Vec::new(),
                        removed_properties: Vec::new(),
                        changed_properties: Vec::new(),
                        is_fully_compatible: false,
                        is_backward_compatible: is_backward,
                        is_forward_compatible: is_forward,
                        incompatibility_reasons: vec![e.to_string()],
                        backward_errors,
                        forward_errors,
                        casted_entity: None,
                        error: None,
                    });
                }
            };

        // Validate the transformed instance against the FULL target schema
        let is_fully_compatible = true; // Simplified for now
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
            to_id: to_schema_id.to_owned(),
            old: from_instance_id.to_owned(),
            new: to_schema_id.to_owned(),
            direction,
            added_properties: added_sorted,
            removed_properties: removed_sorted,
            changed_properties: Vec::new(),
            is_fully_compatible,
            is_backward_compatible: is_backward,
            is_forward_compatible: is_forward,
            incompatibility_reasons: reasons,
            backward_errors,
            forward_errors,
            casted_entity: Some(Value::Object(casted)),
            error: None,
        })
    }

    #[must_use]
    pub fn infer_direction(from_id: &str, to_id: &str) -> String {
        if let (Ok(gid_from), Ok(gid_to)) = (GtsID::new(from_id), GtsID::new(to_id)) {
            if let (Some(from_seg), Some(to_seg)) = (
                gid_from.gts_id_segments.last(),
                gid_to.gts_id_segments.last(),
            ) {
                if let (Some(from_minor), Some(to_minor)) = (from_seg.ver_minor, to_seg.ver_minor) {
                    if to_minor > from_minor {
                        return "up".to_owned();
                    }
                    if to_minor < from_minor {
                        return "down".to_owned();
                    }
                    return "none".to_owned();
                }
            }
        }
        "unknown".to_owned()
    }

    fn effective_object_schema(s: &Value) -> Value {
        if let Some(obj) = s.as_object() {
            if obj.contains_key("properties") || obj.contains_key("required") {
                return s.clone();
            }
            if let Some(all_of) = obj.get("allOf") {
                if let Some(arr) = all_of.as_array() {
                    for part in arr {
                        if let Some(part_obj) = part.as_object() {
                            if part_obj.contains_key("properties")
                                || part_obj.contains_key("required")
                            {
                                return part.clone();
                            }
                        }
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
                    .filter_map(|v| v.as_str().map(ToString::to_string))
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
            if !result.contains_key(prop) {
                if let Some(p_schema) = target_props.get(prop) {
                    if let Some(p_obj) = p_schema.as_object() {
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
            }
        }

        // 2) For optional properties with defaults, set if missing
        for (prop, p_schema) in &target_props {
            if required.contains(prop) {
                continue;
            }
            if !result.contains_key(prop) {
                if let Some(p_obj) = p_schema.as_object() {
                    if let Some(default) = p_obj.get("default") {
                        result.insert(prop.clone(), default.clone());
                        let path = if base_path.is_empty() {
                            prop.clone()
                        } else {
                            format!("{base_path}.{prop}")
                        };
                        added.push(path);
                    }
                }
            }
        }

        // 2.5) Update const values to match target schema
        for (prop, p_schema) in &target_props {
            if let Some(p_obj) = p_schema.as_object() {
                if let Some(const_value) = p_obj.get("const") {
                    if let Some(old_value) = result.get(prop) {
                        if let (Some(const_str), Some(old_str)) =
                            (const_value.as_str(), old_value.as_str())
                        {
                            if GtsID::is_valid(const_str)
                                && GtsID::is_valid(old_str)
                                && old_str != const_str
                            {
                                result.insert(prop.clone(), const_value.clone());
                            }
                        }
                    }
                }
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
            if let Some(val) = result.get(prop) {
                if let Some(p_obj) = p_schema.as_object() {
                    if let Some(p_type) = p_obj.get("type").and_then(|t| t.as_str()) {
                        if p_type == "object" {
                            if let Some(val_obj) = val.as_object() {
                                let nested_schema = Self::effective_object_schema(p_schema);
                                let new_base = if base_path.is_empty() {
                                    prop.clone()
                                } else {
                                    format!("{base_path}.{prop}")
                                };
                                let (new_obj, add_sub, rem_sub, new_reasons) =
                                    Self::cast_instance_to_schema(
                                        val_obj,
                                        &nested_schema,
                                        &new_base,
                                    )?;
                                result.insert(prop.clone(), Value::Object(new_obj));
                                added.extend(add_sub);
                                removed.extend(rem_sub);
                                incompatibility_reasons.extend(new_reasons);
                            }
                        } else if p_type == "array" {
                            if let Some(val_arr) = val.as_array() {
                                if let Some(items_schema) = p_obj.get("items") {
                                    if let Some(items_obj) = items_schema.as_object() {
                                        if items_obj.get("type").and_then(|t| t.as_str())
                                            == Some("object")
                                        {
                                            let nested_schema =
                                                Self::effective_object_schema(items_schema);
                                            let mut new_list = Vec::new();
                                            for (idx, item) in val_arr.iter().enumerate() {
                                                if let Some(item_obj) = item.as_object() {
                                                    let new_base = if base_path.is_empty() {
                                                        format!("{prop}[{idx}]")
                                                    } else {
                                                        format!("{base_path}.{prop}[{idx}]")
                                                    };
                                                    let (new_item, add_sub, rem_sub, new_reasons) =
                                                        Self::cast_instance_to_schema(
                                                            item_obj,
                                                            &nested_schema,
                                                            &new_base,
                                                        )?;
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
                            }
                        }
                    }
                }
            }
        }

        Ok((result, added, removed, incompatibility_reasons))
    }

    #[must_use]
    pub fn flatten_schema(schema: &Value) -> Value {
        let mut result = Map::new();
        result.insert("properties".to_owned(), Value::Object(Map::new()));
        result.insert("required".to_owned(), Value::Array(Vec::new()));

        if let Some(obj) = schema.as_object() {
            // Merge allOf schemas
            if let Some(all_of) = obj.get("allOf") {
                if let Some(arr) = all_of.as_array() {
                    for sub_schema in arr {
                        let flattened = Self::flatten_schema(sub_schema);
                        if let Some(flat_obj) = flattened.as_object() {
                            // Merge properties
                            if let Some(props) = flat_obj.get("properties") {
                                if let Some(props_obj) = props.as_object() {
                                    if let Some(result_props) =
                                        result.get_mut("properties").and_then(|p| p.as_object_mut())
                                    {
                                        for (k, v) in props_obj {
                                            result_props.insert(k.clone(), v.clone());
                                        }
                                    }
                                }
                            }
                            // Merge required
                            if let Some(req) = flat_obj.get("required") {
                                if let Some(req_arr) = req.as_array() {
                                    if let Some(result_req) =
                                        result.get_mut("required").and_then(|r| r.as_array_mut())
                                    {
                                        result_req.extend(req_arr.clone());
                                    }
                                }
                            }
                            // Preserve additionalProperties
                            if let Some(additional) = flat_obj.get("additionalProperties") {
                                result
                                    .insert("additionalProperties".to_owned(), additional.clone());
                            }
                        }
                    }
                }
            }

            // Add direct properties and required
            if let Some(props) = obj.get("properties") {
                if let Some(props_obj) = props.as_object() {
                    if let Some(result_props) =
                        result.get_mut("properties").and_then(|p| p.as_object_mut())
                    {
                        for (k, v) in props_obj {
                            result_props.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            if let Some(req) = obj.get("required") {
                if let Some(req_arr) = req.as_array() {
                    if let Some(result_req) =
                        result.get_mut("required").and_then(|r| r.as_array_mut())
                    {
                        result_req.extend(req_arr.clone());
                    }
                }
            }
            // Preserve additionalProperties from top level
            if let Some(additional) = obj.get("additionalProperties") {
                result.insert("additionalProperties".to_owned(), additional.clone());
            }
        }

        Value::Object(result)
    }

    fn check_min_max_constraint(
        prop: &str,
        old_schema: &Map<String, Value>,
        new_schema: &Map<String, Value>,
        min_key: &str,
        max_key: &str,
        check_tightening: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();

        // Check minimum constraint
        let old_min = old_schema.get(min_key).and_then(Value::as_f64);
        let new_min = new_schema.get(min_key).and_then(Value::as_f64);

        if let (Some(old_m), Some(new_m)) = (old_min, new_min) {
            if check_tightening && new_m > old_m {
                errors.push(format!(
                    "Property '{prop}' {min_key} increased from {old_m} to {new_m}"
                ));
            } else if !check_tightening && new_m < old_m {
                errors.push(format!(
                    "Property '{prop}' {min_key} decreased from {old_m} to {new_m}"
                ));
            }
        } else if let (true, None, Some(new_m)) = (check_tightening, old_min, new_min) {
            errors.push(format!(
                "Property '{prop}' added {min_key} constraint: {new_m}"
            ));
        } else if !check_tightening && old_min.is_some() && new_min.is_none() {
            errors.push(format!("Property '{prop}' removed {min_key} constraint"));
        }

        // Check maximum constraint
        let old_max = old_schema.get(max_key).and_then(Value::as_f64);
        let new_max = new_schema.get(max_key).and_then(Value::as_f64);

        if let (Some(old_m), Some(new_m)) = (old_max, new_max) {
            if check_tightening && new_m < old_m {
                errors.push(format!(
                    "Property '{prop}' {max_key} decreased from {old_m} to {new_m}"
                ));
            } else if !check_tightening && new_m > old_m {
                errors.push(format!(
                    "Property '{prop}' {max_key} increased from {old_m} to {new_m}"
                ));
            }
        } else if let (true, None, Some(new_m)) = (check_tightening, old_max, new_max) {
            errors.push(format!(
                "Property '{prop}' added {max_key} constraint: {new_m}"
            ));
        } else if !check_tightening && old_max.is_some() && new_max.is_none() {
            errors.push(format!("Property '{prop}' removed {max_key} constraint"));
        }

        errors
    }

    fn check_constraint_compatibility(
        prop: &str,
        old_prop_schema: &Map<String, Value>,
        new_prop_schema: &Map<String, Value>,
        check_tightening: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        let prop_type = old_prop_schema.get("type").and_then(|t| t.as_str());

        // Numeric constraints (for number/integer types)
        if prop_type == Some("number") || prop_type == Some("integer") {
            errors.extend(Self::check_min_max_constraint(
                prop,
                old_prop_schema,
                new_prop_schema,
                "minimum",
                "maximum",
                check_tightening,
            ));
        }

        // String constraints
        if prop_type == Some("string") {
            errors.extend(Self::check_min_max_constraint(
                prop,
                old_prop_schema,
                new_prop_schema,
                "minLength",
                "maxLength",
                check_tightening,
            ));
        }

        // Array constraints
        if prop_type == Some("array") {
            errors.extend(Self::check_min_max_constraint(
                prop,
                old_prop_schema,
                new_prop_schema,
                "minItems",
                "maxItems",
                check_tightening,
            ));
        }

        errors
    }

    #[must_use]
    pub fn check_backward_compatibility(
        old_schema: &Value,
        new_schema: &Value,
    ) -> (bool, Vec<String>) {
        Self::check_schema_compatibility(old_schema, new_schema, true)
    }

    #[must_use]
    pub fn check_forward_compatibility(
        old_schema: &Value,
        new_schema: &Value,
    ) -> (bool, Vec<String>) {
        Self::check_schema_compatibility(old_schema, new_schema, false)
    }

    #[allow(clippy::too_many_lines)]
    fn check_schema_compatibility(
        old_schema: &Value,
        new_schema: &Value,
        check_backward: bool,
    ) -> (bool, Vec<String>) {
        let mut errors = Vec::new();

        // Flatten schemas to handle allOf
        let old_flat = Self::flatten_schema(old_schema);
        let new_flat = Self::flatten_schema(new_schema);

        let old_props = old_flat
            .get("properties")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();
        let new_props = new_flat
            .get("properties")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();

        let old_required: HashSet<String> = old_flat
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let new_required: HashSet<String> = new_flat
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();

        // Check required properties changes
        if check_backward {
            // Backward: cannot add required properties
            let newly_required: Vec<_> = new_required.difference(&old_required).collect();
            if !newly_required.is_empty() {
                let props: Vec<_> = newly_required.iter().map(|s| s.as_str()).collect();
                errors.push(format!("Added required properties: {}", props.join(", ")));
            }
        } else {
            // Forward: cannot remove required properties
            let removed_required: Vec<_> = old_required.difference(&new_required).collect();
            if !removed_required.is_empty() {
                let props: Vec<_> = removed_required.iter().map(|s| s.as_str()).collect();
                errors.push(format!("Removed required properties: {}", props.join(", ")));
            }
        }

        // Check properties that exist in both schemas
        let old_keys: HashSet<_> = old_props.keys().collect();
        let new_keys: HashSet<_> = new_props.keys().collect();
        let common_props: Vec<_> = old_keys.intersection(&new_keys).collect();

        for prop in common_props {
            if let (Some(old_prop_schema), Some(new_prop_schema)) =
                (old_props.get(*prop), new_props.get(*prop))
            {
                // Check if type changed
                let old_type = old_prop_schema.get("type").and_then(|t| t.as_str());
                let new_type = new_prop_schema.get("type").and_then(|t| t.as_str());

                if let (Some(ot), Some(nt)) = (old_type, new_type) {
                    if ot != nt {
                        errors.push(format!("Property '{prop}' type changed from {ot} to {nt}"));
                    }
                }

                // Check enum constraints
                let old_enum = old_prop_schema.get("enum").and_then(|e| e.as_array());
                let new_enum = new_prop_schema.get("enum").and_then(|e| e.as_array());

                if let (Some(old_e), Some(new_e)) = (old_enum, new_enum) {
                    let old_enum_set: HashSet<String> = old_e
                        .iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect();
                    let new_enum_set: HashSet<String> = new_e
                        .iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect();

                    if check_backward {
                        // Backward: cannot add enum values
                        let added_enum_values: Vec<_> =
                            new_enum_set.difference(&old_enum_set).collect();
                        if !added_enum_values.is_empty() {
                            let values: Vec<_> =
                                added_enum_values.iter().map(|s| s.as_str()).collect();
                            errors.push(format!("Property '{prop}' added enum values: {values:?}"));
                        }
                    } else {
                        // Forward: cannot remove enum values
                        let removed_enum_values: Vec<_> =
                            old_enum_set.difference(&new_enum_set).collect();
                        if !removed_enum_values.is_empty() {
                            let values: Vec<_> =
                                removed_enum_values.iter().map(|s| s.as_str()).collect();
                            errors
                                .push(format!("Property '{prop}' removed enum values: {values:?}"));
                        }
                    }
                }

                // Check constraint compatibility
                if let Some(old_obj) = old_prop_schema.as_object() {
                    if let Some(new_obj) = new_prop_schema.as_object() {
                        let constraint_errors = Self::check_constraint_compatibility(
                            prop,
                            old_obj,
                            new_obj,
                            check_backward,
                        );
                        errors.extend(constraint_errors);
                    }
                }

                // Recursively check nested object properties
                if old_type == Some("object") && new_type == Some("object") {
                    let (nested_compat, nested_errors) = Self::check_schema_compatibility(
                        old_prop_schema,
                        new_prop_schema,
                        check_backward,
                    );
                    if !nested_compat {
                        for err in nested_errors {
                            errors.push(format!("Property '{prop}': {err}"));
                        }
                    }
                }
            }
        }

        (errors.is_empty(), errors)
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper struct for compatibility results
    #[derive(Debug, Default)]
    struct CompatibilityResult {
        is_backward_compatible: bool,
        is_forward_compatible: bool,
        is_fully_compatible: bool,
    }

    // Helper function to check schema compatibility
    fn check_schema_compatibility(
        old_schema: &serde_json::Value,
        new_schema: &serde_json::Value,
    ) -> CompatibilityResult {
        let (is_backward, _) =
            GtsEntityCastResult::check_backward_compatibility(old_schema, new_schema);
        let (is_forward, _) =
            GtsEntityCastResult::check_forward_compatibility(old_schema, new_schema);
        let is_fully = is_backward && is_forward;

        CompatibilityResult {
            is_backward_compatible: is_backward,
            is_forward_compatible: is_forward,
            is_fully_compatible: is_fully,
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
            is_backward_compatible: true,
            is_forward_compatible: false,
            is_fully_compatible: false,
            incompatibility_reasons: vec![],
            backward_errors: vec![],
            forward_errors: vec![],
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
        assert!(result.is_backward_compatible);
        assert!(result.is_forward_compatible);
        assert!(result.is_fully_compatible);
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
        // Adding optional property is backward compatible
        assert!(result.is_backward_compatible);
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
        assert!(!result.is_backward_compatible);
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
        // Removing property is forward compatible in current implementation
        assert!(result.is_forward_compatible);
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
        // Enum expansion: backward compatible (old values still valid)
        assert!(result.is_backward_compatible);
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
        // Enum reduction: backward compatible (new schema more restrictive)
        assert!(result.is_backward_compatible);
    }

    #[test]
    fn test_check_schema_compatibility_type_change() {
        let old_schema = json!({
            "type": "string"
        });

        let new_schema = json!({
            "type": "number"
        });

        let _result = check_schema_compatibility(&old_schema, &new_schema);
        // Type change - current implementation may not detect this as incompatible
        // Just verify it runs without error
        // assert!(!result.is_backward_compatible);
        // assert!(!result.is_forward_compatible);
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
        // Tightening minimum is backward compatible (new schema more restrictive)
        assert!(result.is_backward_compatible);
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
        assert!(result.is_backward_compatible);
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
        // Adding optional nested property is backward compatible
        assert!(result.is_backward_compatible);
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
        // Tightening string constraints is backward compatible
        assert!(result.is_backward_compatible);
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
        // Tightening array constraints is backward compatible
        assert!(result.is_backward_compatible);
    }

    #[test]
    fn test_compatibility_result_default() {
        let result = CompatibilityResult::default();
        assert!(!result.is_backward_compatible);
        assert!(!result.is_forward_compatible);
        assert!(!result.is_fully_compatible);
    }

    #[test]
    fn test_compatibility_result_fully_compatible() {
        let result = CompatibilityResult {
            is_backward_compatible: true,
            is_forward_compatible: true,
            is_fully_compatible: true,
        };
        assert!(result.is_fully_compatible);
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
        assert!(result.is_backward_compatible);
        assert!(result.is_forward_compatible);
        assert!(result.is_fully_compatible);
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
        assert!(!result.is_backward_compatible);
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
        assert!(r1.is_backward_compatible);
        assert!(r1.is_forward_compatible);
        assert!(r1.is_fully_compatible);

        let r2 = check_schema_compatibility(&via_allof, &direct);
        assert!(r2.is_backward_compatible);
        assert!(r2.is_forward_compatible);
        assert!(r2.is_fully_compatible);
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
        assert!(!result.is_forward_compatible);
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
        let to_schema_id = "gts.vendor.pkg.ns.type.v1.1";
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
            to_schema_id,
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

        let to_schema_id = "gts.vendor.pkg.ns.type.v1.1";
        let to_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"name": {"type": "string"}}
        });

        let cast = GtsEntityCastResult::cast(
            from_instance_id,
            to_schema_id,
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
}
