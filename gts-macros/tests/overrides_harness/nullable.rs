#![allow(clippy::unwrap_used, dead_code)]

use gts::GtsSchema;
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;
use serde_json::json;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct Address {
    #[schemars(length(max = 10))]
    pub city: String,
    pub country: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr_null.subject.v1~",
    description = "Subject with a nullable address",
    properties = "subject_type,address,payload"
)]
pub struct SubjectV1<P> {
    #[serde(rename = "type")]
    pub subject_type: gts::gts::GtsTypeId,
    pub address: Option<Address>,
    pub payload: P,
}

/// A complete replacement of `Address`: `country` is restated, not inherited.
#[derive(JsonSchema)]
pub struct ShortAddress {
    #[schemars(length(max = 3))]
    pub city: String,
    pub country: String,
}

#[derive(JsonSchema)]
pub struct NullableAddressRules {
    pub address: Option<ShortAddress>,
}

#[derive(JsonSchema)]
pub struct RequiredAddressRules {
    pub address: ShortAddress,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = "gts.x.test.ovr_null.subject.v1~x.test.ovr_null.optional.v1~",
    description = "Overridden nullable address",
    properties = "ext",
    overrides(SubjectV1 = NullableAddressRules)
)]
pub struct OptionalV1<E> {
    pub ext: E,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = "gts.x.test.ovr_null.subject.v1~x.test.ovr_null.required.v1~",
    description = "Overridden required address",
    properties = "",
    overrides(SubjectV1 = RequiredAddressRules)
)]
pub struct RequiredV1;

/// Narrows `country`, restating the city bound `OptionalV1` set.
#[derive(JsonSchema)]
pub struct CountryRules {
    #[schemars(length(max = 3))]
    pub city: String,
    #[schemars(length(min = 2, max = 2))]
    pub country: String,
}

/// Restates only the base's city bound, which `OptionalV1` narrowed.
#[derive(JsonSchema)]
pub struct WideCityRules {
    #[schemars(length(max = 10))]
    pub city: String,
    pub country: String,
}

#[derive(JsonSchema)]
pub struct WideCityAddressRules {
    pub address: Option<WideCityRules>,
}

#[derive(JsonSchema)]
pub struct CountryAddressRules {
    pub address: Option<CountryRules>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OptionalV1,
    type_id = "gts.x.test.ovr_null.subject.v1~x.test.ovr_null.optional.v1~x.test.ovr_null.country.v1~",
    description = "Narrow another field of the nullable ancestor",
    properties = "",
    overrides(SubjectV1 = CountryAddressRules)
)]
pub struct CountryV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = OptionalV1,
    type_id = "gts.x.test.ovr_null.subject.v1~x.test.ovr_null.optional.v1~x.test.ovr_null.wide_city.v1~",
    description = "Undo the intermediate city narrowing",
    properties = "",
    overrides(SubjectV1 = WideCityAddressRules)
)]
pub struct WideCityV1;

fn assert_address_overrides<T: GtsSchema>(nullable: bool, payload: &serde_json::Value) {
    let base = SubjectV1::<()>::gts_schema_with_refs_allof();
    let derived = T::gts_schema_with_refs_allof();
    gts::testing::validate_all(&[&base, &derived]).unwrap();

    // Resolve the parent so validation exercises both the inherited object
    // shape and the replacement.
    let resolved = T::gts_resolved_schema();
    let validator = jsonschema::validator_for(&resolved).unwrap();
    let mut instance = json!({
        "type": T::TYPE_ID,
        "address": { "city": "SFO", "country": "US" },
        "payload": payload
    });
    assert!(validator.is_valid(&instance));
    instance["address"]["city"] = json!("Paris");
    assert!(!validator.is_valid(&instance));
    instance["address"] = json!({ "city": "SFO" });
    assert!(!validator.is_valid(&instance));
    instance["address"] = json!(null);
    assert_eq!(validator.is_valid(&instance), nullable);
    instance.as_object_mut().unwrap().remove("address");
    assert_eq!(validator.is_valid(&instance), nullable);
}

#[test]
fn inherited_nullable_object_can_be_narrowed_while_keeping_null() {
    assert_address_overrides::<OptionalV1<()>>(true, &json!({ "ext": {} }));
}

#[test]
fn inherited_nullable_object_can_be_made_required_and_non_null() {
    assert_address_overrides::<RequiredV1>(false, &json!({}));
}

#[test]
fn later_levels_can_narrow_other_fields_of_the_nullable_ancestor() {
    gts::testing::validate_all(&[
        &SubjectV1::<()>::gts_schema_with_refs_allof(),
        &OptionalV1::<()>::gts_schema_with_refs_allof(),
        &CountryV1::gts_schema_with_refs_allof(),
    ])
    .unwrap();
    let validator = jsonschema::validator_for(&CountryV1::gts_resolved_schema()).unwrap();
    let mut instance = json!({
        "type": CountryV1::TYPE_ID,
        "address": { "city": "SFO", "country": "US" },
        "payload": { "ext": {} }
    });
    assert!(validator.is_valid(&instance));
    instance["address"]["country"] = json!("USA");
    assert!(!validator.is_valid(&instance));
    instance["address"] = json!(null);
    assert!(validator.is_valid(&instance));
}

#[test]
fn later_levels_are_checked_against_the_intermediate_override() {
    let message = WideCityV1::validate_overrides().unwrap_err().to_string();
    assert!(message.contains("field '/address'"), "{message}");
    assert!(message.contains("widens"), "{message}");
}
