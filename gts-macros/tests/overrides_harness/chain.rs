#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

//! `overrides(Ancestor = Helper)`: a derived type replaces fields its
//! ancestors declare. Chain under test:
//!
//! `SubjectV1<P>` (id, nickname, level, tags, address, payload: P)
//!   -> `RegionalSubjectV1<D>` (region, details: D)
//!     -> `EuTenantSubjectV1<E>` (tenant, ext: E) overrides `/id` and `/payload/region`
//!       -> `EuWestSubjectV1` overrides `/payload/region` again
//!
//! Invalid overrides still compile and generate schemas; explicit validation
//! returns a diagnostic naming the ancestor and the field's wire path.
//! The outer harness expects their generated validation tests to fail;
//! the tests below also assert the individual diagnostics.

use gts::{GtsSchema, GtsStore};
use gts_macros::struct_to_gts_schema;
use schemars::JsonSchema;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Chain
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct Address {
    #[schemars(length(max = 100))]
    pub city: String,
    #[schemars(length(min = 2, max = 3))]
    pub country: String,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.ovr.subject.v1~",
    description = "Subject",
    properties = "subject_type,id,nickname,level,tags,address,payload"
)]
#[derive(Debug)]
pub struct SubjectV1<P> {
    #[serde(rename = "type")]
    pub subject_type: gts::gts::GtsTypeId,
    #[schemars(length(min = 1, max = 1024), regex(pattern = r"^[\x20-\x7E]+$"))]
    pub id: String,
    #[schemars(length(max = 64))]
    pub nickname: Option<String>,
    #[schemars(range(min = 0, max = 100))]
    pub level: u32,
    #[schemars(inner(length(max = 32)))]
    pub tags: Vec<String>,
    pub address: Address,
    pub payload: P,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = SubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~",
    description = "Regional subject",
    properties = "region,details"
)]
#[derive(Debug)]
pub struct RegionalSubjectV1<D> {
    #[schemars(length(max = 32), regex(pattern = r"^[a-z]+-[a-z]+$"))]
    pub region: String,
    pub details: D,
}

/// `/id` must be a UUID; the inherited `pattern` is restated.
#[derive(JsonSchema)]
pub struct TenantIdRules {
    #[schemars(
        length(min = 36, max = 36),
        regex(pattern = r"^[\x20-\x7E]+$"),
        extend("format" = "uuid")
    )]
    pub id: String,
}

#[derive(JsonSchema, serde::Serialize)]
pub enum EuRegion {
    #[serde(rename = "eu-west")]
    EuWest,
    #[serde(rename = "eu-central")]
    EuCentral,
}

/// `/payload/region` must be an EU region: a finite enum within the
/// inherited `maxLength` and `pattern`, which it need not repeat.
#[derive(JsonSchema)]
pub struct EuRegionRules {
    pub region: EuRegion,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.eu_tenant.v1~",
    description = "EU tenant subject",
    properties = "tenant,ext",
    overrides(SubjectV1 = TenantIdRules, RegionalSubjectV1 = EuRegionRules)
)]
#[derive(Debug)]
pub struct EuTenantSubjectV1<E> {
    pub tenant: String,
    pub ext: E,
}

#[derive(JsonSchema, serde::Serialize)]
pub enum EuWest {
    #[serde(rename = "eu-west")]
    EuWest,
}

#[derive(JsonSchema)]
pub struct EuWestRules {
    pub region: EuWest,
}

/// L4: narrows the region again, within L3's EU regions.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EuTenantSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.eu_tenant.v1~x.test.ovr.eu_west.v1~",
    description = "EU-west subject",
    properties = "",
    overrides(RegionalSubjectV1 = EuWestRules)
)]
#[derive(Debug)]
pub struct EuWestSubjectV1;

/// The issue's shape: a unit struct that only narrows the root id.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.uuid_subject.v1~",
    description = "Subject with a UUID id",
    properties = "",
    overrides(SubjectV1 = TenantIdRules)
)]
#[derive(Debug)]
pub struct UuidSubjectV1;

/// Same chain, no overrides: the control for "unchanged schemas".
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.plain.v1~",
    description = "Plain",
    properties = "",
)]
#[derive(Debug)]
pub struct PlainSubjectV1;

#[allow(clippy::needless_pass_by_value)]
fn store_with<const N: usize>(schemas: [Value; N]) -> Result<GtsStore, String> {
    let mut store = GtsStore::new();
    for schema in &schemas {
        let id = schema["$id"].as_str().unwrap().trim_start_matches("gts://");
        store
            .register_schema(id, schema)
            .map_err(|e| e.to_string())?;
    }
    for schema in &schemas {
        let id = schema["$id"].as_str().unwrap().trim_start_matches("gts://");
        store.validate_schema(id).map_err(|e| e.to_string())?;
    }
    Ok(store)
}

fn chain() -> GtsStore {
    store_with([
        SubjectV1::<()>::gts_schema_with_refs_allof(),
        RegionalSubjectV1::<()>::gts_schema_with_refs_allof(),
        EuTenantSubjectV1::<()>::gts_schema_with_refs_allof(),
        EuWestSubjectV1::gts_schema_with_refs_allof(),
        UuidSubjectV1::gts_schema_with_refs_allof(),
    ])
    .unwrap()
}

const UUID: &str = "0b9c3a4e-6f1d-4c2b-9e8a-1d2c3b4a5f60";

fn instance(id: &str, region: &str) -> Value {
    json!({
        "type": "gts.x.test.ovr.subject.v1~",
        "id": id,
        "level": 1,
        "tags": [],
        "address": { "city": "Berlin", "country": "DE" },
        "payload": {
            "region": region,
            "details": { "tenant": "acme", "ext": {} }
        }
    })
}

/// Diagnostic returned by explicit override validation.
fn validation_error<T: GtsSchema>() -> String {
    T::validate_overrides()
        .expect_err("override validation should fail")
        .to_string()
}

// ---------------------------------------------------------------------------
// Generated overlay
// ---------------------------------------------------------------------------

#[test]
fn root_id_is_replaced_by_the_helper_field() {
    let schema = UuidSubjectV1::gts_schema_with_refs_allof();
    assert_eq!(
        schema["allOf"][1]["properties"]["id"],
        json!({
            "type": "string",
            "minLength": 36,
            "maxLength": 36,
            "pattern": r"^[\x20-\x7E]+$",
            "format": "uuid"
        }),
        "{schema:#}"
    );
    assert_eq!(schema["allOf"][1]["required"], json!(["id"]), "{schema:#}");
    assert!(schema.get("definitions").is_none(), "{schema:#}");
}

#[test]
fn intermediate_field_is_overridden_at_its_generic_path() {
    let schema = EuTenantSubjectV1::<()>::gts_schema_with_refs_allof();
    let level = &schema["allOf"][1]["properties"]["payload"];
    assert_eq!(
        level["properties"]["region"],
        json!({ "type": "string", "enum": ["eu-west", "eu-central"] }),
        "{schema:#}"
    );
    // The wrapper level stays open; only the type's own level is closed.
    assert!(level.get("additionalProperties").is_none(), "{schema:#}");
    assert_eq!(
        level["properties"]["details"]["additionalProperties"],
        json!(false),
        "{schema:#}"
    );
    assert_eq!(
        schema["allOf"][1]["properties"]["id"]["format"], "uuid",
        "{schema:#}"
    );
}

#[test]
fn fourth_level_replacement_is_checked_against_third_level_override() {
    let schema = EuWestSubjectV1::gts_schema_with_refs_allof();
    let region = &schema["allOf"][1]["properties"]["payload"]["properties"]["region"];
    // Nothing inherited is merged in; the enum was proven within L2's
    // declaration and L3's override.
    assert_eq!(
        region,
        &json!({ "type": "string", "enum": ["eu-west"] }),
        "{schema:#}"
    );
}

#[test]
fn schemas_without_overrides_are_unchanged() {
    let schema = PlainSubjectV1::gts_schema_with_refs_allof();
    let overlay = &schema["allOf"][1];
    assert_eq!(
        overlay["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        vec!["payload"],
        "{schema:#}"
    );
    assert!(overlay.get("required").is_none(), "{schema:#}");
}

mod same_named_ancestor {
    use super::SubjectV1 as ParentV1;
    use super::*;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = ParentV1,
        type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.same_name.v1~",
        description = "Derived type sharing its ancestor's Rust name",
        properties = "",
        overrides(super::SubjectV1 = TenantIdRules)
    )]
    pub struct SubjectV1;

    #[test]
    fn qualified_ancestor_can_share_the_derived_types_name() {
        store_with([
            ParentV1::<()>::gts_schema_with_refs_allof(),
            SubjectV1::gts_schema_with_refs_allof(),
        ])
        .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Admission and instance validation
// ---------------------------------------------------------------------------

#[test]
fn registry_admits_the_overridden_chain() {
    chain();
}

#[test]
fn uuid_subject_accepts_uuid_ids_only() {
    let mut store = chain();
    let id = UuidSubjectV1::gts_type_id().as_ref().to_owned();
    let regional = RegionalSubjectV1::<()>::gts_type_id().as_ref().to_owned();
    let mut ok = instance(UUID, "us-east");
    ok["payload"]["details"] = json!({});
    store.validate_payload(&id, &ok).unwrap();

    // 36 printable chars, not a UUID: the base accepts it, the derived does not.
    let mut not_uuid = ok.clone();
    not_uuid["id"] = json!("x".repeat(36));
    store.validate_payload(&regional, &not_uuid).unwrap();
    assert!(store.validate_payload(&id, &not_uuid).is_err());

    let mut short = ok;
    short["id"] = json!("abc");
    assert!(store.validate_payload(&id, &short).is_err());
}

#[test]
fn eu_tenant_enforces_both_levels() {
    let mut store = chain();
    let id = EuTenantSubjectV1::<()>::gts_type_id().as_ref().to_owned();
    store
        .validate_payload(&id, &instance(UUID, "eu-central"))
        .unwrap();
    assert!(
        store
            .validate_payload(&id, &instance(UUID, "us-east"))
            .is_err()
    );
    assert!(
        store
            .validate_payload(&id, &instance("not-a-uuid", "eu-central"))
            .is_err()
    );

    let west = EuWestSubjectV1::gts_type_id().as_ref().to_owned();
    store
        .validate_payload(&west, &instance(UUID, "eu-west"))
        .unwrap();
    assert!(
        store
            .validate_payload(&west, &instance(UUID, "eu-central"))
            .is_err()
    );
}

// ---------------------------------------------------------------------------
// Required / Option / null
// ---------------------------------------------------------------------------

#[derive(JsonSchema)]
pub struct NicknameRequired {
    #[schemars(length(min = 3, max = 64))]
    pub nickname: String,
}

#[derive(JsonSchema)]
pub struct NicknameNonNullOptional {
    #[serde(default)]
    #[schemars(length(min = 3, max = 64))]
    pub nickname: String,
}

/// Optional and non-null, but without the inherited `maxLength: 64`.
#[derive(JsonSchema)]
pub struct NicknameNonNullUnbounded {
    #[serde(default)]
    #[schemars(length(min = 3))]
    pub nickname: String,
}

#[derive(JsonSchema)]
pub struct NicknameStillNullable {
    #[schemars(length(min = 3, max = 64))]
    pub nickname: Option<String>,
}

/// `/level` is required by the base; `#[serde(default)]` makes it optional.
#[derive(JsonSchema)]
pub struct LevelDefault {
    #[serde(default)]
    #[schemars(range(min = 0, max = 50))]
    pub level: u32,
}

#[derive(JsonSchema)]
pub struct NullableId {
    pub id: Option<String>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.nick_req.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = NicknameRequired)
)]
pub struct NickRequiredV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.nick_def.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = NicknameNonNullOptional)
)]
pub struct NickDefaultV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.nick_null.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = NicknameStillNullable)
)]
pub struct NickNullableV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.null_id.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = NullableId)
)]
pub struct NullableIdV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.nick_unbounded.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = NicknameNonNullUnbounded)
)]
pub struct NickUnboundedV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.level_default.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = LevelDefault)
)]
pub struct LevelDefaultV1;

#[test]
fn non_option_helper_field_makes_a_nullable_field_required_and_non_null() {
    let schema = NickRequiredV1::gts_schema_with_refs_allof();
    let overlay = &schema["allOf"][1];
    assert_eq!(overlay["properties"]["nickname"]["type"], "string");
    assert_eq!(overlay["properties"]["nickname"]["maxLength"], 64);
    assert_eq!(overlay["properties"]["nickname"]["minLength"], 3);
    assert_eq!(overlay["required"], json!(["nickname"]), "{schema:#}");
}

#[test]
fn serde_default_keeps_a_field_optional_without_admitting_null() {
    let schema = NickDefaultV1::gts_schema_with_refs_allof();
    let overlay = &schema["allOf"][1];
    assert_eq!(overlay["properties"]["nickname"]["type"], "string");
    assert!(overlay.get("required").is_none(), "{schema:#}");
    // `default` from `#[serde(default)]` is an annotation, not carried over.
    assert!(overlay["properties"]["nickname"].get("default").is_none());
}

#[test]
fn option_helper_field_keeps_null_where_the_base_allows_it() {
    let schema = NickNullableV1::gts_schema_with_refs_allof();
    let overlay = &schema["allOf"][1];
    assert_eq!(
        overlay["properties"]["nickname"]["type"],
        json!(["string", "null"])
    );
    assert!(overlay.get("required").is_none(), "{schema:#}");
}

#[test]
fn option_helper_field_cannot_admit_null_the_base_forbids() {
    let error = validation_error::<NullableIdV1>();
    assert!(error.contains("field '/id'"), "{error}");
    assert!(error.contains("admits null"), "{error}");
}

#[test]
fn optional_non_null_helper_over_a_nullable_field_must_repeat_its_bounds() {
    let _ = NickDefaultV1::gts_schema_with_refs_allof();
    assert_rejected::<NickUnboundedV1>("/nickname", "maxLength");
}

/// The final `allOf` would still require `/level` through the base, but the
/// override itself declares it optional, which it may not.
#[test]
fn serde_default_cannot_make_a_required_field_optional() {
    assert_rejected::<LevelDefaultV1>("/level", "cannot make a field optional");
}

#[test]
fn required_and_nullability_survive_admission() {
    let mut store = store_with([
        SubjectV1::<()>::gts_schema_with_refs_allof(),
        RegionalSubjectV1::<()>::gts_schema_with_refs_allof(),
        NickRequiredV1::gts_schema_with_refs_allof(),
        NickDefaultV1::gts_schema_with_refs_allof(),
        NickNullableV1::gts_schema_with_refs_allof(),
    ])
    .unwrap();
    let mut payload = instance(UUID, "us-east");
    payload["payload"]["details"] = json!({});

    let required = NickRequiredV1::gts_type_id().as_ref().to_owned();
    assert!(store.validate_payload(&required, &payload).is_err());
    let defaulted = NickDefaultV1::gts_type_id().as_ref().to_owned();
    store.validate_payload(&defaulted, &payload).unwrap();
    let nullable = NickNullableV1::gts_type_id().as_ref().to_owned();
    payload["nickname"] = Value::Null;
    store.validate_payload(&nullable, &payload).unwrap();
    assert!(store.validate_payload(&defaulted, &payload).is_err());
}

// ---------------------------------------------------------------------------
// Nested objects, arrays, enums via refs
// ---------------------------------------------------------------------------

/// A complete address: every nested field and bound is restated, and so is
/// the closedness the macro gives the inherited nested struct.
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TwoLetterAddress {
    #[schemars(length(max = 100))]
    pub city: String,
    #[schemars(length(min = 2, max = 2))]
    pub country: String,
}

#[derive(JsonSchema)]
pub struct AddressRules {
    pub address: TwoLetterAddress,
}

/// Only the country: the required `city` is dropped, not inherited.
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CountryOnly {
    #[schemars(length(min = 2, max = 2))]
    pub country: String,
}

#[derive(JsonSchema)]
pub struct PartialAddressRules {
    pub address: CountryOnly,
}

#[derive(JsonSchema)]
pub struct TagRules {
    #[schemars(length(max = 4), inner(length(max = 8)))]
    pub tags: Vec<String>,
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.nested.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = AddressRules)
)]
pub struct NestedV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.partial.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = PartialAddressRules)
)]
pub struct PartialAddressV1;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = RegionalSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.tags.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = TagRules)
)]
pub struct TagsV1;

#[test]
fn nested_struct_field_is_inlined_and_replaced_whole() {
    let schema = NestedV1::gts_schema_with_refs_allof();
    let address = &schema["allOf"][1]["properties"]["address"];
    assert_eq!(
        address,
        &json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "city": { "type": "string", "maxLength": 100 },
                "country": { "type": "string", "minLength": 2, "maxLength": 2 }
            },
            "required": ["city", "country"]
        }),
        "{schema:#}"
    );
    assert!(schema.get("definitions").is_none(), "{schema:#}");
    let mut store = store_with([
        SubjectV1::<()>::gts_schema_with_refs_allof(),
        RegionalSubjectV1::<()>::gts_schema_with_refs_allof(),
        schema,
    ])
    .unwrap();
    let mut payload = instance(UUID, "us-east");
    payload["payload"]["details"] = json!({});
    let id = NestedV1::gts_type_id().as_ref().to_owned();
    store.validate_payload(&id, &payload).unwrap();
    payload["address"]["country"] = json!("DEU");
    assert!(store.validate_payload(&id, &payload).is_err());
}

#[test]
fn nested_object_omitting_a_required_child_is_rejected() {
    assert_rejected::<PartialAddressV1>("/address", "required");
}

#[test]
fn scalar_array_items_are_replaced() {
    let schema = TagsV1::gts_schema_with_refs_allof();
    assert_eq!(
        schema["allOf"][1]["properties"]["tags"],
        json!({
            "type": "array",
            "maxItems": 4,
            "items": { "type": "string", "maxLength": 8 }
        }),
        "{schema:#}"
    );
    store_with([
        SubjectV1::<()>::gts_schema_with_refs_allof(),
        RegionalSubjectV1::<()>::gts_schema_with_refs_allof(),
        schema,
    ])
    .unwrap();
}

// ---------------------------------------------------------------------------
// Rejected overrides (diagnostics carry ancestor + wire path)
// ---------------------------------------------------------------------------

#[rustfmt::skip] // rustfmt re-indents the attribute on every run
macro_rules! rejected_type {
    ($name:ident, $type_id:tt, $anchor:ident = $helper:ident) => {
        #[struct_to_gts_schema(
            dir_path = "schemas",
            base = RegionalSubjectV1,
            type_id = $type_id,
            description = "t",
            properties = "",
            overrides($anchor = $helper)
        )]
        pub struct $name;
    };
}

#[derive(JsonSchema)]
pub struct WideId {
    #[schemars(length(min = 1, max = 2048), regex(pattern = r"^[\x20-\x7E]+$"))]
    pub id: String,
}

#[derive(JsonSchema)]
pub struct EmptyIdRange {
    #[schemars(length(min = 2000))]
    pub id: String,
}

/// Restates the pattern but drops both length bounds.
#[derive(JsonSchema)]
pub struct DroppedIdBounds {
    #[schemars(regex(pattern = r"^[\x20-\x7E]+$"))]
    pub id: String,
}

/// Every inherited bound kept, but a wider pattern.
#[derive(JsonSchema)]
pub struct AnyIdPattern {
    #[schemars(length(min = 1, max = 1024), regex(pattern = ".*"))]
    pub id: String,
}

/// Uninhabited: its schema is `false`.
pub struct Never;

impl JsonSchema for Never {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Never".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::Schema::from(false)
    }
}

#[derive(JsonSchema)]
pub struct DisabledId {
    pub id: Never,
}

#[derive(JsonSchema)]
pub struct UnknownField {
    pub nonexistent: String,
}

#[derive(JsonSchema)]
pub struct SlotRules {
    pub payload: String,
}

#[derive(JsonSchema)]
pub struct NumericId {
    pub id: u32,
}

#[derive(JsonSchema)]
pub struct OtherFormat {
    #[schemars(
        length(min = 36, max = 36),
        regex(pattern = r"^[\x20-\x7E]+$"),
        extend("format" = "email")
    )]
    pub id: String,
}

#[derive(JsonSchema)]
pub struct WideLevel {
    #[schemars(range(max = 1000))]
    pub level: u32,
}

#[derive(JsonSchema)]
pub enum NotAStruct {
    A,
    B,
}

#[derive(JsonSchema)]
#[allow(clippy::empty_structs_with_brackets)] // `{}` is the case under test
pub struct NoFields {}

#[derive(JsonSchema)]
pub struct NestedUnknown {
    pub address: NestedUnknownInner,
}

#[derive(JsonSchema)]
pub struct NestedUnknownInner {
    pub zip: String,
}

#[derive(JsonSchema)]
pub struct Recursive {
    pub id: String,
    pub next: Option<Box<Recursive>>,
}

#[derive(JsonSchema)]
pub struct FlattenedMap {
    pub id: String,
    #[serde(flatten)]
    pub rest: std::collections::BTreeMap<String, String>,
}

#[derive(JsonSchema, serde::Serialize)]
pub enum UsRegion {
    #[serde(rename = "us-east")]
    UsEast,
}

#[derive(JsonSchema)]
pub struct UsRegionRules {
    pub region: UsRegion,
}

#[derive(JsonSchema)]
pub struct BadEnumValue {
    #[schemars(extend("enum" = ["Not_Valid"]))]
    pub region: String,
}

rejected_type!(
    WideIdV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.wide_id.v1~",
    SubjectV1 = WideId
);
rejected_type!(
    EmptyRangeV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.empty_range.v1~",
    SubjectV1 = EmptyIdRange
);
rejected_type!(
    DroppedBoundsV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.dropped.v1~",
    SubjectV1 = DroppedIdBounds
);
rejected_type!(
    AnyPatternV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.any_pattern.v1~",
    SubjectV1 = AnyIdPattern
);
rejected_type!(
    DisabledIdV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.disabled.v1~",
    SubjectV1 = DisabledId
);
rejected_type!(
    UnknownFieldV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.unknown.v1~",
    SubjectV1 = UnknownField
);
rejected_type!(
    SlotV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.slot.v1~",
    SubjectV1 = SlotRules
);
rejected_type!(
    NumericIdV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.numeric.v1~",
    SubjectV1 = NumericId
);
rejected_type!(
    WideLevelV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.wide_level.v1~",
    SubjectV1 = WideLevel
);
rejected_type!(
    EnumHelperV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.enum_helper.v1~",
    SubjectV1 = NotAStruct
);
rejected_type!(
    NoFieldsV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.no_fields.v1~",
    SubjectV1 = NoFields
);
rejected_type!(
    NestedUnknownV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.nested_unknown.v1~",
    SubjectV1 = NestedUnknown
);
rejected_type!(
    RecursiveV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.recursive.v1~",
    SubjectV1 = Recursive
);
rejected_type!(
    FlattenV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.flatten.v1~",
    SubjectV1 = FlattenedMap
);
rejected_type!(
    BadEnumV1,
    "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.bad_enum.v1~",
    RegionalSubjectV1 = BadEnumValue
);

/// L4 under L3's EU regions: an enum value L2's pattern admits but L3's
/// override does not.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EuTenantSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.eu_tenant.v1~x.test.ovr.us_east.v1~",
    description = "t",
    properties = "",
    overrides(RegionalSubjectV1 = UsRegionRules)
)]
pub struct UsUnderEuV1;

/// L4 restating `/id` as L1 declares it widens L3's UUID override.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EuTenantSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.eu_tenant.v1~x.test.ovr.wide_again.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = WideIdUnderL3)
)]
pub struct WideAgainV1;

/// L4 asking for another format than L3's `uuid`.
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = EuTenantSubjectV1,
    type_id = "gts.x.test.ovr.subject.v1~x.test.ovr.regional.v1~x.test.ovr.eu_tenant.v1~x.test.ovr.email_id.v1~",
    description = "t",
    properties = "",
    overrides(SubjectV1 = OtherFormat)
)]
pub struct OtherFormatV1;

/// Exactly L1's declaration: included in it, but not in L3's UUID override.
#[derive(JsonSchema)]
pub struct WideIdUnderL3 {
    #[schemars(length(min = 1, max = 1024), regex(pattern = r"^[\x20-\x7E]+$"))]
    pub id: String,
}

fn assert_rejected<T: GtsSchema>(path: &str, needle: &str) {
    let error = validation_error::<T>();
    assert!(error.contains(&format!("field '{path}'")), "{error}");
    assert!(error.contains(needle), "{error}");
}

#[test]
fn generation_emits_overrides_without_validating_them() {
    for schema in [
        WideIdV1::gts_schema_with_refs(),
        WideIdV1::gts_schema_with_refs_allof(),
    ] {
        assert_eq!(schema["allOf"][1]["properties"]["id"]["maxLength"], 2048);
    }
    let schema = UnknownFieldV1::gts_schema_with_refs();
    assert_eq!(
        schema["allOf"][1]["properties"]["nonexistent"]["type"],
        "string"
    );
}

#[test]
fn widening_is_rejected_with_field_path() {
    assert_rejected::<WideIdV1>("/id", "the override widens the inherited declaration");
    assert_rejected::<WideIdV1>("/id", "maxLength");
    assert_rejected::<WideLevelV1>("/level", "maximum");
}

#[test]
fn omitted_bounds_are_not_inherited_but_rejected() {
    assert_rejected::<EmptyRangeV1>("/id", "maxLength");
    assert_rejected::<DroppedBoundsV1>("/id", "minLength");
}

#[test]
fn a_different_pattern_is_not_proven() {
    assert_rejected::<AnyPatternV1>("/id", "is not proven to be included");
}

#[test]
fn a_false_field_schema_is_rejected_by_explicit_validation() {
    assert_rejected::<DisabledIdV1>("/id", "would disable the inherited field");
}

#[test]
fn undeclared_field_is_rejected() {
    assert_rejected::<UnknownFieldV1>("/nonexistent", "is not declared by the ancestor");
    // A nested replacement is a whole object: an unknown nested field leaves
    // the declared ones unconstrained.
    assert_rejected::<NestedUnknownV1>("/address", "widens");
}

#[test]
fn generic_slot_is_rejected() {
    assert_rejected::<SlotV1>("/payload", "generic slot");
}

#[test]
fn type_and_format_conflicts_are_rejected() {
    assert_rejected::<NumericIdV1>("/id", "widens");
    assert_rejected::<OtherFormatV1>("/id", "format");
}

#[test]
fn unsupported_helper_shapes_are_rejected() {
    assert_rejected::<EnumHelperV1>("/", "struct with named fields");
    assert_rejected::<NoFieldsV1>("/", "declares no fields");
    assert_rejected::<RecursiveV1>("/", "recursive");
    assert_rejected::<FlattenV1>("/", "additionalProperties");
}

#[test]
fn enum_value_violating_an_inherited_declaration_is_rejected() {
    assert_rejected::<BadEnumV1>("/payload/region", "widens");
    assert_rejected::<UsUnderEuV1>("/payload/region", "eu-central");
}

#[test]
fn older_anchor_cannot_bypass_an_intermediate_narrowing() {
    assert_rejected::<WideAgainV1>("/id", "\"maxLength\":36");
}

// ---------------------------------------------------------------------------
// multipleOf across three levels: generated and admitted by the same checker
// ---------------------------------------------------------------------------

mod multiple_of {
    use super::*;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = true,
        type_id = "gts.x.test.ovr_mul.batch.v1~",
        description = "Batch with an even count",
        properties = "batch_type,count,body"
    )]
    pub struct BatchV1<B> {
        #[serde(rename = "type")]
        pub batch_type: gts::gts::GtsTypeId,
        #[schemars(extend("multipleOf" = 2))]
        pub count: u32,
        pub body: B,
    }

    #[derive(JsonSchema)]
    pub struct CountOf4 {
        #[schemars(extend("multipleOf" = 4))]
        pub count: u32,
    }

    #[derive(JsonSchema)]
    pub struct CountOf8 {
        #[schemars(extend("multipleOf" = 8))]
        pub count: u32,
    }

    #[derive(JsonSchema)]
    pub struct CountOf2 {
        #[schemars(extend("multipleOf" = 2))]
        pub count: u32,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = BatchV1,
        type_id = "gts.x.test.ovr_mul.batch.v1~x.test.ovr_mul.quad.v1~",
        description = "Count in fours",
        properties = "extra",
        overrides(BatchV1 = CountOf4)
    )]
    pub struct QuadBatchV1<X> {
        pub extra: X,
    }

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = QuadBatchV1,
        type_id = "gts.x.test.ovr_mul.batch.v1~x.test.ovr_mul.quad.v1~x.test.ovr_mul.oct.v1~",
        description = "Count in eights",
        properties = "",
        overrides(BatchV1 = CountOf8)
    )]
    pub struct OctBatchV1;

    #[struct_to_gts_schema(
        dir_path = "schemas",
        base = QuadBatchV1,
        type_id = "gts.x.test.ovr_mul.batch.v1~x.test.ovr_mul.quad.v1~x.test.ovr_mul.back_to_two.v1~",
        description = "Widens the intermediate level's 4 back to 2",
        properties = "",
        overrides(BatchV1 = CountOf2)
    )]
    pub struct BackToTwoV1;

    #[test]
    fn narrowing_multiple_of_generates_and_is_admitted() {
        let oct = OctBatchV1::gts_schema_with_refs_allof();
        assert_eq!(
            oct["allOf"][1]["properties"]["count"]["multipleOf"], 8,
            "{oct:#}"
        );
        let mut store = store_with([
            BatchV1::<()>::gts_schema_with_refs_allof(),
            QuadBatchV1::<()>::gts_schema_with_refs_allof(),
            oct,
        ])
        .unwrap();
        let id = OctBatchV1::gts_type_id().as_ref().to_owned();
        let mut payload = json!({
            "type": "gts.x.test.ovr_mul.batch.v1~",
            "count": 16,
            "body": { "extra": {} }
        });
        store.validate_payload(&id, &payload).unwrap();
        payload["count"] = json!(12);
        assert!(store.validate_payload(&id, &payload).is_err());
    }

    #[test]
    fn widening_an_intermediate_multiple_of_is_rejected() {
        assert_rejected::<BackToTwoV1>("/count", "multipleOf");
    }
}
