#[cfg(test)]
mod tests {
    use gts_macros::struct_to_gts_schema;

    #[struct_to_gts_schema(
        dir_path = "test_schemas",
        schema_id = "gts.test.pretty.v1~",
        description = "Test schema for pretty formatting",
        properties = "id,name,value",
        base = true
    )]
    #[allow(dead_code)] // Test struct - fields not used, only type is needed for macro
    struct TestPrettyStructV1 {
        id: gts::GtsInstanceId,
        name: String,
        value: i32,
    }

    #[test]
    fn test_gts_schema_with_refs_as_string_pretty() {
        // Test the regular version
        let regular = TestPrettyStructV1::gts_schema_with_refs_as_string();

        // Test the pretty version
        let pretty = TestPrettyStructV1::gts_schema_with_refs_as_string_pretty();

        // Verify they both contain the same content (just different formatting)
        let regular_parsed: serde_json::Value = serde_json::from_str(&regular)
            .unwrap_or_else(|_| panic!("Failed to parse regular JSON: {}", &regular));
        let pretty_parsed: serde_json::Value = serde_json::from_str(&pretty)
            .unwrap_or_else(|_| panic!("Failed to parse pretty JSON: {}", &pretty));

        assert_eq!(
            regular_parsed, pretty_parsed,
            "Both functions should produce the same JSON structure"
        );

        // Verify pretty version is longer (due to formatting)
        assert!(
            pretty.len() > regular.len(),
            "Pretty version should be longer due to whitespace formatting"
        );

        // Verify pretty version contains newlines and indentation
        assert!(
            pretty.contains('\n'),
            "Pretty version should contain newlines"
        );
        assert!(
            pretty.contains("  "),
            "Pretty version should contain indentation"
        );

        println!("Regular version length: {}", regular.len());
        println!("Pretty version length: {}", pretty.len());
        println!("Regular: {regular}");
        println!("Pretty: {pretty}");
    }
}
