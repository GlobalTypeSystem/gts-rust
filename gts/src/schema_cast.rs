use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::gts::GtsId;
use crate::schema_evolution::{
    CompatibilityVerdict, check_backward_compatibility, check_forward_compatibility, flatten_schema,
};

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
#[must_use = "the compatibility verdict must be inspected"]
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
    /// Spec version of the implementation that *produced* this result, absent
    /// when the payload it was read from did not carry one.
    ///
    /// Deliberately not defaulted to this build's constants: a result produced
    /// by an older or foreign implementation would then silently claim our
    /// versions, destroying the provenance these two fields exist to record.
    #[serde(default)]
    pub specification_version: Option<String>,
    /// Version of the implementation that *produced* this result; see
    /// [`Self::specification_version`] for why an absent value stays absent.
    #[serde(default)]
    pub implementation_version: Option<String>,
    pub casted_entity: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The spec version to stamp on a result produced by *this* build. Wrapped in
/// `Some` at each use, so a result read from elsewhere can stay unstamped.
fn specification_version() -> String {
    crate::GTS_SPECIFICATION_VERSION.to_owned()
}

/// The implementation version to stamp on a result produced by *this* build.
fn implementation_version() -> String {
    crate::GTS_IMPLEMENTATION_VERSION.to_owned()
}

impl GtsEntityCastResult {
    /// Builds an error result for a compatibility or cast outcome that could not
    /// be decided.
    pub(crate) fn undecided(from_id: &str, to_id: &str, message: impl Into<String>) -> Self {
        Self::undecided_with_direction(from_id, to_id, "unknown", message)
    }

    /// Same as [`Self::undecided`], retaining a direction already established
    /// independently of the failed compatibility check.
    pub(crate) fn undecided_with_direction(
        from_id: &str,
        to_id: &str,
        direction: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            from_id: from_id.to_owned(),
            to_id: to_id.to_owned(),
            old: from_id.to_owned(),
            new: to_id.to_owned(),
            direction: direction.into(),
            added_properties: Vec::new(),
            removed_properties: Vec::new(),
            changed_properties: Vec::new(),
            full_compatibility: CompatibilityVerdict::Unknown,
            backward_compatibility: CompatibilityVerdict::Unknown,
            forward_compatibility: CompatibilityVerdict::Unknown,
            incompatibility_reasons: Vec::new(),
            backward_errors: Vec::new(),
            forward_errors: Vec::new(),
            specification_version: Some(specification_version()),
            implementation_version: Some(implementation_version()),
            casted_entity: None,
            error: Some(message.into()),
        }
    }

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
        let target_schema = flatten_schema(to_schema_content);

        // Determine direction by IDs
        let direction = Self::infer_direction(from_instance_id, to_type_id);

        // Both directions use the same schema order for compatibility checks
        let (old_schema, new_schema) = (from_schema_content, to_schema_content);

        // Check compatibility
        let (backward_compatibility, backward_errors) =
            check_backward_compatibility(old_schema, new_schema);
        let (forward_compatibility, forward_errors) =
            check_forward_compatibility(old_schema, new_schema);
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
                        specification_version: Some(specification_version()),
                        implementation_version: Some(implementation_version()),
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
            specification_version: Some(specification_version()),
            implementation_version: Some(implementation_version()),
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn test_undecided_result_initializes_error_contract() {
        let result = GtsEntityCastResult::undecided("old", "new", "could not decide");

        assert_eq!(result.from_id, "old");
        assert_eq!(result.to_id, "new");
        assert_eq!(result.direction, "unknown");
        assert!(result.full_compatibility.is_unknown());
        assert!(result.backward_compatibility.is_unknown());
        assert!(result.forward_compatibility.is_unknown());
        assert!(result.added_properties.is_empty());
        assert!(result.removed_properties.is_empty());
        assert!(result.changed_properties.is_empty());
        assert!(result.incompatibility_reasons.is_empty());
        assert!(result.backward_errors.is_empty());
        assert!(result.forward_errors.is_empty());
        assert_eq!(
            result.specification_version.as_deref(),
            Some(crate::GTS_SPECIFICATION_VERSION)
        );
        assert_eq!(
            result.implementation_version.as_deref(),
            Some(crate::GTS_IMPLEMENTATION_VERSION)
        );
        assert!(result.casted_entity.is_none());
        assert_eq!(result.error.as_deref(), Some("could not decide"));

        let directed =
            GtsEntityCastResult::undecided_with_direction("old", "new", "up", "resolution failed");
        assert_eq!(directed.direction, "up");
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
            specification_version: Some(specification_version()),
            implementation_version: Some(implementation_version()),
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

    /// A result read back from a payload that carries no versions must not
    /// acquire this build's, or the provenance the two fields exist to record
    /// is replaced by a claim nobody made.
    #[test]
    fn test_absent_versions_are_not_stamped_on_deserialization() {
        let mut payload = serde_json::to_value(GtsEntityCastResult::undecided(
            "gts.vendor.pkg.ns.type.v1.0",
            "gts.vendor.pkg.ns.type.v2.0",
            "produced elsewhere",
        ))
        .expect("test");
        let object = payload.as_object_mut().expect("test");
        object.remove("specification_version");
        object.remove("implementation_version");

        let foreign: GtsEntityCastResult = serde_json::from_value(payload).expect("test");
        assert_eq!(foreign.specification_version, None);
        assert_eq!(foreign.implementation_version, None);

        // A locally produced result still stamps them, so the wire format of
        // our own output is unchanged.
        let local = GtsEntityCastResult::undecided("a", "b", "produced here");
        assert_eq!(
            local.specification_version.as_deref(),
            Some(crate::GTS_SPECIFICATION_VERSION)
        );
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
}
