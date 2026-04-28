use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonPathResolver {
    pub gts_id: String,
    pub content: Value,
    pub path: String,
    pub value: Option<Value>,
    pub resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_fields: Option<Vec<String>>,
}

impl JsonPathResolver {
    #[must_use]
    pub fn new(gts_id: String, content: Value) -> Self {
        JsonPathResolver {
            gts_id,
            content,
            path: String::new(),
            value: None,
            resolved: false,
            error: None,
            available_fields: None,
        }
    }

    fn normalize(path: &str) -> String {
        path.replace('/', ".")
    }

    fn split_raw_parts(norm: &str) -> Vec<String> {
        norm.split('.')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn parse_part(seg: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = String::new();
        let mut i = 0;
        let chars: Vec<char> = seg.chars().collect();

        while i < chars.len() {
            let ch = chars[i];
            if ch == '[' {
                if !buf.is_empty() {
                    out.push(buf.clone());
                    buf.clear();
                }
                if let Some(j) = seg[i + 1..].find(']') {
                    let j = i + 1 + j;
                    out.push(seg[i..=j].to_string());
                    i = j + 1;
                } else {
                    buf.push_str(&seg[i..]);
                    break;
                }
            } else {
                buf.push(ch);
                i += 1;
            }
        }

        if !buf.is_empty() {
            out.push(buf);
        }

        out
    }

    fn parts(path: &str) -> Vec<String> {
        let norm = Self::normalize(path);
        let raw = Self::split_raw_parts(&norm);
        let mut parts = Vec::new();

        for seg in raw {
            parts.extend(Self::parse_part(&seg));
        }

        parts
    }

    fn list_available(node: &Value, prefix: &str, out: &mut Vec<String>) {
        match node {
            Value::Object(map) => {
                for (k, v) in map {
                    let p = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    out.push(p.clone());
                    if v.is_object() || v.is_array() {
                        Self::list_available(v, &p, out);
                    }
                }
            }
            Value::Array(arr) => {
                for (i, v) in arr.iter().enumerate() {
                    let p = if prefix.is_empty() {
                        format!("[{i}]")
                    } else {
                        format!("{prefix}[{i}]")
                    };
                    out.push(p.clone());
                    if v.is_object() || v.is_array() {
                        Self::list_available(v, &p, out);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_from(node: &Value) -> Vec<String> {
        let mut acc = Vec::new();
        Self::list_available(node, "", &mut acc);
        acc
    }

    #[must_use]
    pub fn resolve(mut self, path: &str) -> Self {
        path.clone_into(&mut self.path);
        self.value = None;
        self.resolved = false;
        self.error = None;
        self.available_fields = None;

        let parts = Self::parts(path);
        let mut cur = self.content.clone();

        for p in parts {
            match &cur {
                Value::Array(arr) => {
                    let idx = if p.starts_with('[') && p.ends_with(']') {
                        let idx_str = &p[1..p.len() - 1];
                        if let Ok(i) = idx_str.parse::<usize>() {
                            i
                        } else {
                            self.error = Some(format!("Expected list index at segment '{p}'"));
                            self.available_fields = Some(Self::collect_from(&cur));
                            return self;
                        }
                    } else if let Ok(i) = p.parse::<usize>() {
                        i
                    } else {
                        self.error = Some(format!("Expected list index at segment '{p}'"));
                        self.available_fields = Some(Self::collect_from(&cur));
                        return self;
                    };

                    if idx >= arr.len() {
                        self.error = Some(format!("Index out of range at segment '{p}'"));
                        self.available_fields = Some(Self::collect_from(&cur));
                        return self;
                    }

                    cur = arr[idx].clone();
                }
                Value::Object(map) => {
                    if p.starts_with('[') && p.ends_with(']') {
                        self.error = Some(format!(
                            "Path not found at segment '{p}' in '{path}', see available fields"
                        ));
                        self.available_fields = Some(Self::collect_from(&cur));
                        return self;
                    }

                    if let Some(v) = map.get(&p) {
                        cur = v.clone();
                    } else {
                        self.error = Some(format!(
                            "Path not found at segment '{p}' in '{path}', see available fields"
                        ));
                        self.available_fields = Some(Self::collect_from(&cur));
                        return self;
                    }
                }
                _ => {
                    self.error = Some(format!("Cannot descend into {cur:?} at segment '{p}'"));
                    self.available_fields = if cur.is_object() || cur.is_array() {
                        Some(Self::collect_from(&cur))
                    } else {
                        Some(Vec::new())
                    };
                    return self;
                }
            }
        }

        self.value = Some(cur);
        self.resolved = true;
        self
    }

    #[must_use]
    pub fn failure(mut self, path: &str, error: &str) -> Self {
        path.clone_into(&mut self.path);
        self.value = None;
        self.resolved = false;
        self.error = Some(error.to_owned());
        self.available_fields = Some(Vec::new());
        self
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_resolve_simple_path() {
        let content = json!({"field": "value"});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("field");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::String("value".to_owned())));
    }

    #[test]
    fn test_resolve_nested_path() {
        let content = json!({"outer": {"inner": "value"}});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("outer.inner");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::String("value".to_owned())));
    }

    #[test]
    fn test_resolve_array_index() {
        let content = json!({"items": [1, 2, 3]});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("items[1]");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::Number(2.into())));
    }

    #[test]
    fn test_resolve_missing_path() {
        let content = json!({"field": "value"});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("missing");
        assert!(!result.resolved);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_normalize_slash_to_dot() {
        let normalized = JsonPathResolver::normalize("outer/inner/deep");
        assert_eq!(normalized, "outer.inner.deep");
    }

    #[test]
    fn test_normalize_already_dotted() {
        let normalized = JsonPathResolver::normalize("outer.inner.deep");
        assert_eq!(normalized, "outer.inner.deep");
    }

    #[test]
    fn test_split_raw_parts() {
        let parts = JsonPathResolver::split_raw_parts("a.b.c");
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_raw_parts_empty() {
        let parts = JsonPathResolver::split_raw_parts("");
        assert!(parts.is_empty());
    }

    #[test]
    fn test_split_raw_parts_trailing_dots() {
        let parts = JsonPathResolver::split_raw_parts("a..b...c");
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_part_simple() {
        let parts = JsonPathResolver::parse_part("field");
        assert_eq!(parts, vec!["field"]);
    }

    #[test]
    fn test_parse_part_with_bracket() {
        let parts = JsonPathResolver::parse_part("field[0]");
        assert_eq!(parts, vec!["field", "[0]"]);
    }

    #[test]
    fn test_parse_part_multiple_brackets() {
        let parts = JsonPathResolver::parse_part("arr[0][1]");
        assert_eq!(parts, vec!["arr", "[0]", "[1]"]);
    }

    #[test]
    fn test_parse_part_unclosed_bracket() {
        let parts = JsonPathResolver::parse_part("field[0");
        assert_eq!(parts, vec!["field", "[0"]);
    }

    #[test]
    fn test_resolve_array_bracket_notation() {
        let content = json!({"items": [1, 2, 3]});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("items.[1]");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::Number(2.into())));
    }

    #[test]
    fn test_resolve_nested_array() {
        let content = json!({"outer": {"items": [1, 2, 3]}});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("outer.items[2]");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::Number(3.into())));
    }

    #[test]
    fn test_resolve_array_out_of_bounds() {
        let content = json!({"items": [1, 2, 3]});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("items[10]");
        assert!(!result.resolved);
        assert!(result.error.as_ref().unwrap().contains("out of range"));
    }

    #[test]
    fn test_resolve_invalid_array_index() {
        let content = json!({"items": [1, 2, 3]});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("items[abc]");
        assert!(!result.resolved);
        assert!(
            result
                .error
                .as_ref()
                .unwrap()
                .contains("Expected list index")
        );
    }

    #[test]
    fn test_resolve_bracket_on_object() {
        let content = json!({"obj": {"field": "value"}});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("obj[0]");
        assert!(!result.resolved);
        assert!(result.error.as_ref().unwrap().contains("Path not found"));
    }

    #[test]
    fn test_resolve_descend_into_primitive() {
        let content = json!({"field": "value"});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("field.nested");
        assert!(!result.resolved);
        assert!(result.error.as_ref().unwrap().contains("Cannot descend"));
    }

    #[test]
    fn test_resolve_slash_notation() {
        let content = json!({"outer": {"inner": "value"}});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("outer/inner");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::String("value".to_owned())));
    }

    #[test]
    fn test_resolve_empty_path() {
        let content = json!({"field": "value"});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content.clone());
        let result = resolver.resolve("");
        assert!(result.resolved);
        assert_eq!(result.value, Some(content));
    }

    #[test]
    fn test_failure_method() {
        let content = json!({"field": "value"});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.failure("some.path", "custom error");
        assert!(!result.resolved);
        assert_eq!(result.path, "some.path");
        assert_eq!(result.error, Some("custom error".to_owned()));
        assert_eq!(result.available_fields, Some(Vec::new()));
    }

    #[test]
    fn test_list_available_complex() {
        let content = json!({
            "a": {"b": {"c": 1}},
            "x": [1, 2, {"y": "z"}]
        });
        let mut fields = Vec::new();
        JsonPathResolver::list_available(&content, "", &mut fields);
        assert!(fields.contains(&"a".to_owned()));
        assert!(fields.contains(&"a.b".to_owned()));
        assert!(fields.contains(&"a.b.c".to_owned()));
        assert!(fields.contains(&"x".to_owned()));
        assert!(fields.contains(&"x[0]".to_owned()));
        assert!(fields.contains(&"x[2].y".to_owned()));
    }

    #[test]
    fn test_collect_from() {
        let content = json!({"a": 1, "b": {"c": 2}});
        let fields = JsonPathResolver::collect_from(&content);
        assert!(fields.contains(&"a".to_owned()));
        assert!(fields.contains(&"b".to_owned()));
        assert!(fields.contains(&"b.c".to_owned()));
    }

    #[test]
    fn test_resolve_deeply_nested() {
        let content = json!({"a": {"b": {"c": {"d": {"e": "deep"}}}}});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("a.b.c.d.e");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::String("deep".to_owned())));
    }

    #[test]
    fn test_resolve_array_of_arrays() {
        let content = json!({"matrix": [[1, 2], [3, 4], [5, 6]]});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("matrix[1][0]");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::Number(3.into())));
    }

    #[test]
    fn test_resolve_mixed_path() {
        let content = json!({"data": [{"name": "first"}, {"name": "second"}]});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("data[1].name");
        assert!(result.resolved);
        assert_eq!(result.value, Some(Value::String("second".to_owned())));
    }

    #[test]
    fn test_available_fields_on_error() {
        let content = json!({"field1": "value", "field2": "other"});
        let resolver = JsonPathResolver::new("gts.test.v1~".to_owned(), content);
        let result = resolver.resolve("nonexistent");
        assert!(!result.resolved);
        assert!(result.available_fields.is_some());
        let fields = result.available_fields.unwrap();
        assert!(fields.contains(&"field1".to_owned()));
        assert!(fields.contains(&"field2".to_owned()));
    }
}
