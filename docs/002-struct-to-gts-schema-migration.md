# Migrating from `#[struct_to_gts_schema]` to `#[derive(GtsSchema)]`

**Target design**: [ADR-001 — `#[derive(GtsSchema)]` Macro Design](./001-gts-schema-derive-macro-adr.md)
**Implementation plan**: [002-macro-migration-implementation-plan.md](./002-macro-migration-implementation-plan.md)
**Issue**: [#72 — `struct_to_gts_schema`: `gts_type` field blocks `Deserialize`](https://github.com/GlobalTypeSystem/gts-rust/issues/72)

This document covers what changes between the old and new macro and how to migrate. For the target design itself — what `#[derive(GtsSchema)]` does and why — see ADR-001.

---

## 1. Why migrate

The `#[struct_to_gts_schema]` attribute macro is the primary integration point between Rust structs and the Global Type System. It currently serves three purposes: compile-time validation, JSON Schema generation, and runtime API generation. While the macro delivers significant value, its monolithic design has led to accumulated friction as the GTS specification and user requirements have evolved.

Issue [#72](https://github.com/GlobalTypeSystem/gts-rust/issues/72) exposed a concrete symptom — the macro requires base structs to declare a `gts_type: GtsSchemaId` or `id: GtsInstanceId` field, but does not properly handle deserialization of that field. Users are forced into workarounds like `#[serde(skip_serializing, default = "dummy_fn")]`, which are fragile and pollute consumer code.

However, the root cause runs deeper than a missing `#[serde(skip)]`. The macro conflates multiple orthogonal concerns into a single attribute, couples identity-field validation to hardcoded field names, and silently manipulates user-controlled serde behavior in ways that are difficult to reason about.

The migration replaces `#[struct_to_gts_schema]` with a derive macro plus `#[gts(...)]` attributes that decompose those concerns into focused, composable units aligned with the GTS specification and Rust ecosystem conventions. The full target design is in [ADR-001](./001-gts-schema-derive-macro-adr.md); this document covers what callers need to do to move over.

---

## 2. The old macro today

### 2.1 What the macro does today

The `#[struct_to_gts_schema]` attribute macro accepts five required parameters:

```rust
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,                                    // or base = ParentStruct
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type",
    properties = "event_type,id,tenant_id,payload"
)]
```

In a single invocation, the macro performs all of the following:

1. **Validates the schema ID format** against GTS identifier rules
2. **Validates version consistency** between the struct name suffix (e.g., `V1`) and the schema ID version (e.g., `v1~`)
3. **Validates parent-child inheritance** — segment count, parent schema ID match
4. **Validates the `properties` list** — every listed property must exist as a struct field
5. **Requires id/type fields** — base structs must have either an `id: GtsInstanceId` or a type field (`type`/`gts_type`/etc.) of type `GtsSchemaId`, but not both
6. **Injects serde derives** — automatically adds `Serialize`, `Deserialize`, `JsonSchema` for base structs
7. **Removes serde derives** — strips `Serialize`/`Deserialize` from nested structs and emits compile errors if the user adds them manually
8. **Injects serde attributes** — adds `#[serde(bound(...))]` on the struct and `#[serde(serialize_with, deserialize_with)]` on generic fields for base structs
9. **Implements `GtsSchema` trait** — with `SCHEMA_ID`, `GENERIC_FIELD`, and schema composition methods
10. **Implements `GtsSerialize`/`GtsDeserialize`** — custom serialization traits for nested structs
11. **Implements `GtsNoDirectSerialize`/`GtsNoDirectDeserialize`** — marker traits that block direct serde usage on nested structs
12. **Generates runtime API** — `gts_schema_id()`, `gts_base_schema_id()`, `gts_make_instance_id()`, `gts_instance_json()`, schema string methods

### 2.2 The `base` attribute's dual role

The `base` attribute conflates two orthogonal concepts:

| `base` value | GTS meaning | Serialization meaning |
|---|---|---|
| `base = true` | Root type in GTS hierarchy | Gets direct `Serialize`/`Deserialize` via serde derives |
| `base = ParentStruct` | Child type inheriting from parent | Blocked from direct serialization; uses `GtsSerialize`/`GtsDeserialize` instead |

Conflating these two meanings into a single attribute makes the macro's behavior hard to read and hard to extend. The redesign splits them: inheritance is declared via `extends = Parent` (or its absence), and serialization behavior is a direct consequence of the GTS role — base structs must be `Serialize`/`Deserialize` for Type Registry round-tripping, nested structs must not.

### 2.3 The `properties` parameter

The `properties` parameter is a comma-separated string listing which struct fields should appear in the generated JSON Schema:

```rust
properties = "event_type,id,tenant_id,payload"
```

This requires the user to duplicate the field list — once in the struct definition and once in `properties`. If a field is added to the struct but not to `properties`, it silently becomes invisible to the schema. If a field is listed in `properties` but doesn't exist, the macro catches it — but the inverse (forgotten field) is not caught.

### 2.4 The id/type field requirement

The macro requires every base struct to have a GTS identity field. On `main`, `validate_field_types` (line 260) enforces:

```
Base structs must have either an ID field (one of: $id, id, gts_id, gtsId) of type
GtsInstanceId OR a GTS Type field (one of: type, gts_type, gtsType, schema) of type
GtsSchemaId
```

The requirement itself is correct and carried forward. What changes is the mechanism: field-name matching (`$id`, `id`, `gts_id`, `gtsId`, `type`, `gts_type`, ...) is replaced by explicit `#[gts(type_field)]` / `#[gts(instance_id)]` annotations (see [ADR-001](./001-gts-schema-derive-macro-adr.md) for the full design). The name-matching also conflated presence with serde behavior, which is the root cause behind Issue #72.

### 2.5 The parallel serialization system

For nested structs, the macro creates a shadow serialization system:

- `GtsSerialize` / `GtsDeserialize` — parallel traits to `serde::Serialize` / `serde::Deserialize`
- `GtsSerializeWrapper` / `GtsDeserializeWrapper` — bridge types between the two systems
- `GtsNoDirectSerialize` / `GtsNoDirectDeserialize` — marker traits that cause compile errors if serde traits are also implemented
- `serialize_gts` / `deserialize_gts` — helper functions used via `#[serde(serialize_with, deserialize_with)]`

This exists to solve a real problem: a nested payload struct like `AuditPayloadV1` produces incomplete JSON when serialized alone (it lacks the base event fields). The system is preserved in the redesign — the nested-blocking guarantee remains load-bearing. What changes is that the derives users write (and the ones the macro generates) move into the struct definition where they are visible.

---

## 3. Spec alignment motivation

The GTS specification (v0.8) informs several aspects of the redesign. The sections below cover the spec behaviors the macro must faithfully reflect.

### 3.1 The `type` field is a polymorphic discriminator, not a static constant

The base event schema (`gts.x.core.events.type.v1~`) defines its `type` property as:

```json
"type": {
  "description": "Identifier of the event type in GTS format.",
  "type": "string",
  "x-gts-ref": "/$id"
}
```

The `x-gts-ref: "/$id"` annotation means the field value references the current schema's `$id`. In a child schema (`gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~`), this is narrowed to:

```json
"type": {
  "const": "gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~"
}
```

The `type` value is the **child's** full chained schema ID — not the base struct's own `SCHEMA_ID`. This is visible in the spec's instance examples:

```json
{
  "type": "gts.x.core.events.type.v1~x.commerce.orders.order_placed.v1.0~",
  "id": "7a1d2f34-5678-49ab-9012-abcdef123456"
}
```

And in the macro's own README examples:

```rust
let event = BaseEventV1 {
    event_type: PlaceOrderDataV1::gts_schema_id().clone(),  // child's ID, not base's
    ...
};
```

**Conclusion**: The `type` field is a runtime value set by the user — it depends on which concrete child type the instance represents. It is not something the macro can or should auto-populate from the struct's own `SCHEMA_ID`.

### 3.2 The `id` field follows different patterns depending on entity type

The spec shows multiple patterns for carrying GTS identity on an instance:

| Schema | `id` field | `type`/`gtsId` field | Pattern |
|---|---|---|---|
| `events.type.v1~` | UUID | Full chained schema ID | Anonymous instance |
| `events.type_combined.v1~` | Chained GTS ID + UUID | (omitted) | Combined anonymous |
| `events.topic.v1~` | GTS instance ID | (none) | Well-known instance |
| `compute.vm.v1~` | UUID | `gtsId`: schema ID | Hybrid |
| `compute.vm_state.v1~` | (none) | `gtsId`: schema ID | Well-known (type-only) |

**Conclusion**: The `id` field can be either (a) a plain identifier (UUID, string) for anonymous instances carried alongside a `type` field of `GtsSchemaId`, or (b) a `GtsInstanceId` for well-known instances. The macro supports both patterns via `#[gts(type_field)]` and `#[gts(instance_id)]` respectively. Exactly one of the two must appear on every **root** struct (no `extends`) so GTS type is always deducible from the instance — required for the Type Registry, RPC/MCP APIs, and Event streams. Derived structs inherit their GTS type from the root struct's chained identifier and do not carry an identity field of their own.

### 3.3 Field names are implementation-defined

From [spec §11.1](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories):

> *"The exact field names used for instance IDs and instance types are **implementation-defined** and may be **configuration-driven** (different systems may look for identifiers in different fields)."*

And [spec §9.1](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema):

> *"Field naming: typically `id` (alternatives: `gtsId`, `gts_id`)"*
> *"Field naming: `type` (alternatives: `gtsType`, `gts_type`)"*

The macro supports arbitrary field names via the per-field `#[gts(type_field)]` / `#[gts(instance_id)]` annotations, so callers are not constrained to a fixed set of field names.

---


## 4. Migration walkthrough

### 4.1 Before (current)

```rust
#[derive(Debug)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type definition",
    properties = "event_type,id,tenant_id,sequence_id,payload"
)]
pub struct BaseEventV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

#[derive(Debug)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event with user context",
    properties = "user_agent,user_id,ip_address,data"
)]
pub struct AuditPayloadV1<D> {
    pub user_agent: String,
    pub user_id: Uuid,
    pub ip_address: String,
    pub data: D,
}
```

### 4.2 After (new)

```rust
#[derive(Debug, Clone, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type definition",
)]
pub struct BaseEventV1<P: GtsSchema> {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}

#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event with user context",
    extends = BaseEventV1,
)]
pub struct AuditPayloadV1<D: GtsSchema> {
    pub user_agent: String,
    pub user_id: Uuid,
    pub ip_address: String,
    pub data: D,
}
```

Note that the generic root (`BaseEventV1<P>`) does **not** derive `Serialize` / `Deserialize` — the macro emits those impls itself so that nested payloads route through the `GtsSerialize` / `GtsDeserialize` bridge. Non-generic root structs (see §4.3 below) derive `Serialize` / `Deserialize` normally.

### 4.3 The issue #72 case — identity field keeps working with Deserialize

Issue #72 surfaced because the current macro's identity handling breaks `Deserialize`: a `gts_type: GtsSchemaId` field is required at the struct level but the generated serde configuration can't round-trip it, forcing users into `#[serde(skip_serializing, default = "dummy_fn")]` workarounds. The migration does not make identity fields optional — they remain mandatory ([see ADR-001](./001-gts-schema-derive-macro-adr.md)). The fix is in the serde configuration: the identity field now round-trips cleanly.

```rust
// BEFORE: identity field blocks Deserialize, requires fragile serde workaround
#[derive(Debug, Clone)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.cf.core.errors.quota_violation.v1~",
    description = "A single quota violation entry",
    properties = "subject,description"
)]
pub struct QuotaViolationV1 {
    #[allow(dead_code)]
    #[serde(skip_serializing, default = "dummy_gts_schema_id")]
    gts_type: gts::GtsSchemaId,       // required, but breaks Deserialize (#72)
    pub subject: String,
    pub description: String,
}

// AFTER: identity field annotated explicitly, serde round-trips cleanly
#[derive(Debug, Clone, Serialize, Deserialize, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.cf.core.errors.quota_violation.v1~",
    description = "A single quota violation entry",
)]
pub struct QuotaViolationV1 {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub gts_type: GtsSchemaId,        // required; macro populates it via new()
    pub subject: String,
    pub description: String,
}
```

The `#[gts(type_field)]` annotation both (a) tells the macro to annotate the JSON Schema property with `"x-gts-ref": "/$id"`, and (b) wires the field into the generated `new(...)` constructor so callers never hand-assign it. Users can construct a quota violation with `QuotaViolationV1::new(subject, description)` — the `gts_type` is filled in from `Self::SCHEMA_ID` automatically. The `#[serde(skip_serializing, default = "dummy_fn")]` workaround is no longer needed because the generated serde configuration round-trips the identity field correctly.

---

## 5. At-a-glance diff

| Concern | Current behavior | Proposed behavior |
|---|---|---|
| **Macro entry point** | Single `#[struct_to_gts_schema]` attribute macro | `#[derive(GtsSchema)]` derive macro + `#[gts(...)]` attributes |
| **Schema identity** | Provided via `schema_id` param | Same — via `#[gts(schema_id = "...")]` |
| **Inheritance** | `base = true` / `base = Parent` | Absent or `extends = None` (root) / `extends = Parent` (child) |
| **Properties list** | Manual `properties = "a,b,c"` | Automatic from struct fields; `#[gts(skip)]` to exclude |
| **Id/type fields** | Required on all base structs | Required; exactly one of `#[gts(type_field)]` / `#[gts(instance_id)]` (mutually exclusive) |
| **Constructor** | Not generated | `new(...)` generated, auto-populates `#[gts(type_field)]` from `SCHEMA_ID` / generic |
| **Serde derives** | Silently injected (base) or removed (nested) | User-writes them explicitly (base); prohibited by macro (nested) |
| **Serde attributes on generic fields** | Silently injected | Retained (necessary), but documented |
| **Nested struct serialization blocking** | Always blocked via marker traits | Always blocked via marker traits (no opt-out) |
| **GtsSerialize/GtsDeserialize** | Explicit impls generated for nested structs | Retained — explicit impls still generated for nested structs |
| **`GtsSchema` trait** | Implemented by macro | Same — implemented by `#[derive(GtsSchema)]` |
| **Runtime API** | Generated methods on struct | Same — generated by derive; `gts_instance_json()` gated on `Self: Serialize` |
| **CLI schema generation** | Uses `dir_path` and `properties` | Uses `dir_path`; properties derived from fields |
| **JsonSchema derive** | Auto-added | Must be derived explicitly by the caller. `#[derive(GtsSchema)]` uses `schemars::schema_for!(Self)` internally, so omitting `#[derive(JsonSchema)]` produces an immediate, localized compile error at the struct definition. |

---

## 6. Schema output parity

A key concern: does the migration change the generated JSON Schemas?

### 6.1 Structurally identical output

With correct migration (adding `#[gts(skip)]` to fields previously omitted from `properties`), the generated schemas are **structurally identical**:

- `$id`, `$schema`, `title`, `type: "object"` — unchanged
- `properties` object — same fields included
- `required` array — derived the same way (non-`Option` fields are required)
- `additionalProperties: false` — unchanged
- `allOf` + `$ref` structure for child types — unchanged
- Generic field nesting via `wrap_in_nesting_path` — unchanged
- `GtsInstanceId`/`GtsSchemaId` field representation — unchanged (via `json_schema_value()`)

### 6.2 One improvement: spec-correct `x-gts-ref` on identity fields

The current macro generates all `GtsSchemaId` fields with a generic `x-gts-ref`:

```json
"type": { "type": "string", "format": "gts-schema-id", "x-gts-ref": "gts.*" }
```

But the GTS spec examples ([§9.6](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support)) use a more precise self-reference annotation on identity fields — `"x-gts-ref": "/$id"` — meaning "this field's value must equal the current schema's `$id`":

```json
"type": { "type": "string", "x-gts-ref": "/$id" }
```

This distinction matters. Consider a base event schema with two `GtsSchemaId` fields:
- `type` — identifies *this* entity's schema (should be `"x-gts-ref": "/$id"`)
- `subjectType` — references *another* entity's schema (should be `"x-gts-ref": "gts.*"`)

The current macro treats both identically. The new field-level attributes fix this:

```rust
#[gts(type_field)]
#[serde(rename = "type")]
pub event_type: GtsSchemaId,    // → "x-gts-ref": "/$id"

pub subject_type: GtsSchemaId,  // → "x-gts-ref": "gts.*" (from schemars JsonSchema impl)
```

This brings the generated schemas closer to the spec examples:
- `events.type.v1~` schema: `"type"` property has `"x-gts-ref": "/$id"` (spec ref: [events.type.v1~.schema.json](/.gts-spec/examples/events/schemas/gts.x.core.events.type.v1~.schema.json))
- `events.topic.v1~` schema: `"id"` property has `"x-gts-ref": "/$id"` (spec ref: [events.topic.v1~.schema.json](/.gts-spec/examples/events/schemas/gts.x.core.events.topic.v1~.schema.json))
- `modules.capability.v1~` schema: `"id"` property has `"x-gts-ref": "/$id"` (spec ref: [capability.v1~.schema.json](/.gts-spec/examples/modules/schemas/gts.x.core.modules.capability.v1~.schema.json))

### 6.3 Summary

| Aspect | Change? | Notes |
|---|---|---|
| Schema structure (`$id`, `allOf`, `$ref`, `properties`, `required`) | No | Identical output |
| `additionalProperties` | No | Same behavior |
| `GtsSchemaId` / `GtsInstanceId` field representation | No | Same `json_schema_value()` |
| `x-gts-ref` on `#[gts(type_field)]` / `#[gts(instance_id)]` fields | Yes — improved | Changes from `"gts.*"` to `"/$id"`, matching spec examples |
| `x-gts-ref` on other GTS fields (e.g., `subjectType`) | No | Retains `"gts.*"` from schemars impl |

---

## 7. Pre-existing bugs fixed

The following are pre-existing issues that the **new** macro (`#[derive(GtsSchema)]`) fixes. The **old** macro (`#[struct_to_gts_schema]`) is unchanged and retains its current behavior — coexistence means the old macro's existing behavior is frozen.

1. **Fix nested struct deserializer field renaming**: The old macro's `GtsDeserialize` impl for nested structs generates a field identifier enum with `#[serde(field_identifier, rename_all = "snake_case")]`. This assumes all incoming JSON field names are snake_case. Fields with `#[serde(rename = "someOtherName")]` are handled correctly during serialization (via `get_serde_rename()`), but the `rename_all = "snake_case"` on the field identifier enum does not correctly match camelCase or other conventions in incoming JSON. The new macro generates the field identifier enum with explicit per-field `#[serde(rename = "...")]` attributes that respect the user's serde renames, rather than applying a blanket `rename_all`. The old macro continues to emit the `rename_all = "snake_case"` form as existing documented behavior.

2. **Include `description` in runtime schemas**: The `description` parameter is stored as `GTS_SCHEMA_DESCRIPTION` under both macros, but the old macro's runtime-generated schemas (via `gts_schema_with_refs()`) omit it, even though the GTS spec example schemas consistently include `description` (e.g., `events.type.v1~.schema.json`, `events.topic.v1~.schema.json`, `compute.vm.v1~.schema.json`). The new macro includes `description` in runtime-generated schemas to match the spec. The old macro's runtime output is unchanged.

## 8. Compile-fail test migration

The current macro has 31 compile-fail tests. The redesign affects their status:

Mapping from old macro compile-fail behavior to the new macro's fixtures under `gts-macros/tests/v2_compile_fail/` (left column is the behavior the old macro rejected; right column is the new fixture and its status):

| Old macro behavior | New `v2_compile_fail/` fixture | Status |
|---|---|---|
| Base struct missing id/type field | `missing_identity_annotation` | **Retained** — reframed as "must have `#[gts(type_field)]` or `#[gts(instance_id)]`" |
| Id field wrong type (not `GtsInstanceId`) | `instance_id_wrong_type` | **Retained** — validates that `#[gts(instance_id)]` field is `GtsInstanceId` |
| Type field wrong type (not `GtsSchemaId`) | `type_field_wrong_type` | **Retained** — validates that `#[gts(type_field)]` field is `GtsSchemaId` |
| Both id and type field present | `both_type_and_instance` | **Retained** — `#[gts(type_field)]` and `#[gts(instance_id)]` are mutually exclusive |
| Nested struct with direct `Serialize`/`Deserialize` | `nested_direct_serialize` | **Retained** — nested `Serialize` always blocked, no opt-out |
| Nested struct with direct serde via `cfg_attr` | `nested_direct_serialize_cfg_attr` | **Retained** |
| Struct name / schema-id version mismatch | `version_mismatch` | **Retained** as-is |
| Invalid GTS identifier format | `invalid_gts_id` | **Retained** as-is |
| `base = true` with multi-segment schema_id | `root_multi_segment` | **Retained** — root type (no `extends` / `extends = None`) with multi-segment errors similarly |
| Parent segment in schema_id doesn't match `Parent::SCHEMA_ID` | `extends_parent_mismatch` | **Retained** as-is |
| Parent struct has no generic parameter | `extends_parent_no_generic` | **Retained** as-is |
| Struct has more than one generic parameter | `multiple_generics` | **Retained** as-is |
| `extends = Parent` with single-segment schema_id | `extends_single_segment` | **Retained** — parent-only segment without own segment |
| Tuple struct annotated with the macro | `tuple_struct` | **Retained** as-is |
| Enum annotated with the macro | `enum_not_supported` | **New** — enums explicitly rejected |
| Missing `schema_id` | `missing_schema_id` | **Retained** — still required in `#[gts(...)]` |
| Missing `description` | `missing_description` | **Retained** — still required in `#[gts(...)]` |
| Missing `dir_path` | `missing_dir_path` | **Retained** |
| Missing `properties` / property not in struct | *(no fixture)* | **Removed** — `properties` parameter eliminated |
| Unknown struct-level attribute | `unknown_gts_attr` | **Modified** — new attribute names (`extends`, `dir_path`, etc.) |
| Unknown field-level attribute | `unknown_field_attr` | **New** — field attribute grammar is closed |

**New fixtures specific to the redesign's identity-field rules:**

| Fixture | Validates |
|---|---|
| `missing_identity_annotation` | Root struct with no `#[gts(type_field)]` / `#[gts(instance_id)]` is rejected |
| `both_type_and_instance` | A single struct cannot declare both `#[gts(type_field)]` and `#[gts(instance_id)]` |
| `duplicate_type_field` | Two fields on the same struct both carrying `#[gts(type_field)]` is rejected |
| `duplicate_instance_id` | Two fields on the same struct both carrying `#[gts(instance_id)]` is rejected |
| `identity_on_derived_struct` | A derived struct (`extends = Parent`) cannot carry `#[gts(type_field)]` / `#[gts(instance_id)]` |
| `type_field_wrong_type` | `#[gts(type_field)]` on a non-`GtsSchemaId` field is rejected |
| `instance_id_wrong_type` | `#[gts(instance_id)]` on a non-`GtsInstanceId` field is rejected |
| `extends_none_multi_segment` | `extends = None` with a multi-segment `schema_id` errors identically to the absent case |

Calling `gts_instance_json()` on a struct that does not derive `Serialize` is enforced by rustc itself (the generated body calls `serde_json::to_value(self)`), not by a macro-side trybuild fixture — so there is no compile-fail fixture for that case.


---

## 9. Coexistence, deprecation, and removal

Both macros coexist. `#[struct_to_gts_schema]` and `#[derive(GtsSchema)]` both compile; the old macro continues to work on existing code with no changes, and new code should use the derive.

Deprecation (`#[deprecated]` attribute, diagnostic banners, CHANGELOG entries) and eventual removal of `#[struct_to_gts_schema]` are **explicitly out of scope of the implementation plan** that introduced the derive — see [002-macro-migration-implementation-plan.md](./002-macro-migration-implementation-plan.md) §"Out of scope". They are gated on the cross-cutting effort to migrate all existing `#[struct_to_gts_schema]` callers, which is tracked separately and has to be coordinated across every downstream consumer. Turning deprecation on prematurely would flood in-tree and downstream builds with warnings nobody can act on yet.

In other words: coexistence is the stable steady state until the broader migration is complete. No calendar date, no staged rollout within this plan.
