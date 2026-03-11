# Proposal: Align gts-rust Macro with GTS Specification

**ADR**: [001-macro-alignment-adr.md](./001-macro-alignment-adr.md) | **Implementation Plan**: [001-macro-alignment-implementation-plan.md](./001-macro-alignment-implementation-plan.md)
**Issue**: [#72 - gts_type field blocks Deserialize](https://github.com/GlobalTypeSystem/gts-rust/issues/72)
**Branch**: `gts-macro-proposal`

---

## 1. Purpose

The `#[struct_to_gts_schema]` macro has been the primary integration point between Rust structs and the Global Type System. It delivers compile-time validation, JSON Schema generation, and a runtime API from a single annotation. As the GTS specification has matured and usage has grown, several areas have emerged where the macro's assumptions can be brought into closer alignment with the spec.

This proposal evolves the macro from `#[struct_to_gts_schema]` to `#[derive(GtsSchema)]` with `#[gts(...)]` attributes. The goals are:

- Align field and identity requirements with the GTS specification (v0.8)
- Support the full range of GTS document categories ([Spec §11.1, Rule C](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories))
- Enable spec-correct `x-gts-ref` annotations ([Spec §9.6](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support))
- Give users explicit control over serde derives while preserving safety defaults
- Structure the codebase for future spec features like schema traits ([§9.7](https://github.com/GlobalTypeSystem/gts-spec#97---schema-traits-x-gts-traits-schema--x-gts-traits))

All existing compile-time validations, runtime behavior, and schema output are preserved. The old macro continues to work alongside the new one during migration.

---

## 2. Opportunities for Alignment

### 2.1 Supporting all GTS document categories

The current macro requires every base struct to declare either a `GtsSchemaId` field (for anonymous instances) or a `GtsInstanceId` field (for well-known instances). This was a reasonable default when the macro was written, as the primary use case was event types that always carry identity fields.

However, the GTS specification (v0.8) defines **five** categories of JSON documents ([Spec §11.1, Rule C](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)). Only two of the five require identity fields:

| Category | Identity field required? | Example |
|---|---|---|
| 1. GTS entity schemas | No (identity is `$id` in the schema document) | Any `.schema.json` file |
| 2. Non-GTS schemas | No | Third-party JSON Schemas |
| 3. Instances of unknown/non-GTS schemas | No | Opaque JSON payloads |
| 4. **Well-known GTS instances** | **Yes** -- GTS instance ID in `id` field | Event topics, modules |
| 5. **Anonymous GTS instances** | **Yes** -- GTS type ID in `type` field | Events, audit records |

The spec includes concrete examples of GTS schemas whose instances have no GTS identity field:

- `gts.x.commerce.orders.order.v1.0~` -- Order schema. The `id` field is a plain UUID, not a `GtsInstanceId`. There is no `type` field.
- `gts.x.core.idp.contact.v1.0~` -- Contact schema. Same pattern: UUID `id`, no GTS identity.

These are valid GTS entity schemas (category 1) that produce instances falling under category 3. They are referenced by other GTS types (e.g., an event's `subjectType` references the order schema) but their instances do not self-identify via GTS.

The spec notes this explicitly ([§11.1](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)):

> *"The exact field names used for instance IDs and instance types are **implementation-defined** and may be **configuration-driven** (different systems may look for identifiers in different fields)."*

This gap surfaced as Issue #72, where data entity structs are forced to add a dummy `gts_type` field with fragile serde workarounds to satisfy the macro's requirement.

### 2.2 Distinguishing self-reference from cross-reference

The GTS specification defines two kinds of `x-gts-ref` annotations on schema properties ([§9.6](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support)):

- **`"x-gts-ref": "/$id"`** -- Self-reference. The field's value must equal the current schema's `$id`. Used on fields that identify *this* entity.
- **`"x-gts-ref": "gts.*"`** -- Cross-reference. The field's value can be any valid GTS identifier. Used on fields that reference *other* entities.

This distinction is visible in the spec's example schemas. The base event schema (`gts.x.core.events.type.v1~.schema.json`) demonstrates both on the same struct:

```json
{
  "type": {
    "description": "Identifier of the event type in GTS format.",
    "type": "string",
    "x-gts-ref": "/$id"
  },
  "subjectType": {
    "description": "GTS type identifier of the entity this event is about.",
    "type": "string",
    "x-gts-ref": "gts.*"
  }
}
```

The module schema (`gts.x.core.modules.module.v1~.schema.json`) shows the same pattern:

```json
{
  "type": { "x-gts-ref": "/$id" },
  "capabilities": {
    "items": { "x-gts-ref": "gts.x.core.modules.capability.v1~" }
  }
}
```

The current macro treats all `GtsSchemaId` fields identically, generating `"x-gts-ref": "gts.*"` for every one. It does not yet have a mechanism to distinguish a field that identifies *this* entity from a field that references *another* entity. This proposal adds that mechanism through field-level annotations.

### 2.3 Making serde derives visible

The current macro automatically adds `Serialize`, `Deserialize`, and `JsonSchema` derives to base structs, and blocks `Serialize`/`Deserialize` on nested structs. This approach successfully prevents nested structs from producing incomplete JSON, which was the original design goal.

The tradeoff is that users cannot see which traits are derived by reading the struct definition. The macro's serde attribute injection (`#[serde(bound(...))]`, `#[serde(serialize_with)]`) is invisible in source code. Issue #72 arose in part because the macro's serde handling for identity fields didn't account for deserialization correctly -- a problem that's harder to diagnose when the serde configuration isn't visible.

This proposal makes all derives explicit while preserving the same safety default: nested structs are still blocked from direct serialization unless the user opts out with `allow_direct_serde`.

### 2.4 Auto-deriving properties from struct fields

The current macro requires `properties = "event_type,id,tenant_id,payload"` -- a comma-separated string that lists which fields appear in the schema. This serves as both a schema surface declaration and a typo check (the macro validates that every listed property exists as a field).

The tradeoff is that the more dangerous direction isn't caught: if a field is added to the struct but omitted from `properties`, it silently disappears from the generated JSON Schema. For a system focused on diffable API contracts, this means a schema diff would show no change even though the wire format changed.

This proposal auto-derives properties from struct fields, catching changes in both directions. Fields can be excluded from the schema with `#[gts(skip)]` or `#[serde(skip)]`.

### 2.5 Clearer inheritance declaration

The current macro uses `base` to declare a struct's position in the hierarchy:

| `base` value | Meaning |
|---|---|
| `base = true` | Root type (no parent) |
| `base = ParentStruct` | Child type inheriting from parent |

`base = true` is the default state and carries no information. This proposal removes the need to declare root types explicitly -- the absence of `extends` means root -- and uses `extends = ParentStruct` for child types, which reads more naturally in the context of GTS's left-to-right inheritance model ([§2.2](https://github.com/GlobalTypeSystem/gts-spec#22-chained-identifiers), [§3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)).

---

## 3. What Changes

### 3.1 Entry point: Derive macro with `#[gts(...)]` attributes

The single `#[struct_to_gts_schema]` attribute macro evolves into `#[derive(GtsSchema)]` with `#[gts(...)]` attributes at both the struct and field level.

**Before:**

```rust
#[derive(Debug)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type",
    properties = "event_type,id,tenant_id,payload"
)]
pub struct BaseEventV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub payload: P,
}
```

**After:**

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type",
)]
pub struct BaseEventV1<P: GtsSchema> {
    #[gts(type_field)]
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub payload: P,
}
```

**What changed and why:**

| Change | Reason |
|---|---|
| `Serialize`, `Deserialize`, `JsonSchema` are explicit | User controls all derives. No hidden injection. |
| `base = true` removed | Root types are the default -- no declaration needed. |
| `properties = "..."` removed | Properties are auto-derived from struct fields. |
| `#[gts(type_field)]` added to `event_type` | Explicit opt-in marks this as the identity field ([§9.6](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support): `"x-gts-ref": "/$id"`). |
| `P: GtsSchema` bound is visible | Generic constraint is in source, not injected. |

### 3.2 Inheritance: `extends` replaces `base`

**Before:**

```rust
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event",
    properties = "user_agent,data"
)]
pub struct AuditPayloadV1<D> {
    pub user_agent: String,
    pub data: D,
}
```

**After:**

```rust
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event",
    extends = BaseEventV1,
)]
pub struct AuditPayloadV1<D: GtsSchema> {
    pub user_agent: String,
    pub data: D,
}
```

`extends = BaseEventV1` reads as what it means: this type extends the base event type. The `allOf` + `$ref` schema composition is generated from this declaration, following the GTS chained identifier model ([§2.2](https://github.com/GlobalTypeSystem/gts-spec#22-chained-identifiers), [§3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)):

> *"Multiple GTS identifiers can be chained with `~` to express derivation and conformance. The chain follows **left-to-right inheritance** semantics."*

The compile-time validations remain identical:
- Schema ID segment count must match `extends` presence ([§2.2](https://github.com/GlobalTypeSystem/gts-spec#22-chained-identifiers))
- Parent's `SCHEMA_ID` must match the parent segment in `schema_id` ([§3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance))
- Parent struct must have exactly one generic parameter

### 3.3 Optional identity fields with explicit annotations

**Before (Issue #72):**

```rust
// Forced to add a dead field with serde workaround
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.cf.core.errors.quota_violation.v1~",
    description = "A quota violation entry",
    properties = "subject,description"
)]
pub struct QuotaViolationV1 {
    #[allow(dead_code)]
    #[serde(skip_serializing, default = "dummy_gts_schema_id")]
    gts_type: GtsSchemaId,       // unwanted, breaks Deserialize (#72)
    pub subject: String,
    pub description: String,
}
```

**After:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.cf.core.errors.quota_violation.v1~",
    description = "A quota violation entry",
)]
pub struct QuotaViolationV1 {
    pub subject: String,
    pub description: String,
}
```

No dummy field. No serde workaround. The struct represents what the GTS spec intends -- a data entity schema whose instances don't carry GTS identity fields, like `order.v1.0~` or `contact.v1.0~` in the spec examples.

When identity fields *are* needed, they are annotated explicitly:

```rust
// Well-known instance (Spec §3.7: named instance with GTS instance ID)
#[gts(instance_id)]
pub id: GtsInstanceId,          // generates "x-gts-ref": "/$id"

// Anonymous instance (Spec §3.7: opaque id + GTS type discriminator)
#[gts(type_field)]
#[serde(rename = "type")]
pub event_type: GtsSchemaId,    // generates "x-gts-ref": "/$id"

// Cross-reference (Spec §9.6: reference to another entity's schema)
pub subject_type: GtsSchemaId,  // generates "x-gts-ref": "gts.*"
```

This maps directly to the spec's distinction in [§9.6](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support):

> *"`x-gts-ref": "/$id"` -- relative self-reference; field value must equal the current schema's `$id`"*
>
> *"`x-gts-ref": "gts.*"` -- field must be a valid GTS identifier; optionally resolve against a registry"*

The field-level attributes are validated at compile time:
- `#[gts(type_field)]` must be on a `GtsSchemaId` field
- `#[gts(instance_id)]` must be on a `GtsInstanceId` field
- The two are mutually exclusive (a schema's instances are either well-known or anonymous, per [§3.7](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances))
- At most one of each per struct

### 3.4 User-controlled serialization

The macro no longer injects or removes serde derives. Users explicitly declare `Serialize` and `Deserialize` where needed.

Nested structs (those with `extends`) are still blocked from direct serialization by default -- serializing a nested payload alone produces incomplete JSON (missing the base event envelope). This safety behavior, which was an intentional and valuable part of the original design, is preserved via marker trait conflicts (`GtsNoDirectSerialize` / `GtsNoDirectDeserialize`). The user can opt out with `allow_direct_serde` for testing or standalone use:

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema, GtsSchema)]
#[gts(
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    extends = BaseEventV1,
    allow_direct_serde,
)]
pub struct AuditPayloadV1 { ... }
```

Without `allow_direct_serde`, deriving `Serialize` on a nested struct produces a compile error.

### 3.5 Auto-derived properties

The `properties` parameter is replaced with auto-derivation from struct fields. All named fields are included in the generated JSON Schema by default. To exclude a field:

```rust
#[gts(skip)]                    // excluded from schema, still serializable
pub internal_cache: String,

#[serde(skip)]                  // excluded from both schema and serialization
pub runtime_state: String,
```

---

## 4. Schema Output

The generated JSON Schemas are **structurally identical** between old and new macros, verified by 17 parity tests that compare both macros' output on equivalent struct definitions.

### 4.1 Unchanged

- `$id` with `gts://` prefix ([§9.1](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema))
- `$schema` set to `http://json-schema.org/draft-07/schema#`
- `type: "object"`, `additionalProperties: false`
- `properties` and `required` arrays
- `allOf` + `$ref` composition for inherited types ([§9.1](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema))
- Generic field nesting via `wrap_in_nesting_path`
- `GtsSchemaId` / `GtsInstanceId` inline representation

### 4.2 Improvements

**`description` included in runtime schemas.** The current macro stores the `description` attribute but omits it from `gts_schema_with_refs()` output. The updated macro includes it, consistent with every spec example schema (`events.type.v1~`, `events.topic.v1~`, `orders.order.v1.0~`, `modules.module.v1~` -- all include `description`).

**Spec-correct `x-gts-ref` on identity fields.** As described in section 3.3, annotated identity fields generate `"x-gts-ref": "/$id"` while unannotated `GtsSchemaId` fields retain `"x-gts-ref": "gts.*"`.

### 4.3 Example output

```json
{
  "$id": "gts://gts.x.core.events.type.v1~",
  "$schema": "http://json-schema.org/draft-07/schema#",
  "description": "Base event type",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "type": {
      "type": "string",
      "format": "gts-schema-id",
      "title": "GTS Schema ID",
      "description": "GTS schema identifier",
      "x-gts-ref": "/$id"
    },
    "id": { "type": "string", "format": "uuid" },
    "tenant_id": { "type": "string", "format": "uuid" },
    "payload": { "type": "object" }
  },
  "required": ["type", "id", "tenant_id", "payload"]
}
```

Compare the `type` property above with the spec's base event schema (`gts.x.core.events.type.v1~.schema.json`):

```json
"type": {
  "description": "Identifier of the event type in GTS format.",
  "type": "string",
  "x-gts-ref": "/$id"
}
```

Both use `"x-gts-ref": "/$id"` on the type discriminator field. The current macro generates `"x-gts-ref": "gts.*"` here.

---

## 5. Extensibility

The current macro is ~1,843 lines in a single file (`lib.rs`). The updated implementation splits into focused modules:

```
gts-macros/src/
  lib.rs                  Entry points (current + updated macro)
  gts_schema_derive.rs    #[derive(GtsSchema)] orchestration
  gts_attrs.rs            Struct-level #[gts(...)] parsing
  gts_field_attrs.rs      Field-level #[gts(...)] parsing
  gts_validation.rs       All compile-time validations
  gts_codegen.rs          GtsSchema trait impl + runtime API generation
  gts_serde.rs            GtsSerialize/GtsDeserialize + serde blocking
```

This structure is designed to grow with the GTS specification. Concrete examples:

**Adding a new field-level attribute** (e.g., `#[gts(sensitive)]` to mark PII fields in the schema):
1. Add a variant to the `GtsFieldAttr` enum in `gts_field_attrs.rs`
2. Add parsing for the new keyword (3 lines)
3. Add validation rules in `gts_validation.rs`
4. Generate the schema annotation in `gts_codegen.rs`

No other modules are touched.

**Adding schema traits** ([§9.7](https://github.com/GlobalTypeSystem/gts-spec#97---schema-traits-x-gts-traits-schema--x-gts-traits) -- `x-gts-traits-schema` / `x-gts-traits`): The spec defines a trait system for schema-level metadata like retention rules and topic associations. The current macro doesn't support this. The modular design accommodates it via new struct-level attributes (e.g., `#[gts(traits_schema = "...")]`) following the same parse-validate-generate pipeline. The spec examples show this pattern:

```json
"x-gts-traits-schema": {
  "properties": {
    "topicRef": { "x-gts-ref": "gts.x.core.events.topic.v1~" },
    "retention": { "type": "string", "default": "P30D" }
  }
},
"x-gts-traits": {
  "topicRef": "gts.x.core.events.topic.v1~x.commerce._.orders.v1.0",
  "retention": "P90D"
}
```

**Adding new struct-level attributes**: A new option in `#[gts(...)]` is a key-value pair in `gts_attrs.rs` + validation + codegen. The parsing infrastructure handles it uniformly.

---

## 6. What Stays the Same

The proposal preserves all existing runtime behavior:

- **`GtsSchema` trait** -- `SCHEMA_ID`, `GENERIC_FIELD`, `gts_schema_with_refs()`, `gts_schema_with_refs_allof()`, `innermost_schema_id()`, `innermost_schema()`, `collect_nesting_path()`, `wrap_in_nesting_path()`
- **`GtsSerialize` / `GtsDeserialize`** trait system for nested structs, including `GtsSerializeWrapper` / `GtsDeserializeWrapper` bridge types
- **Serde blocking** for nested structs via `GtsNoDirectSerialize` / `GtsNoDirectDeserialize` marker traits (default behavior)
- **Runtime API** -- `gts_schema_id()`, `gts_base_schema_id()`, `gts_make_instance_id()`, `gts_instance_json()`, schema string methods
- **Associated constants** -- `SCHEMA_ID`, `GENERIC_FIELD`, `GTS_SCHEMA_FILE_PATH`, `GTS_SCHEMA_DESCRIPTION`, `GTS_SCHEMA_PROPERTIES`, `BASE_SCHEMA_ID`
- **Compile-time validations** -- schema ID format, version consistency, segment count, parent assertions, single generic parameter
- **Unit struct handling** -- `{}` / `null` serialization for both base and nested unit structs

---

## 7. Test Coverage

235 tests pass, covering both current and updated macros:

| Test suite | Count | What it validates |
|---|---|---|
| `compile_fail_tests` (v1) | 31 | Current macro compile-time error cases |
| `compile_fail_v2_tests` | 21 | Updated macro compile-time error cases |
| `integration_tests` (v1) | 45 | Current macro runtime behavior |
| `v2_integration_tests` | 22 | Updated macro runtime behavior |
| `v2_inheritance_tests` | 14 | Multi-level inheritance chains (2-level, 3-level) |
| `v2_serialization_tests` | 10 | Serialize / deserialize round-trips |
| `v2_serde_rename_tests` | 5 | Per-field `#[serde(rename)]` handling |
| `v2_parity_tests` | 17 | Current vs updated macro output comparison |
| `inheritance_tests` (v1) | 45 | Current macro inheritance chains |
| `inheritance_tests_mixed` | 7 | Mixed current/updated macro interop |
| Other | 18 | Pretty printing, serde rename (v1) |

The **17 parity tests** are the most critical -- they define equivalent structs using both macros and assert identical schema output, serialization output, deserialization behavior, trait constants, and runtime API results.

---

## 8. Migration

Both macros coexist. The current macro continues to work without changes.

Migration per struct:

1. Replace `#[struct_to_gts_schema(...)]` with `#[derive(GtsSchema)]` + `#[gts(...)]`
2. Add `#[derive(JsonSchema)]` and `#[derive(Serialize, Deserialize)]` where needed
3. Replace `base = true` with nothing; replace `base = Parent` with `extends = Parent`
4. Remove `properties = "..."` -- add `#[gts(skip)]` to fields that were excluded
5. Add `#[gts(type_field)]` or `#[gts(instance_id)]` to identity fields if present
6. Remove dummy identity fields that existed only to satisfy the current macro's requirement

---

## 9. Specification References

| Spec section | Topic | How this proposal uses it |
|---|---|---|
| [§2.2](https://github.com/GlobalTypeSystem/gts-spec#22-chained-identifiers) | Chained identifiers | `extends` models left-to-right inheritance via chained `~` segments |
| [§3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance) | Type inheritance | Compile-time validation of parent-child segment matching; `allOf` + `$ref` generation |
| [§3.7](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances) | Well-known vs anonymous instances | `#[gts(instance_id)]` for well-known, `#[gts(type_field)]` for anonymous -- both optional |
| [§4.1](https://github.com/GlobalTypeSystem/gts-spec#41-compatibility-modes) | Versioning | Version match validation between struct name suffix and schema ID |
| [§9.1](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema) | `$id` and `$ref` conventions | Generated schemas use `gts://` prefix on `$id` and `$ref` |
| [§9.6](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support) | `x-gts-ref` support | Identity fields get `"/$id"`, cross-reference fields get `"gts.*"` |
| [§9.7](https://github.com/GlobalTypeSystem/gts-spec#97---schema-traits-x-gts-traits-schema--x-gts-traits) | Schema traits | Not yet implemented; modular design accommodates future support |
| [§11.1](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories), Rule C | Five document categories | Identity fields made optional -- not all schemas produce self-identifying instances |
| [§11.1](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories) | Implementation-defined field names | Field annotations replace hardcoded name matching |
