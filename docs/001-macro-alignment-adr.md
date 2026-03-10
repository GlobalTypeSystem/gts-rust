# ADR-001: Align gts-rust Macro with GTS Specification

- **Status**: Proposed
- **Date**: 2026-03-06
- **Authors**: Andre Smith
- **Issue**: [#72 — struct_to_gts_schema: gts_type field blocks Deserialize for structs](https://github.com/GlobalTypeSystem/gts-rust/issues/72)

---

## 1. Context

The `#[struct_to_gts_schema]` attribute macro is the primary integration point between Rust structs and the Global Type System. It currently serves three purposes: compile-time validation, JSON Schema generation, and runtime API generation. While the macro delivers significant value, its monolithic design has led to accumulated friction as the GTS specification and user requirements have evolved.

Issue [#72](https://github.com/GlobalTypeSystem/gts-rust/issues/72) exposed a concrete symptom — the macro requires base structs to declare a `gts_type: GtsSchemaId` or `id: GtsInstanceId` field, but does not properly handle deserialization of that field. Users are forced into workarounds like `#[serde(skip_serializing, default = "dummy_fn")]`, which are fragile and pollute consumer code.

However, the root cause runs deeper than a missing `#[serde(skip)]`. The macro conflates multiple orthogonal concerns into a single attribute, makes assumptions that are not grounded in the GTS specification, and silently manipulates user-controlled serde behavior in ways that are difficult to reason about.

This ADR proposes a redesign that decomposes the macro into focused, composable units that align with the GTS specification and follow Rust ecosystem conventions.

---

## 2. Current Design

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

These are separate concerns. A root GTS type might not need direct serialization. A child type might need standalone serialization for testing or debugging. The current design couples them.

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

This is not grounded in the GTS specification (see [Section 3](#3-motivation-from-gts-specification) below).

### 2.5 The parallel serialization system

For nested structs, the macro creates a shadow serialization system:

- `GtsSerialize` / `GtsDeserialize` — parallel traits to `serde::Serialize` / `serde::Deserialize`
- `GtsSerializeWrapper` / `GtsDeserializeWrapper` — bridge types between the two systems
- `GtsNoDirectSerialize` / `GtsNoDirectDeserialize` — marker traits that cause compile errors if serde traits are also implemented
- `serialize_gts` / `deserialize_gts` — helper functions used via `#[serde(serialize_with, deserialize_with)]`

This exists to solve a real problem: a nested payload struct like `AuditPayloadV1` produces incomplete JSON when serialized alone (it lacks the base event fields). But the solution enforces a type-level restriction for what is fundamentally a usage concern, and creates significant cognitive overhead.

---

## 3. Motivation from GTS Specification

The GTS specification (v0.8) provides clear guidance that contradicts several assumptions baked into the current macro design.

### 3.1 Identity fields are not universally required

The spec defines five categories of JSON documents (spec section 11.1, Rule C):

1. **GTS entity schemas** — have `$schema` and `$id` starting with `gts://`
2. **Non-GTS schemas** — have `$schema` but no GTS `$id`
3. **Instances of unknown/non-GTS schemas** — no `$schema`, no determinable GTS identity
4. **Well-known GTS instances** — identified by a GTS instance ID in an `id` field
5. **Anonymous GTS instances** — opaque `id` (UUID), GTS type in a separate `type` field

Categories 4 and 5 require GTS identity fields. But the spec examples include schemas that produce instances with **no GTS identity field at all**:

- `gts.x.commerce.orders.order.v1.0~` — the Order schema has `id: uuid` (a plain business identifier, not a `GtsInstanceId`) and no `type` field
- `gts.x.core.idp.contact.v1.0~` — the Contact schema has `id: uuid` and no `type` field

These are valid GTS schemas — they have a `$id` in their schema document — but their instances are pure data objects. They are referenced by other GTS entities (e.g., an event's `subjectType` points to `gts.x.commerce.orders.order.v1.0~`) but they don't self-identify at the instance level via GTS.

**Conclusion**: The macro's requirement that every base struct must have a `GtsInstanceId` or `GtsSchemaId` field is not justified by the spec.

### 3.2 The `type` field is a polymorphic discriminator, not a static constant

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

### 3.3 The `id` field follows different patterns depending on entity type

The spec shows multiple patterns:

| Schema | `id` field | `type`/`gtsId` field | Pattern |
|---|---|---|---|
| `events.type.v1~` | UUID | Full chained schema ID | Anonymous instance |
| `events.type_combined.v1~` | Chained GTS ID + UUID | (omitted) | Combined anonymous |
| `events.topic.v1~` | GTS instance ID | (none) | Well-known instance |
| `compute.vm.v1~` | UUID | `gtsId`: schema ID | Hybrid |
| `compute.vm_state.v1~` | (none) | `gtsId`: schema ID | Well-known (type-only) |
| `orders.order.v1.0~` | UUID | (none) | Plain data entity |
| `idp.contact.v1.0~` | UUID | (none) | Plain data entity |

**Conclusion**: There is no single "correct" identity pattern. The choice is domain-specific and implementation-defined (spec section 11.1). The macro should support these patterns, not mandate one.

### 3.4 Field names are implementation-defined

From spec section 11.1:

> *"The exact field names used for instance IDs and instance types are **implementation-defined** and may be **configuration-driven** (different systems may look for identifiers in different fields)."*

And spec section 9.1:

> *"Field naming: typically `id` (alternatives: `gtsId`, `gts_id`)"*
> *"Field naming: `type` (alternatives: `gtsType`, `gts_type`)"*

The macro already supports multiple field name variants. This is appropriate, but the rigid requirement around field presence is not.

### 3.5 Schema identity lives at the schema level, not the instance level

Every GTS schema has a `$id` in its JSON Schema document. The macro's `schema_id` attribute captures this. The generated `SCHEMA_ID` constant, `gts_schema_id()` method, and `gts_make_instance_id()` method provide runtime access.

Instance-level identity (the `id` or `type` field on a JSON object) is a separate concern — it's how a specific JSON document identifies itself at runtime. Not every schema produces instances that need self-identification.

**Conclusion**: Schema-level identity (the `schema_id` macro attribute) and instance-level identity (the `id`/`type` struct field) should be treated as independent concerns.

---

## 4. Proposed Design

### 4.1 Decompose into focused macros

Replace the single `#[struct_to_gts_schema]` with composable, single-responsibility macros and field-level attributes:

#### `#[derive(GtsSchema)]` — Schema identity and metadata

A derive macro that handles the pure GTS metadata concern:

```rust
#[derive(Debug, Clone, GtsSchema)]
#[gts(
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type with common fields",
)]
pub struct BaseEventV1<P: GtsSchema> {
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}
```

**Responsibilities:**
- Validate the `schema_id` format against GTS identifier rules
- Validate version consistency between struct name and schema ID
- Implement the `GtsSchema` trait (`SCHEMA_ID`, `GENERIC_FIELD`, schema composition methods)
- Generate `gts_schema_id()`, `gts_base_schema_id()`, `gts_make_instance_id()`
- Generate `gts_schema_with_refs_as_string()` and similar convenience methods
- Store `dir_path` and `description` as associated constants for CLI schema generation

**Automatic derives:**
- The derive macro automatically adds `schemars::JsonSchema` if not already present. This is required because the `GtsSchema` trait implementation uses `schemars::schema_for!(Self)` internally for runtime schema generation. Unlike `Serialize`/`Deserialize`, `JsonSchema` is a direct dependency of the GTS schema system and cannot be meaningfully omitted.

**What it does NOT do:**
- No `Serialize`/`Deserialize` injection or removal — user-controlled
- No serde attribute manipulation (except on generic fields — see [4.5](#45-gts-aware-serde-for-generic-fields))
- No field requirements (no mandatory id/type)
- No `properties` parameter — the schema is derived from the struct's fields (see [4.2](#42-remove-the-properties-parameter))

#### `#[gts(extends = ParentStruct)]` — Inheritance declaration

An attribute within the `#[gts(...)]` namespace that declares parent-child relationships:

```rust
#[derive(Debug, Clone, GtsSchema)]
#[gts(
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

**Responsibilities:**
- Validate that the schema ID has the correct segment count (multi-segment for child types)
- Validate at compile time that the parent's `SCHEMA_ID` matches the parent segment in `schema_id`
- Validate that the parent struct has exactly one generic parameter
- Generate `allOf` + `$ref` schema composition

**What it does NOT do:**
- Does not control serialization behavior — that remains the user's choice (see [4.4](#44-let-users-control-serde))

#### Field-level attributes — Opt-in GTS semantics

Instead of requiring id/type fields and hardcoding field-name recognition, provide opt-in field-level attributes:

```rust
#[gts(type_field)]              // Marks this as the GTS type discriminator
pub event_type: GtsSchemaId,

#[gts(instance_id)]             // Marks this as the GTS instance ID
pub id: GtsInstanceId,

#[gts(skip)]                    // Exclude from generated JSON schema
pub internal_cache: HashMap<String, String>,
```

**`#[gts(type_field)]`:**
- Validates that the field type is `GtsSchemaId`
- In the generated JSON Schema, annotates the property with `"x-gts-ref": "/$id"` per spec section 9.6
- Can only appear once per struct, and is mutually exclusive with `#[gts(instance_id)]`

**`#[gts(instance_id)]`:**
- Validates that the field type is `GtsInstanceId`
- In the generated JSON Schema, annotates the property with `"x-gts-ref": "/$id"` per spec section 9.6
- Can only appear once per struct, and is mutually exclusive with `#[gts(type_field)]`

**`#[gts(skip)]`:**
- Excludes the field from the generated JSON Schema properties
- Does not affect serde behavior (use `#[serde(skip)]` for that)

These attributes are **all optional**. A struct without any `#[gts(type_field)]` or `#[gts(instance_id)]` annotation is valid — it represents a data entity like `order.v1.0~` or `contact.v1.0~` from the spec.

### 4.2 Remove the `properties` parameter

The current `properties = "event_type,id,tenant_id,payload"` parameter manually lists which fields appear in the schema. This is redundant with the struct definition itself.

**New behavior**: All named fields are included in the JSON Schema by default. To exclude a field, use `#[gts(skip)]` (schema-only exclusion) or `#[serde(skip)]` (also excluded from serialization).

```rust
#[derive(Debug, Clone, Serialize, Deserialize, GtsSchema)]
#[gts(
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type",
)]
pub struct BaseEventV1<P: GtsSchema> {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,

    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,

    #[gts(skip)]
    pub internal_metadata: Option<String>,  // not in schema, but still serializable
}
```

**Migration**: The `properties` parameter is removed. Fields previously omitted from `properties` should add `#[gts(skip)]`. The `dir_path` parameter is retained as it controls file output location for the CLI.

### 4.3 Replace `base` with `extends`

The current `base` attribute has two forms:
- `base = true` — "this is a root type"
- `base = ParentStruct` — "this inherits from ParentStruct"

The `base = true` case carries no information — it simply means "not a child." This is the default state and doesn't need to be declared. The child case is better expressed as `extends = ParentStruct`, which reads naturally and only appears when needed.

```rust
// Root type — no `extends`, this is the default
#[derive(GtsSchema)]
#[gts(schema_id = "gts.x.core.events.type.v1~", description = "...")]
pub struct BaseEventV1<P: GtsSchema> { ... }

// Child type — declares parent explicitly
#[derive(GtsSchema)]
#[gts(
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "...",
    extends = BaseEventV1,
)]
pub struct AuditPayloadV1<D: GtsSchema> { ... }
```

**Validation rules remain the same:**
- Without `extends`: schema ID must have exactly 1 segment
- With `extends = Parent`: schema ID must have 2+ segments, and the parent segment must match `Parent::SCHEMA_ID`

### 4.4 Let users control serde

The current macro silently adds `Serialize`/`Deserialize` derives for base structs and silently removes them for nested structs. This is surprising behavior that fights against Rust conventions.

**New behavior**: The macro does **not** inject or remove any serde derives. Users explicitly control their serialization:

```rust
// Base struct — user adds Serialize/Deserialize themselves
#[derive(Debug, Serialize, Deserialize, GtsSchema)]
#[gts(schema_id = "gts.x.core.events.type.v1~", description = "...")]
pub struct BaseEventV1<P: GtsSchema> { ... }

// Nested struct — user decides whether it's directly serializable
#[derive(Debug, GtsSchema)]
#[gts(
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "...",
    extends = BaseEventV1,
)]
pub struct AuditPayloadV1<D: GtsSchema> { ... }
```

**Impact on nested struct serialization:**

The current design blocks nested structs from implementing `Serialize`/`Deserialize` to prevent users from accidentally serializing incomplete JSON. This is a valid safety concern, but the current approach is heavy-handed.

**Decision: Retain current blocking as default, with opt-out.** Nested structs (those with `extends`) will continue to be blocked from deriving `Serialize`/`Deserialize` by default, since direct serialization produces incomplete JSON and is a real source of bugs. Users who understand the tradeoff (testing, debugging, standalone use cases) can opt out:

```rust
#[derive(Debug, Serialize, Deserialize, GtsSchema)]
#[gts(
    schema_id = "...",
    extends = BaseEventV1,
    allow_direct_serde,       // opt-out: allow Serialize/Deserialize
)]
pub struct AuditPayloadV1<D: GtsSchema> { ... }
```

Without `allow_direct_serde`, deriving `Serialize`/`Deserialize` on a nested struct will produce a compile error directing the user to either remove the derives or add `allow_direct_serde`.

The `GtsSerialize`/`GtsDeserialize` trait system is retained regardless. It remains necessary for bridging generic fields in base structs where the generic parameter only implements `GtsSerialize`, not `serde::Serialize`. The key change is that users control whether a struct *also* implements serde directly.

**How serialization works without `Serialize` on nested structs:**

When a nested struct does not derive `Serialize`/`Deserialize` (the default), it can still be serialized through the base struct. The chain works as follows:

1. The base struct (e.g., `BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>`) derives `Serialize` and has `#[serde(serialize_with = "gts::serialize_gts")]` on its generic field
2. `serialize_gts` calls `GtsSerialize::gts_serialize()` on the nested struct
3. The macro generates an explicit `GtsSerialize` impl for the nested struct (this is not the blanket impl — it's a custom implementation that handles generic field wrapping via `GtsSerializeWrapper`)
4. The same applies in reverse for deserialization via `GtsDeserialize`

The macro **must** continue generating explicit `GtsSerialize`/`GtsDeserialize` implementations for nested structs. Without `Serialize`/`Deserialize` derives, the blanket impls (`impl<T: Serialize> GtsSerialize for T`) do not apply, so these explicit impls are the only path for nested struct serialization.

**Instance serialization methods (`gts_instance_json`, etc.):**

The current macro generates `gts_instance_json()`, `gts_instance_json_as_string()`, and `gts_instance_json_as_string_pretty()` for base structs. These call `serde_json::to_value(self)` which requires `Serialize`. Since the new design does not inject `Serialize`, these methods must be generated with a `where Self: serde::Serialize` bound so they are only available when the user has derived `Serialize`. If `Serialize` is not derived, the methods simply won't exist — no compile error unless the user tries to call them.

**Unit struct handling:**

The current macro provides custom `Serialize`/`Deserialize` implementations for unit structs (both base and nested) that serialize as `{}` instead of `null` and accept both `{}` and `null` on deserialization. This behavior is retained in the new design:
- Base unit structs: custom `Serialize`/`Deserialize` impls generated by the macro
- Nested unit structs: custom `GtsSerialize`/`GtsDeserialize` impls generated by the macro

### 4.5 GTS-aware serde for generic fields

The current macro injects `#[serde(bound(...))]` and `#[serde(serialize_with, deserialize_with)]` attributes on base structs with generic parameters. This is necessary because the generic parameter `P` may only implement `GtsSerialize`/`GtsDeserialize` (not direct serde traits), and serde needs to be told how to handle it.

This behavior is retained, but made explicit. When the derive macro detects a generic field of type `P` where `P: GtsSchema`, it adds the appropriate serde bounds and delegation attributes. This is the one case where the macro manipulates serde attributes, and it is justified because the `GtsSchema` bound is user-declared and the serde bridging is a direct consequence of the GTS type system.

The macro should emit a note in documentation (or via `#[doc]`) explaining what serde attributes were added and why.

---

## 5. Migration Path

### 5.1 Before (current)

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

### 5.2 After (proposed)

```rust
#[derive(Debug, Serialize, Deserialize, GtsSchema)]
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

#[derive(Debug, GtsSchema)]
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

### 5.3 The issue #72 case — data entity without GTS identity

```rust
// BEFORE: forced to add dead gts_type field with fragile serde workaround
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
    gts_type: gts::GtsSchemaId,       // unwanted field, serde workaround
    pub subject: String,
    pub description: String,
}

// AFTER: clean data entity, no GTS identity field needed
#[derive(Debug, Clone, Serialize, Deserialize, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.cf.core.errors.quota_violation.v1~",
    description = "A single quota violation entry",
)]
pub struct QuotaViolationV1 {
    pub subject: String,
    pub description: String,
}
```

---

## 6. Summary of Changes

| Concern | Current behavior | Proposed behavior |
|---|---|---|
| **Macro entry point** | Single `#[struct_to_gts_schema]` attribute macro | `#[derive(GtsSchema)]` derive macro + `#[gts(...)]` attributes |
| **Schema identity** | Provided via `schema_id` param | Same — via `#[gts(schema_id = "...")]` |
| **Inheritance** | `base = true` / `base = Parent` | Absent (default = root) / `extends = Parent` |
| **Properties list** | Manual `properties = "a,b,c"` | Automatic from struct fields; `#[gts(skip)]` to exclude |
| **Id/type fields** | Required on all base structs | Optional; opt-in via `#[gts(type_field)]` / `#[gts(instance_id)]` |
| **Serde derives** | Silently injected (base) or removed (nested) | User-controlled; macro does not add or remove derives |
| **Serde attributes on generic fields** | Silently injected | Retained (necessary), but documented |
| **Nested struct serialization blocking** | Always blocked via marker traits | Blocked by default; opt-out via `#[gts(allow_direct_serde)]` |
| **GtsSerialize/GtsDeserialize** | Explicit impls generated for nested structs | Retained — explicit impls still generated for nested structs |
| **`GtsSchema` trait** | Implemented by macro | Same — implemented by `#[derive(GtsSchema)]` |
| **Runtime API** | Generated methods on struct | Same — generated by derive; `gts_instance_json()` gated on `Self: Serialize` |
| **CLI schema generation** | Uses `dir_path` and `properties` | Uses `dir_path`; properties derived from fields |
| **JsonSchema derive** | Auto-added | Auto-added by `#[derive(GtsSchema)]` (required for `schemars::schema_for!`) |

---

## 7. Schema Output Impact

A key concern: does the redesign change the generated JSON Schemas?

### 7.1 Structurally identical output

With correct migration (adding `#[gts(skip)]` to fields previously omitted from `properties`), the generated schemas are **structurally identical**:

- `$id`, `$schema`, `title`, `type: "object"` — unchanged
- `properties` object — same fields included
- `required` array — derived the same way (non-`Option` fields are required)
- `additionalProperties: false` — unchanged
- `allOf` + `$ref` structure for child types — unchanged
- Generic field nesting via `wrap_in_nesting_path` — unchanged
- `GtsInstanceId`/`GtsSchemaId` field representation — unchanged (via `json_schema_value()`)

### 7.2 One improvement: spec-correct `x-gts-ref` on identity fields

The current macro generates all `GtsSchemaId` fields with a generic `x-gts-ref`:

```json
"type": { "type": "string", "format": "gts-schema-id", "x-gts-ref": "gts.*" }
```

But the GTS spec examples (section 9.6) use a more precise self-reference annotation on identity fields — `"x-gts-ref": "/$id"` — meaning "this field's value must equal the current schema's `$id`":

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

### 7.3 Summary

| Aspect | Change? | Notes |
|---|---|---|
| Schema structure (`$id`, `allOf`, `$ref`, `properties`, `required`) | No | Identical output |
| `additionalProperties` | No | Same behavior |
| `GtsSchemaId` / `GtsInstanceId` field representation | No | Same `json_schema_value()` |
| `x-gts-ref` on `#[gts(type_field)]` / `#[gts(instance_id)]` fields | Yes — improved | Changes from `"gts.*"` to `"/$id"`, matching spec examples |
| `x-gts-ref` on other GTS fields (e.g., `subjectType`) | No | Retains `"gts.*"` from schemars impl |

---

## 8. Decisions

1. **Nested struct serialization policy**: Retain current blocking as default, with opt-out via `#[gts(allow_direct_serde)]`. See [section 4.4](#44-let-users-control-serde).

2. **Backwards compatibility**: `#[struct_to_gts_schema]` will be deprecated. Both the old and new macros will coexist during a migration period. The old macro will emit a deprecation warning pointing users to the migration guide.

3. **`dir_path` location**: Remains per-struct for now. A future enhancement may support crate-level configuration with per-struct overrides.

## 9. Decisions on Pre-existing Issues

The following are pre-existing issues in the current macro that will be addressed as part of the redesign:

1. **Fix nested struct deserializer field renaming**: The current macro's `GtsDeserialize` impl for nested structs generates a field identifier enum with `#[serde(field_identifier, rename_all = "snake_case")]`. This assumes all incoming JSON field names are snake_case. However, fields with `#[serde(rename = "someOtherName")]` are handled correctly during serialization (via `get_serde_rename()`), but the `rename_all = "snake_case"` on the field identifier enum does not correctly match camelCase or other conventions in incoming JSON. The redesign will generate the field identifier enum with explicit per-field `#[serde(rename = "...")]` attributes that respect the user's serde renames, rather than applying a blanket `rename_all`.

2. **Include `description` in runtime schemas**: The `description` parameter is currently stored as `GTS_SCHEMA_DESCRIPTION` but is only used by the CLI for file-based schema generation. The runtime-generated schemas (via `gts_schema_with_refs()`) omit it, even though the GTS spec example schemas consistently include `description` (e.g., `events.type.v1~.schema.json`, `events.topic.v1~.schema.json`, `compute.vm.v1~.schema.json`). The redesign will include `description` in runtime-generated schemas to match the spec.

## 10. Compile-Fail Test Migration

The current macro has 31 compile-fail tests. The redesign affects their status:

| Test | Current behavior | Redesign status |
|---|---|---|
| `base_struct_missing_id` | Error: must have id or type field | **Removed** — id/type fields no longer required |
| `base_struct_wrong_id_type` | Error: id must be `GtsInstanceId` | **Modified** — only validates if `#[gts(instance_id)]` is present |
| `base_struct_wrong_gts_type` | Error: type must be `GtsSchemaId` | **Modified** — only validates if `#[gts(type_field)]` is present |
| `base_struct_both_id_and_type` | Error: cannot have both | **Retained** — `#[gts(type_field)]` and `#[gts(instance_id)]` are mutually exclusive |
| `nested_direct_serialize` | Error: cannot derive Serialize | **Retained** — blocked by default; `allow_direct_serde` to opt out |
| `nested_direct_serialize_cfg_attr` | Error: same, via cfg_attr | **Retained** |
| Version mismatch tests (6 cases) | Error: version inconsistency | **Retained** as-is |
| Schema ID format tests | Error: invalid GTS identifier | **Retained** as-is |
| `base_true_multi_segment` | Error: base=true needs 1 segment | **Retained** — no `extends` with multi-segment errors similarly |
| `base_parent_mismatch` | Error: parent ID doesn't match | **Retained** as-is |
| `base_parent_no_generic` | Error: parent must have generic | **Retained** as-is |
| `multiple_type_generics` | Error: only 1 generic allowed | **Retained** as-is |
| `non_gts_generic` | Error: generic must impl GtsSchema | **Retained** as-is |
| `tuple_struct` | Error: not supported | **Retained** as-is |
| `missing_schema_id` | Error: required attribute | **Retained** — still required in `#[gts(...)]` |
| `missing_description` | Error: required attribute | **Retained** — still required in `#[gts(...)]` |
| `missing_file_path` | Error: dir_path required | **Retained** |
| `missing_properties` | Error: required attribute | **Removed** — `properties` parameter eliminated |
| `missing_property` | Error: property not in struct | **Removed** — `properties` parameter eliminated |
| `unknown_attribute` | Error: unrecognized attribute | **Modified** — new attribute names (`extends`, `type_field`, etc.) |

## 11. Open Questions

1. **CLI impact**: The CLI currently scans source files for `#[struct_to_gts_schema]` annotations to extract metadata (schema ID, properties, description). The new `#[derive(GtsSchema)]` + `#[gts(...)]` pattern requires updating the CLI parser. The removal of `properties` means the CLI must parse struct fields directly (respecting `#[gts(skip)]` and `#[serde(skip)]` attributes). This is deferred for a separate design discussion.

2. **`GTS_SCHEMA_PROPERTIES` constant**: ~~The current macro generates `GTS_SCHEMA_PROPERTIES: &'static str` as a comma-separated string derived from the `properties` parameter. With `properties` removed, this constant could either be auto-generated from struct fields or removed entirely.~~ **Resolved**: The derive macro will auto-generate this constant from the struct's field names, excluding fields with `#[gts(skip)]` or `#[serde(skip)]`. The struct fields are already available to the macro at compile time — no user input needed.

3. **Schema traits (`x-gts-traits-schema` / `x-gts-traits`)**: The GTS spec (section 9.7) defines a trait system for schema-level metadata — semantic annotations like retention rules, topic associations, and processing directives that are not part of the instance data model. The current macro does not generate these, and this ADR does not address them. Examples from the spec:

   - Base event schema defines a trait schema:
     ```json
     "x-gts-traits-schema": {
         "type": "object",
         "properties": {
             "topicRef": { "x-gts-ref": "gts.x.core.events.topic.v1~" },
             "retention": { "type": "string", "default": "P30D" }
         }
     }
     ```
   - Child schemas provide trait values (with immutability — once set by an ancestor, descendants cannot override):
     ```json
     "x-gts-traits": {
         "topicRef": "gts.x.core.events.topic.v1~x.commerce._.orders.v1",
         "retention": "P90D"
     }
     ```

   This is a significant spec feature that needs its own design. The macro redesign should be compatible with future trait support (e.g., via `#[gts(traits_schema = "...")]` or a separate derive), but the design is deferred.

---

## 12. References

- [GTS Specification v0.8](/.gts-spec/README.md)
  - Section 3.7 — Well-known and Anonymous Instances
  - Section 9.1 — Identifier reference in JSON and JSON Schema
  - Section 9.6 — `x-gts-ref` support
  - Section 11.1 — JSON document categories (Rule C)
  - Section 11.2 — JSON and JSON Schema examples
- [Issue #72 — struct_to_gts_schema: gts_type field blocks Deserialize](https://github.com/GlobalTypeSystem/gts-rust/issues/72)
- Spec examples:
  - [`order.v1.0~.schema.json`](/.gts-spec/examples/events/schemas/gts.x.commerce.orders.order.v1.0~.schema.json) — data entity without GTS identity field
  - [`contact.v1.0~.schema.json`](/.gts-spec/examples/events/schemas/gts.x.core.idp.contact.v1.0~.schema.json) — data entity without GTS identity field
  - [`events.type.v1~.schema.json`](/.gts-spec/examples/events/schemas/gts.x.core.events.type.v1~.schema.json) — anonymous instance with `type` field using `x-gts-ref: "/$id"`
  - [`events.topic.v1~.schema.json`](/.gts-spec/examples/events/schemas/gts.x.core.events.topic.v1~.schema.json) — well-known instance with `id` field using `x-gts-ref: "/$id"`
  - [`compute.vm.v1~.schema.json`](/.gts-spec/examples/typespec/vms/schemas/gts.x.infra.compute.vm.v1~.schema.json) — hybrid pattern with `gtsId` + UUID `id`
