# Implementation Plan: ADR-001 Macro Redesign

This plan implements the redesign specified in [ADR-001](./001-macro-alignment-adr.md). The work is structured in phases that can each be independently tested and merged.

Each phase references the GTS specification sections that justify the design choices. Spec references link to the [GTS Specification](https://github.com/GlobalTypeSystem/gts-spec).

---

## Phase 1: New derive macro skeleton with attribute parsing

**Goal**: Create `#[derive(GtsSchema)]` with `#[gts(...)]` attribute parsing. No code generation yet — just parsing, validation, and error reporting.

### 1.1 Attribute parsing

Create the `GtsSchema` derive macro entry point in `gts-macros/src/lib.rs` (alongside the existing `struct_to_gts_schema`). Parse `#[gts(...)]` attributes into a struct:

```rust
struct GtsAttrs {
    dir_path: String,
    schema_id: String,
    description: String,
    extends: Option<syn::Ident>,         // None = root type
    allow_direct_serde: bool,
}
```

Parse field-level attributes:

```rust
enum GtsFieldAttr {
    TypeField,      // #[gts(type_field)]
    InstanceId,     // #[gts(instance_id)]
    Skip,           // #[gts(skip)]
}
```

**Spec justification:**
- `schema_id` — maps to the `$id` in JSON Schema documents [[Spec §9.1](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema), [§11.1 Rule C](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)]
- `extends` — models left-to-right inheritance via chained identifiers [[Spec §2.2](https://github.com/GlobalTypeSystem/gts-spec#22-chained-identifiers), [§3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)]
- `type_field` / `instance_id` — optional, maps to anonymous instance `type` field [[Spec §3.7](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances), [§11.1 Rule C](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)] or well-known instance `id` field. Made optional because not all GTS schemas require instance-level identity (e.g., `order.v1.0~`, `contact.v1.0~` have plain UUID `id` fields with no GTS semantics) [[Spec §11.1 Rule C](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)]. **This is the fix for Issue #72.**
- Field names are implementation-defined [[Spec §11.1](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)]

### 1.2 Validation (carried over from current macro)

Implement all validations that apply to the new design:

- `schema_id` format validation via `gts_id::validate_gts_id()` [Spec §2.1, §2.3, §8.1](https://github.com/GlobalTypeSystem/gts-spec)
- Version match between struct name suffix and schema ID [Spec §4](https://github.com/GlobalTypeSystem/gts-spec#4-versioning)
- Segment count: no `extends` → single segment; `extends` → multi-segment [[Spec §2.2](https://github.com/GlobalTypeSystem/gts-spec#22-chained-identifiers)]
- Only named structs (no tuple structs, enums)
- Max 1 generic type parameter (GTS inheritance is single-chain, not multi-branch) [[Spec §3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)]
- `#[gts(type_field)]` must be on a `GtsSchemaId` field — the `type` field value is a GTS type identifier (ending with `~`) [[Spec §3.7](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances), [§11.1 Rule C](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)]
- `#[gts(instance_id)]` must be on a `GtsInstanceId` field — the `id` field value is a GTS instance identifier (no trailing `~`) [[Spec §3.7](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances), [§11.1 Rule C](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)]
- `#[gts(type_field)]` and `#[gts(instance_id)]` are mutually exclusive — a schema's instances follow either the well-known or anonymous pattern [[Spec §3.7](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances)]
- At most one `#[gts(type_field)]` and one `#[gts(instance_id)]` per struct
- Unknown `#[gts(...)]` attributes emit clear errors

### 1.3 Tests for Phase 1

**Compile-fail tests** (`tests/compile_fail_v2/`):

| Test | Validates | Spec reference |
|---|---|---|
| `missing_schema_id` | `#[gts(...)]` without `schema_id` | §9.1 — schema `$id` is mandatory |
| `missing_description` | `#[gts(...)]` without `description` | Spec examples consistently include `description` |
| `missing_dir_path` | `#[gts(...)]` without `dir_path` | Implementation requirement for CLI |
| `invalid_gts_id` | Malformed schema_id string | §2.1, §2.3, §8.1 — identifier format rules |
| `version_mismatch` | Struct `V1` with schema `v2~` | §4 — version consistency |
| `root_multi_segment` | No `extends` but multi-segment schema_id | §2.2 — single segment = base type, multi-segment = derived |
| `extends_single_segment` | `extends = Parent` with single-segment schema_id | §2.2 — derived types must chain |
| `tuple_struct` | `#[derive(GtsSchema)]` on tuple struct | JSON Schema `"type": "object"` maps to named fields |
| `enum_not_supported` | `#[derive(GtsSchema)]` on enum | JSON Schema `"type": "object"` maps to structs |
| `multiple_generics` | Struct with 2+ type params | §3.2 — inheritance is single-chain |
| `type_field_wrong_type` | `#[gts(type_field)]` on `String` field | §3.7 — type field must be a GTS type identifier (ending with `~`) |
| `instance_id_wrong_type` | `#[gts(instance_id)]` on `Uuid` field | §3.7 — well-known id must be a GTS instance identifier |
| `both_type_and_instance` | Same struct has both `#[gts(type_field)]` and `#[gts(instance_id)]` | §3.7 — well-known and anonymous are distinct patterns |
| `duplicate_type_field` | Two fields with `#[gts(type_field)]` | One identity field per entity |
| `unknown_gts_attr` | `#[gts(nonexistent)]` | Fail-fast on typos |
| `extends_parent_mismatch` | Parent's `SCHEMA_ID` doesn't match parent segment | §3.2 — chain must be valid derivation |
| `extends_parent_no_generic` | Parent struct has no generic parameter | Inheritance requires a slot for child properties |
| `nested_direct_serialize` | `extends` struct with `Serialize` derive (no `allow_direct_serde`) | Nested structs produce incomplete JSON without base envelope |
| `nested_direct_serialize_cfg_attr` | Same via `cfg_attr` | Same as above |

---

## Phase 2: GtsSchema trait implementation and runtime API

**Goal**: Generate the `GtsSchema` trait impl and all runtime methods. At this point, the new macro produces the same runtime API as the old one.

### 2.1 Auto-derive JsonSchema

If the struct does not already derive `schemars::JsonSchema`, inject it. This is required for `schemars::schema_for!(Self)` used in `gts_schema_with_refs()`.

### 2.2 GtsSchema trait implementation

Generate the `GtsSchema` trait impl with:
- `SCHEMA_ID` — from `schema_id` attribute [[Spec §9.1](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema)]
- `GENERIC_FIELD` — detected from struct fields (field whose type matches the generic param)
- `gts_schema_with_refs()` / `gts_schema_with_refs_allof()` — runtime schema generation using `schemars::schema_for!(Self)`, resolving `$ref` for `GtsSchemaId`/`GtsInstanceId`
- `innermost_schema_id()`, `innermost_schema()`, `collect_nesting_path()` — for generic base structs [[Spec §3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)]
- `wrap_in_nesting_path()` — inherited from trait default

For `extends` structs:
- `allOf` + `$ref` schema composition [[Spec §9.1](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema), [§3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)]
- Compile-time assertion that parent's `SCHEMA_ID` matches [[Spec §3.1](https://github.com/GlobalTypeSystem/gts-spec#31-gts-types)]
- Property nesting under parent's generic field

### 2.3 Runtime API methods

Generate on the struct impl:
- `gts_schema_id() -> &'static GtsSchemaId` (LazyLock) [[Spec §9.1](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema)]
- `gts_base_schema_id() -> Option<&'static GtsSchemaId>` (LazyLock) [[Spec §3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)]
- `gts_make_instance_id(segment) -> GtsInstanceId` [[Spec §3.7](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances)]
- `gts_schema_with_refs_as_string() -> String`
- `gts_schema_with_refs_as_string_pretty() -> String`
- `gts_instance_json(&self) -> Value` — with `where Self: Serialize` bound
- `gts_instance_json_as_string(&self) -> String` — with `where Self: Serialize` bound
- `gts_instance_json_as_string_pretty(&self) -> String` — with `where Self: Serialize` bound

Generate associated constants:
- `GTS_SCHEMA_FILE_PATH` — from `dir_path` + `schema_id`
- `GTS_SCHEMA_DESCRIPTION` — from `description`
- `GTS_SCHEMA_PROPERTIES` — auto-generated from fields (excluding `#[gts(skip)]` / `#[serde(skip)]`)
- `BASE_SCHEMA_ID` — `Option<&str>`, `Some(parent_segment)` for `extends`, `None` otherwise

### 2.4 Include `description` in runtime schemas

The `gts_schema_with_refs_allof()` method should include `"description"` in the generated JSON schema output, sourced from `GTS_SCHEMA_DESCRIPTION`. This aligns with every spec example schema (e.g., `events.type.v1~.schema.json`, `events.topic.v1~.schema.json`, `compute.vm.v1~.schema.json`), all of which include a `description` field.

### 2.5 `x-gts-ref` on identity fields

When generating the runtime schema, if a field has `#[gts(type_field)]` or `#[gts(instance_id)]`, override its schema property to use `"x-gts-ref": "/$id"` instead of the default `"x-gts-ref": "gts.*"` from `json_schema_value()`.

**Spec justification** [[Spec §9.6](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support)]:
- `"x-gts-ref": "/$id"` — relative self-reference; field value must equal the current schema's `$id`. Used on identity fields that identify *this* entity (e.g., `type` on events, `id` on topics).
- `"x-gts-ref": "gts.*"` — generic reference; field must be any valid GTS identifier. Used on fields that reference *other* entities (e.g., `subjectType` referencing an order schema).

The spec examples consistently use `"/$id"` on identity fields:
- `events.type.v1~` → `"type"` property has `"x-gts-ref": "/$id"`
- `events.topic.v1~` → `"id"` property has `"x-gts-ref": "/$id"`
- `modules.capability.v1~` → `"id"` property has `"x-gts-ref": "/$id"`
- `compute.vm_state.v1~` → `"gtsId"` property has `"x-gts-ref": "/$id"`

### 2.6 Tests for Phase 2

**Integration tests** (`tests/v2_integration_tests.rs`):

| Test | Validates | Spec reference |
|---|---|---|
| `base_struct_schema_id` | `gts_schema_id()` returns correct value | §9.1 — `$id` access |
| `base_struct_schema_constants` | `SCHEMA_ID`, `GTS_SCHEMA_FILE_PATH`, `GTS_SCHEMA_DESCRIPTION`, `GTS_SCHEMA_PROPERTIES` | §9.1 |
| `base_struct_instance_id` | `gts_make_instance_id()` produces correct format | §3.7 — instance = schema chain + segment |
| `base_struct_schema_output` | `gts_schema_with_refs()` produces correct JSON schema structure | §9.1, §11.1 Rule C cat. 1 |
| `base_struct_schema_has_description` | Runtime schema includes `description` field | Spec examples consistently include `description` |
| `base_struct_no_gts_identity_field` | Struct without id/type field compiles and produces valid schema | **§11.1 Rule C** — not all schemas need identity fields (e.g., `order.v1.0~`, `contact.v1.0~`). **Fixes Issue #72.** |
| `base_struct_with_type_field` | `#[gts(type_field)]` field gets `x-gts-ref: "/$id"` in schema | §9.6 — `/$id` self-reference |
| `base_struct_with_instance_id` | `#[gts(instance_id)]` field gets `x-gts-ref: "/$id"` in schema | §9.6 — `/$id` self-reference |
| `base_struct_other_gts_fields` | Non-annotated `GtsSchemaId` fields retain `x-gts-ref: "gts.*"` | §9.6 — `gts.*` generic reference |
| `base_struct_gts_skip` | `#[gts(skip)]` field excluded from schema properties | ADR §4.2 |
| `base_struct_serde_skip` | `#[serde(skip)]` field excluded from schema properties | ADR §4.2 |
| `base_struct_properties_auto` | `GTS_SCHEMA_PROPERTIES` matches struct fields minus skipped | ADR §4.2 — auto-derived from fields |
| `base_struct_with_generic` | Generic base struct with `GENERIC_FIELD` set correctly | §3.2 — generic field is the extension point |
| `base_struct_no_generic` | Non-generic base struct (leaf type) | Leaf types have no extension point |
| `base_struct_schema_pretty` | `gts_schema_with_refs_as_string_pretty()` is valid formatted JSON | — |
| `base_struct_base_schema_id_none` | Root type returns `None` for `gts_base_schema_id()` | §2.2 — single-segment = no parent |

**Schema structure tests** (`tests/v2_schema_structure_tests.rs`):

| Test | Validates | Spec reference |
|---|---|---|
| `schema_has_id` | `$id` field is `gts://` + schema_id | §9.1 — `$id` must use `gts://` prefix |
| `schema_has_json_schema_ref` | `$schema` is `http://json-schema.org/draft-07/schema#` | §11.1 Rule A — `$schema` presence = schema document |
| `schema_type_object` | `type` is `"object"` | JSON Schema standard for struct-like types |
| `schema_additional_properties_false` | `additionalProperties` is `false` | §4.2 — closed content model for type safety |
| `schema_required_fields` | Non-`Option` fields appear in `required` | JSON Schema `required` semantics |
| `schema_optional_fields_not_required` | `Option<T>` fields not in `required` | JSON Schema `required` semantics |
| `schema_uuid_format` | `Uuid` fields have `format: "uuid"` | §9.10 — UUID support for instance IDs |
| `schema_gts_schema_id_format` | `GtsSchemaId` fields have `format: "gts-schema-id"` | §9.6 — GTS identifier reference |
| `schema_gts_instance_id_format` | `GtsInstanceId` fields have `format: "gts-instance-id"` | §9.6 — GTS identifier reference |

---

## Phase 3: Serialization — serde attribute injection and GtsSerialize/GtsDeserialize

**Goal**: Handle the serialization aspects — serde bound injection on generic fields, GtsSerialize/GtsDeserialize for nested structs, direct-serde blocking.

### 3.1 Serde attribute injection on generic fields

For base structs with a generic parameter `P`:
- Add `#[serde(bound(serialize = "P: GtsSerialize", deserialize = "P: GtsDeserialize<'de>"))]` to the struct
- Add `#[serde(serialize_with = "gts::serialize_gts", deserialize_with = "gts::deserialize_gts")]` to the generic field

### 3.2 GtsSerialize/GtsDeserialize for nested structs

For structs with `extends`:
- Generate explicit `GtsSerialize` impl (custom `SerializeStruct` with `GtsSerializeWrapper` for generic fields)
- Generate explicit `GtsDeserialize` impl (custom visitor with field identifier enum)
- The field identifier enum must use explicit per-field `#[serde(rename = "...")]` respecting the user's serde renames — **not** `rename_all = "snake_case"` (fixing the pre-existing bug)

### 3.3 Direct serde blocking for nested structs

For structs with `extends` (and without `allow_direct_serde`):
- Implement `GtsNoDirectSerialize` and `GtsNoDirectDeserialize` marker traits
- These conflict with the blanket impls if the user also derives `Serialize`/`Deserialize`

If `allow_direct_serde` is set, skip the marker trait impls.

### 3.4 Unit struct handling

- Base unit structs: generate custom `Serialize`/`Deserialize` that handles `{}` and `null`
- Nested unit structs: generate custom `GtsSerialize`/`GtsDeserialize` with same behavior

### 3.5 Instance serialization methods

Generate `gts_instance_json()`, `gts_instance_json_as_string()`, `gts_instance_json_as_string_pretty()` with `where Self: serde::Serialize` bound.

### 3.6 Tests for Phase 3

**Serialization tests** (`tests/v2_serialization_tests.rs`):

| Test | Validates |
|---|---|
| `base_struct_serialize` | Base struct serializes to JSON correctly |
| `base_struct_deserialize` | Base struct deserializes from JSON correctly |
| `base_struct_roundtrip` | Serialize → deserialize produces identical struct |
| `base_struct_with_nested_serialize` | `BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>` serializes with nested fields |
| `base_struct_with_nested_deserialize` | Same type deserializes from JSON |
| `base_struct_with_nested_roundtrip` | Full roundtrip through the generic chain |
| `nested_struct_gts_serialize` | `GtsSerialize` impl works correctly |
| `nested_struct_gts_deserialize` | `GtsDeserialize` impl works correctly |
| `serde_rename_respected` | `#[serde(rename = "type")]` appears in serialized JSON |
| `serde_rename_in_deserialize` | Deserialization reads renamed field correctly |
| `generic_field_serde_rename` | Renamed generic field nests correctly |
| `unit_struct_serialize` | Unit struct serializes to `{}` |
| `unit_struct_deserialize_object` | Unit struct deserializes from `{}` |
| `unit_struct_deserialize_null` | Unit struct deserializes from `null` |
| `nested_unit_struct` | Nested unit struct through GtsSerialize chain |
| `instance_json_methods` | `gts_instance_json()` returns correct `serde_json::Value` |
| `no_gts_identity_field_roundtrip` | Struct without id/type field round-trips correctly (issue #72) |

**Compile-fail tests** (add to `tests/compile_fail_v2/`):

| Test | Validates |
|---|---|
| `nested_direct_serialize` | `extends` + `Serialize` without `allow_direct_serde` fails |
| `nested_direct_serialize_cfg_attr` | Same via `cfg_attr` |
| `allow_direct_serde_on_root` | `allow_direct_serde` without `extends` is an error (or warning) |

---

## Phase 4: Inheritance chain tests

**Goal**: Verify that multi-level inheritance produces correct schemas and serialization. These tests mirror the existing `inheritance_tests.rs` patterns.

### 4.1 Tests for Phase 4

**Inheritance tests** (`tests/v2_inheritance_tests.rs`):

Define a test hierarchy:

```rust
// Level 1: Base event (root, generic)
#[derive(Debug, Serialize, Deserialize, GtsSchema)]
#[gts(dir_path = "schemas", schema_id = "gts.x.core.events.type.v1~", description = "Base event type")]
pub struct BaseEventV1<P: GtsSchema> {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

// Level 2: Audit payload (nested, generic)
#[derive(Debug, GtsSchema)]
#[gts(dir_path = "schemas", schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
      description = "Audit event with user context", extends = BaseEventV1)]
pub struct AuditPayloadV1<D: GtsSchema> {
    pub user_agent: String,
    pub user_id: Uuid,
    pub ip_address: String,
    pub data: D,
}

// Level 3: Place order data (nested, non-generic, leaf)
#[derive(Debug, GtsSchema)]
#[gts(dir_path = "schemas", schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
      description = "Order placement audit event", extends = AuditPayloadV1)]
pub struct PlaceOrderDataV1 {
    pub order_id: Uuid,
    pub product_id: Uuid,
}
```

| Test | Validates | Spec reference |
|---|---|---|
| `two_level_schema` | Child schema has `allOf` with `$ref` to parent | §3.2, §9.1 — derived type uses `allOf` + `$ref` |
| `three_level_schema` | 3-level chain produces correct nested `allOf` | §3.2 — `A~B~C` left-to-right inheritance |
| `three_level_serialize` | Full 3-level instance serializes with correct nesting | §11.2 Example #2 — nested instance structure |
| `three_level_deserialize` | Full 3-level instance deserializes correctly | §11.2 — instance roundtrip |
| `three_level_roundtrip` | Serialize → deserialize preserves all fields | — |
| `child_schema_id` | Child `gts_schema_id()` returns full chained ID | §2.2 — chained identifier format |
| `child_base_schema_id` | Child `gts_base_schema_id()` returns parent's ID | §3.2 — parent segment extraction |
| `child_instance_id` | Child `gts_make_instance_id()` appends to full chain | §3.7 — instance = chain + segment |
| `innermost_schema_id` | `BaseEventV1::<AuditPayloadV1<PlaceOrderDataV1>>::innermost_schema_id()` returns leaf ID | §3.1 — rightmost type resolution |
| `generic_field_detection` | `GENERIC_FIELD` is `Some("payload")` for base, `Some("data")` for audit, `None` for leaf | §3.2 — generic field is the extension point |
| `unit_struct_child` | Unit struct as leaf in inheritance chain | Empty derived types (no new properties) |
| `schema_additional_properties` | Each nesting level has `additionalProperties: false` | §4.2 — closed content model |
| `nesting_path` | `collect_nesting_path()` returns correct path through chain | §3.2 — path from outer to inner |

---

## Phase 5: Parity validation and deprecation

**Goal**: Verify that the new macro produces identical behavior to the old one, then deprecate the old macro.

### 5.1 Schema parity tests

Create tests that generate schemas from both the old and new macros for equivalent struct definitions, and assert the schemas are identical (except for the `x-gts-ref` improvement on identity fields).

**Parity tests** (`tests/v2_parity_tests.rs`):

| Test | Validates |
|---|---|
| `base_event_schema_parity` | Old and new macros produce same base event schema |
| `child_event_schema_parity` | Old and new macros produce same child schema |
| `three_level_schema_parity` | Old and new macros produce same 3-level schemas |
| `instance_json_parity` | Old and new macros produce same serialized instance JSON |
| `deserialization_parity` | Same JSON deserializes identically under both macros |
| `topic_schema_parity` | Well-known instance (id field) schema parity |

### 5.2 New capability tests

Tests for features that the old macro couldn't support:

| Test | Validates | Spec reference |
|---|---|---|
| `data_entity_no_identity` | Struct without id/type field (issue #72) | **§11.1 Rule C** — data entities like `order.v1.0~` and `contact.v1.0~` have no GTS identity fields |
| `data_entity_roundtrip` | Serialize/deserialize without id/type field | §11.1 — instances of unknown/non-GTS schemas are valid |
| `gts_skip_field` | `#[gts(skip)]` excludes from schema but not serde | ADR §4.2 |
| `allow_direct_serde_nested` | `allow_direct_serde` enables Serialize on nested struct | ADR §4.4 |
| `description_in_runtime_schema` | `description` appears in `gts_schema_with_refs()` output | Spec examples — all schemas include `description` |
| `x_gts_ref_self_reference` | `#[gts(type_field)]` produces `"x-gts-ref": "/$id"` | §9.6 — `/$id` relative self-reference |
| `x_gts_ref_cross_reference` | Non-annotated `GtsSchemaId` retains `"x-gts-ref": "gts.*"` | §9.6 — `gts.*` generic reference |

### 5.3 Deprecate old macro

Add `#[deprecated]` to `struct_to_gts_schema` with a message pointing to the migration guide. The old macro continues to work but emits warnings.

### 5.4 Update README

Rewrite `gts-macros/README.md` to document the new `#[derive(GtsSchema)]` API, with migration examples from the ADR.

---

## Phase 6: Serde rename fix for nested deserializer

**Goal**: Fix the pre-existing bug where the nested struct deserializer uses `rename_all = "snake_case"` instead of respecting per-field serde renames.

### 6.1 Implementation

In the `GtsDeserialize` code generation for nested structs, replace:

```rust
#[serde(field_identifier, rename_all = "snake_case")]
enum Field {
    #field_idents,
    #[serde(other)]
    Unknown,
}
```

With per-field rename attributes:

```rust
#[serde(field_identifier)]
enum Field {
    #[serde(rename = "user_agent")]
    user_agent,
    #[serde(rename = "userId")]  // respects user's #[serde(rename)]
    user_id,
    #[serde(other)]
    Unknown,
}
```

### 6.2 Tests

| Test | Validates |
|---|---|
| `nested_camel_case_deserialize` | Nested struct with `#[serde(rename = "camelCase")]` deserializes correctly |
| `nested_mixed_renames` | Struct with mix of renamed and non-renamed fields |
| `nested_rename_roundtrip` | Serialize → deserialize with renamed fields |

---

## Implementation Order and Dependencies

```
Phase 1 ──→ Phase 2 ──→ Phase 3 ──→ Phase 4 ──→ Phase 5
  (parse)    (trait)     (serde)    (inherit)    (parity)
                                        │
                                   Phase 6
                                   (rename fix)
```

- Phases 1-4 are sequential — each builds on the previous
- Phase 5 (parity) requires all prior phases
- Phase 6 (rename fix) can be done independently after Phase 3

Each phase should be a separate PR with its own tests passing before merge.

---

## File Structure

```
gts-macros/
├── src/
│   ├── lib.rs                          # Both old + new macro entry points
│   ├── gts_schema_derive.rs            # New: #[derive(GtsSchema)] implementation
│   ├── gts_attrs.rs                    # New: #[gts(...)] attribute parsing
│   ├── gts_field_attrs.rs              # New: field-level attribute parsing
│   ├── gts_validation.rs               # New: validation logic (extracted + new)
│   ├── gts_codegen.rs                  # New: code generation (trait impl, runtime API)
│   └── gts_serde.rs                    # New: serde-related code generation
├── tests/
│   ├── compile_fail/                   # Existing (old macro)
│   ├── compile_fail_v2/                # New: compile-fail tests for new macro
│   ├── integration_tests.rs            # Existing (old macro)
│   ├── inheritance_tests.rs            # Existing (old macro)
│   ├── v2_integration_tests.rs         # New: integration tests
│   ├── v2_inheritance_tests.rs         # New: inheritance chain tests
│   ├── v2_serialization_tests.rs       # New: serialization tests
│   ├── v2_schema_structure_tests.rs    # New: schema output tests
│   ├── v2_parity_tests.rs             # New: old vs new comparison
│   └── ...
```

The new macro source is split into focused modules rather than a single 1800-line file. The old macro source remains in `lib.rs` until deprecation is complete.
