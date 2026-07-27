use serde_json::Value;

const NON_ASSERTION_KEYWORDS: &[&str] = &[
    "$anchor",
    "$comment",
    "$defs",
    "$dynamicAnchor",
    "$id",
    "$schema",
    "default",
    "definitions",
    "deprecated",
    "description",
    "examples",
    "readOnly",
    "title",
    "writeOnly",
];

/// Returns the boolean value of a schema when it has a directly recognizable
/// boolean-equivalent form.
///
/// JSON Schema permits boolean schemas to be written as objects. In particular,
/// `{}` is equivalent to `true`, and `{"not": {}}` is equivalent to `false`.
/// Annotation and identifier keywords do not change those equivalences.
pub fn boolean_schema_value(schema: &Value) -> Option<bool> {
    match schema {
        Value::Bool(value) => Some(*value),
        Value::Object(map) => {
            let mut assertions = map
                .iter()
                .filter(|(keyword, _)| !NON_ASSERTION_KEYWORDS.contains(&keyword.as_str()));
            let first = assertions.next();
            if assertions.next().is_some() {
                return None;
            }
            match first {
                None => Some(true),
                Some((keyword, inner)) if keyword == "not" => {
                    boolean_schema_value(inner).map(|value| !value)
                }
                Some(_) => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::boolean_schema_value;
    use serde_json::json;

    #[test]
    fn recognizes_boolean_equivalent_object_schemas() {
        assert_eq!(boolean_schema_value(&json!({})), Some(true));
        assert_eq!(
            boolean_schema_value(&json!({"description": "anything"})),
            Some(true)
        );
        assert_eq!(boolean_schema_value(&json!({"not": {}})), Some(false));
        assert_eq!(
            boolean_schema_value(&json!({"not": {"not": {}}})),
            Some(true)
        );
        assert_eq!(boolean_schema_value(&json!({"type": "string"})), None);
    }
}
