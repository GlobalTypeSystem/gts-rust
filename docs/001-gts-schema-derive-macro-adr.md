---
status: accepted
date: 2026-04-24
decision-makers: Andre Smith
---

# `#[derive(GtsSchema)]` — Derive Macro for GTS Types

## Context and Problem Statement

The `gts-rust` crate needs a macro that binds Rust structs to the Global Type System. The macro must:

- Validate GTS schema identifiers and parent-child inheritance chains at compile time
- Generate JSON Schema that conforms to the GTS specification (v0.8)
- Produce a runtime API consumed by the Type Registry, REST/RPC/MCP APIs, and Event streams
- Make GTS type deducible from every instance so the same object can move uniformly across those transports
- Distinguish self-reference (`"x-gts-ref": "/$id"`) from cross-reference (`"x-gts-ref": "gts.*"`) on `GtsSchemaId` fields

The question is what macro shape best satisfies these goals while remaining idiomatic Rust.

## Decision Drivers

* GTS type must always be deducible from any instance — a hard requirement for Type Registry, RPC/MCP, and Event use
* Explicit over implicit — derives and attributes visible in source code so that behavior is readable from the struct definition
* Spec-correct `x-gts-ref` annotations ([Spec §9.6](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support)) distinguishing self-reference from cross-reference
* Safety: nested structs must not produce standalone JSON (would omit the base envelope)
* Maintainability: single-responsibility modules, clear parse-validate-generate pipeline, room for future spec features (schema traits, PII annotations, etc.)
* Familiar Rust idiom: `#[derive(...)]` with a `#[namespace(...)]` attribute, matching `serde`, `schemars`, `thiserror`

## Considered Options

* Derive macro with `#[gts(...)]` struct and field attributes (`#[derive(GtsSchema)]` + `#[gts(schema_id = "...", ...)]` on struct + `#[gts(type_field)]` / `#[gts(instance_id)]` / `#[gts(skip)]` on fields)
* Attribute macro that stays the single entry point, augmented with `#[gts(...)]` field annotations
* Multiple derives partitioned by role (`#[derive(GtsRoot)]` / `#[derive(GtsChild)]` / `#[derive(GtsLeaf)]`)

## Decision Outcome

Chosen option: **Derive macro with `#[gts(...)]` struct and field attributes**, because it matches standard Rust ecosystem patterns (`serde`, `schemars`), makes every derive visible at the struct definition, and cleanly separates GTS metadata (struct-level attributes) from per-field identity semantics (field-level attributes). A single derive handles root, child, and leaf types — the role is determined by `extends`, not by a different macro.

### `#[derive(GtsSchema)]` and `#[gts(...)]` — one macro, not two

`#[gts(...)]` is a **helper attribute** declared by the derive, not a separate proc macro. The proc macro declaration is:

```rust
#[proc_macro_derive(GtsSchema, attributes(gts))]
pub fn gts_schema_derive(input: TokenStream) -> TokenStream { ... }
```

The `attributes(gts)` clause claims the `#[gts(...)]` namespace for this derive. Without `#[derive(GtsSchema)]` in scope, `#[gts(...)]` has no meaning — rustc rejects it as an unknown attribute. The derive is the macro; the attribute is configuration the macro reads.

This is the standard Rust convention for derive macros with configuration:

- `#[derive(Serialize)]` + `#[serde(...)]`
- `#[derive(JsonSchema)]` + `#[schemars(...)]`
- `#[derive(Error)]` + `#[error(...)]` (thiserror)
- `#[derive(Parser)]` + `#[clap(...)]` (clap)

The alternative — making `#[gts(...)]` an attribute macro that wraps the struct — is the shape of the old `#[struct_to_gts_schema(...)]`. That shape replaces the annotated item with macro-expanded code, so the derives the user writes are invisible from the source. Issue #72 is a direct consequence: the old macro's silent serde handling couldn't be diagnosed without expanding the macro. Moving to a derive + helper-attribute pair is how the new design keeps derives visible.

### Entry Point

```rust
#[derive(Debug, Clone, JsonSchema, GtsSchema)]
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

Note on derives for generic roots: the macro emits its own `serde::Serialize` / `serde::Deserialize` impls for structs carrying a `GtsSchema`-bounded generic parameter (so that nested payloads route through the `GtsSerialize` / `GtsDeserialize` bridge). Callers must not add `Serialize` / `Deserialize` to the derive list in that case — the generated impls would conflict. Non-generic root structs derive `Serialize` / `Deserialize` normally.

### Struct-Level `#[gts(...)]` Attributes

| Attribute | Cardinality | Purpose |
|---|---|---|
| `schema_id = "..."` | required | GTS schema identifier (e.g., `"gts.x.core.events.type.v1~"`). Maps to `$id` in generated JSON Schema. Validated against [Spec §2.1, §2.3, §8.1](https://github.com/GlobalTypeSystem/gts-spec) and version-matched against the struct name suffix ([§4.1](https://github.com/GlobalTypeSystem/gts-spec#41-compatibility-modes)). |
| `description = "..."` | required | Human-readable schema description. Emitted into generated JSON Schema (including at runtime via `gts_schema_with_refs()`). |
| `dir_path = "..."` | required | Output directory for CLI schema generation (relative to crate root). Stored as the `GTS_SCHEMA_FILE_PATH` associated constant. |
| `extends = Parent` | optional | Declares parent type for a child schema. Validated against the parent segment in `schema_id` ([Spec §2.2](https://github.com/GlobalTypeSystem/gts-spec#22-chained-identifiers), [§3.2](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)). Absent means root; `extends = None` is an equivalent explicit form for grep/IDE discoverability. |

### Field-Level `#[gts(...)]` Attributes

| Attribute | Cardinality | Purpose |
|---|---|---|
| `#[gts(type_field)]` | exactly 0 or 1 per struct, mutually exclusive with `instance_id` | Marks the GTS type discriminator. Must be on a `GtsSchemaId` field. Generates `"x-gts-ref": "/$id"` on the property. |
| `#[gts(instance_id)]` | exactly 0 or 1 per struct, mutually exclusive with `type_field` | Marks the GTS instance identifier. Must be on a `GtsInstanceId` field. Generates `"x-gts-ref": "/$id"` on the property. |
| `#[gts(skip)]` | any number | Excludes the field from generated JSON Schema properties. Serde behavior unaffected (use `#[serde(skip)]` to skip serialization as well). |

### Required Identity Field (root structs)

Every **root struct** (no `extends`) must declare **exactly one** of `#[gts(type_field)]` or `#[gts(instance_id)]`. The two are mutually exclusive. A root struct with neither is rejected at compile time; duplicates on the same struct are rejected.

Derived structs (`extends = Parent`) do not carry an identity field of their own. The outermost serialized JSON has a single `type` (or `id`) field on the root struct, carrying the full chained identifier — e.g., `gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~` — which is sufficient for consumers to recover the full GTS type. This matches [Spec §11.2](https://github.com/GlobalTypeSystem/gts-spec#112---normalized-instance-format-event-audit-and-order) and the spec's instance examples. Nested structs that declared their own identity field would produce a redundant, ambiguous shape, so the annotation is rejected on derived structs.

This guarantees GTS type is always deducible from the instance itself — a core GTS design goal. Type Registry lookups, RPC/MCP payload dispatch, and Event consumers can all recover the schema from the payload without external metadata.

### Generated Constructor

`#[derive(GtsSchema)]` generates a `new(...)` constructor that auto-populates the identity field, removing the most common error surface (hand-writing the wrong `SCHEMA_ID` into the type field):

```rust
// Generated for BaseEventV1<P: GtsSchema>
impl<P: GtsSchema> BaseEventV1<P> {
    pub fn new(id: Uuid, tenant_id: Uuid, payload: P) -> Self {
        Self {
            event_type: ::gts::gts::GtsSchemaId::new(<P as ::gts::GtsSchema>::SCHEMA_ID),
            id,
            tenant_id,
            payload,
        }
    }
}
```

The signature lists every field in struct-definition order **except** the `#[gts(type_field)]` field (which is auto-populated). `#[gts(skip)]` and `#[serde(skip)]` are schema/serde concerns and do not affect the constructor — every field remains part of the struct's data model.

For root structs with `#[gts(type_field)]`, the identity field is populated from `::gts::gts::GtsSchemaId::new(<P as ::gts::GtsSchema>::SCHEMA_ID)` when the struct is generic (so a given base-event shape specializes to the child's chained identifier), and from `Self::gts_schema_id().clone()` when it is not (the inherent LazyLock-cached accessor generated alongside). For `#[gts(instance_id)]` fields, the caller passes the id into `new(...)` — the macro does not deduce domain-specific id segments.

Derived structs have no identity field to populate, so their `new(...)` simply takes every field in order.

### Serde Policy

**Non-generic** base structs **must** derive `Serialize` and `Deserialize`. The macro does not inject them; instead, the generated runtime API (`gts_instance_json()`, etc.) calls `serde_json::to_value(self)`, which requires `Self: Serialize`. If the derives are missing, the compiler produces a clear, localized error at the use site — no separate macro validation needed. **Generic** base structs (those carrying a `GtsSchema`-bounded type param) are handled by the macro as described above — callers must not derive `Serialize`/`Deserialize` on them.

Nested structs (those with `extends = Parent`) **cannot** derive `Serialize`/`Deserialize`. Direct serialization would produce incomplete JSON (missing the base envelope). The block is enforced by the `GtsNoDirectSerialize` / `GtsNoDirectDeserialize` marker traits, which conflict with the standard serde blanket impls. There is no opt-out attribute; testing and debugging paths must go through the base struct.

The `GtsSerialize` / `GtsDeserialize` trait system bridges the two worlds. The macro generates explicit impls for nested structs; the base struct's `#[serde(serialize_with = "gts::serialize_gts")]` on the generic field delegates through `GtsSerializeWrapper`. This is the one place the macro manipulates serde attributes (and only on generic fields with a `GtsSchema` bound).

### Inheritance

`extends = Parent` generates `allOf` + `$ref` schema composition following GTS's left-to-right inheritance model:

```rust
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event with user context",
    extends = BaseEventV1,
)]
pub struct AuditPayloadV1<D: GtsSchema> {
    pub user_agent: String,
    pub data: D,
}
```

Compile-time validations:

- `extends` absent (or `extends = None`): `schema_id` must have exactly one segment
- `extends = Parent`: `schema_id` must have 2+ segments; the parent segment must match `Parent::SCHEMA_ID`; the parent struct must have exactly one generic parameter

### Auto-Derived Schema Properties

All named struct fields appear in the generated JSON Schema by default. Fields can be excluded with `#[gts(skip)]` (schema-only) or `#[serde(skip)]` (schema and serialization). This replaces the previous design's `properties = "a,b,c"` list, which was opt-in and silently omitted newly added fields — an opposite-direction failure that would not show up in a schema diff.

### Schema Output Contract

Generated JSON Schema is structurally identical to the previous macro's output, with two improvements:

1. `"x-gts-ref": "/$id"` on `#[gts(type_field)]` / `#[gts(instance_id)]` fields (previously `"gts.*"` on every `GtsSchemaId`), matching spec examples `events.type.v1~`, `events.topic.v1~`, `modules.capability.v1~`.
2. `description` included in runtime schemas via `gts_schema_with_refs()` (previously omitted from runtime output, though stored), matching every spec example schema.

Other aspects — `$id`, `$schema`, `type: "object"`, `additionalProperties: false`, `properties`, `required`, `allOf` + `$ref`, generic field nesting, `GtsSchemaId` / `GtsInstanceId` representation — are unchanged.

### File Layout

```
gts-macros/src/
  lib.rs                   Derive macro entry point
  gts_schema_derive.rs     Orchestration
  gts_attrs.rs             Struct-level #[gts(...)] parsing
  gts_field_attrs.rs       Field-level #[gts(...)] parsing
  gts_validation.rs        Compile-time validations
  gts_codegen.rs           Trait impl + runtime API generation
  gts_serde.rs             GtsSerialize/GtsDeserialize + blocking
```

Each module has a single responsibility; new struct-level or field-level attributes follow the same parse-validate-generate pipeline.

### Consequences

* Good, because derives are visible at the struct definition — readers can see exactly what `Serialize`, `Deserialize`, `JsonSchema`, and `GtsSchema` contribute without inspecting macro expansion
* Good, because GTS type is always deducible from any instance, enabling uniform handling across Type Registry, REST, RPC, MCP, and Event transports
* Good, because the generated `new(...)` constructor eliminates the most common identity-field error (wrong `SCHEMA_ID` assigned by hand)
* Good, because spec-correct `x-gts-ref` annotations match the GTS specification examples, improving downstream tooling interop
* Good, because focused modules (attrs / validation / codegen / serde) lower the cost of adding new spec features (schema traits, PII markers, etc.)
* Good, because existing generated JSON Schemas remain structurally identical — migration does not break downstream consumers
* Bad, because every root struct now carries at least one field-level attribute (`#[gts(type_field)]` or `#[gts(instance_id)]`) in addition to the struct-level `#[gts(...)]` block — more syntax than the single-attribute macro (derived structs are unchanged: they already carried no identity field)
* Bad, because the `GtsNoDirectSerialize` / `GtsNoDirectDeserialize` blocking has no escape hatch, which can be surprising for users trying to test a nested struct in isolation (resolved by testing through the base struct)
* Bad, because splitting into focused modules adds files to navigate, though each is smaller and single-purpose

### Confirmation

Test suites in `gts-macros/tests/` verify the design:

| Suite | Count | Validates |
|---|---|---|
| `v2_compile_fail` | 24 | Compile-time rejection of invalid configurations (missing identity, wrong field types, schema-ID format, inheritance mismatches, direct-serde on nested, etc.) |
| `v2_integration_tests` | 28 | Runtime API + schema output for base structs |
| `v2_inheritance_tests` | 14 | Multi-level inheritance chains (2-level, 3-level) |
| `v2_serialization_tests` | 11 | `Serialize` / `Deserialize` round-trips through the `GtsSerialize` bridge |
| `v2_serde_rename_tests` | 5 | Per-field `#[serde(rename)]` handling in nested deserializer |
| `v2_parity_tests` | 17 | Identical output vs the previous `#[struct_to_gts_schema]` macro on equivalent structs — the critical contract that downstream consumers see no change |
| `v2_inheritance_tests_mixed` | 5 | Interop between old and new macros during migration |

The 17 parity tests are load-bearing: they assert identical schema output, serialized instance JSON, deserialization behavior, trait constants, and runtime API results across both macros. Any divergence would break downstream consumers.

## Pros and Cons of the Options

### Derive macro with `#[gts(...)]` attributes

Single `#[derive(GtsSchema)]` entry point. Struct-level `#[gts(schema_id = "...", ...)]`. Field-level `#[gts(type_field)]` / `#[gts(instance_id)]` / `#[gts(skip)]`. `extends = Parent` handles inheritance.

* Good, because matches standard Rust ecosystem patterns (`serde`, `schemars`)
* Good, because all derives visible at the struct definition
* Good, because `#[gts(...)]` namespace keeps GTS attributes isolated from `#[serde(...)]`, `#[schemars(...)]`, etc.
* Good, because a single derive handles root / child / leaf — role determined by `extends`, not by a different macro
* Good, because new attributes (PII, schema traits, ...) extend the same namespace without touching the derive contract
* Bad, because users write both `#[derive(GtsSchema)]` *and* `#[gts(...)]` — two namespaces instead of one

### Attribute macro with field annotations

Keep `#[struct_to_gts_schema(...)]` as the single entry point. Add `#[gts(type_field)]` / `#[gts(skip)]` field annotations to fill the gaps.

* Good, because only one macro to invoke
* Good, because minimal migration for existing users
* Bad, because attribute macros replace the annotated item — every expansion must re-emit the struct, obscuring what the user wrote
* Bad, because the attribute macro continues to control derives silently (injecting `Serialize`/`Deserialize`), contrary to the visibility driver
* Bad, because it conflates GTS metadata, trait impl, runtime API, and derive control into one macro — the same shape that caused Issue #72

### Multiple derives by role

Separate `#[derive(GtsRoot)]`, `#[derive(GtsChild)]`, `#[derive(GtsLeaf)]` for the three inheritance positions.

* Good, because type-level distinction between roles
* Bad, because fragments a single concept across three macros with mostly-overlapping attributes
* Bad, because cross-role validation (parent `SCHEMA_ID` match, segment count) becomes harder when the derive doesn't know its siblings
* Bad, because converting a leaf into an intermediate (adding a child) requires changing the derive, not just the attributes

## More Information

- [Migration guide — `#[struct_to_gts_schema]` → `#[derive(GtsSchema)]`](./002-struct-to-gts-schema-migration.md): the old/new diff, migration walkthrough, schema-output parity, compile-fail test mapping, coexistence stance
- [Implementation plan](./002-macro-migration-implementation-plan.md): phased rollout
- [GTS Specification v0.8](https://github.com/GlobalTypeSystem/gts-spec):
  - [§2.2 — Chained identifiers](https://github.com/GlobalTypeSystem/gts-spec#22-chained-identifiers)
  - [§3.2 — Type inheritance](https://github.com/GlobalTypeSystem/gts-spec#32-gts-types-inheritance)
  - [§3.7 — Well-known and anonymous instances](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances)
  - [§9.1 — `$id` and `$ref` conventions](https://github.com/GlobalTypeSystem/gts-spec#91---identifier-reference-in-json-and-json-schema)
  - [§9.6 — `x-gts-ref` support](https://github.com/GlobalTypeSystem/gts-spec#96---x-gts-ref-support)
  - [§9.7 — Schema traits](https://github.com/GlobalTypeSystem/gts-spec#97---schema-traits-x-gts-traits-schema--x-gts-traits) (future work — design accommodates via new struct-level attributes)
  - [§11.1 — JSON document categories](https://github.com/GlobalTypeSystem/gts-spec#111-global-rules-schema-vs-instance-normalization-and-document-categories)
- Spec examples referenced in the design:
  - [`events.type.v1~.schema.json`](/.gts-spec/examples/events/schemas/gts.x.core.events.type.v1~.schema.json) — anonymous instance with `type` field using `x-gts-ref: "/$id"`
  - [`events.topic.v1~.schema.json`](/.gts-spec/examples/events/schemas/gts.x.core.events.topic.v1~.schema.json) — well-known instance with `id` field using `x-gts-ref: "/$id"`
  - [`compute.vm.v1~.schema.json`](/.gts-spec/examples/typespec/vms/schemas/gts.x.infra.compute.vm.v1~.schema.json) — hybrid pattern with `gtsId` + UUID `id`
- [Issue #72 — `struct_to_gts_schema: gts_type` field blocks `Deserialize`](https://github.com/GlobalTypeSystem/gts-rust/issues/72): concrete symptom that motivated the redesign

## Traceability

- **Supersedes proposal**: the former `001-macro-proposal.md` (removed when this ADR landed)
- **Implemented on branch**: `gts-macro-implementation`
- **Depends on**: [Migration guide](./002-struct-to-gts-schema-migration.md) for coexistence and diff details
