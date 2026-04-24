# GTS Macros

Procedural macros binding Rust structs to the [Global Type System](https://github.com/GlobalTypeSystem/gts-spec).

The primary API is the `#[derive(GtsSchema)]` derive macro with `#[gts(...)]` helper attributes. It:

1. **Validates** GTS identifiers, inheritance chains, and identity-field placement at compile time
2. **Generates** JSON Schema conforming to the GTS specification (v0.8)
3. **Exposes** a runtime API consumed by the Type Registry, REST/RPC/MCP APIs, and Event streams
4. **Generates a safe-by-construction `new(...)` constructor** that eliminates the most common identity-field errors

> **Legacy macro notice.** The older `#[struct_to_gts_schema(...)]` attribute macro is still shipped and fully functional. New code should use `#[derive(GtsSchema)]`. For mechanical conversion steps and schema-output parity, see the [migration guide](../docs/002-struct-to-gts-schema-migration.md). The design rationale is documented in [ADR-001](../docs/001-gts-schema-derive-macro-adr.md).

---

## Table of contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [Struct-level `#[gts(...)]` attributes](#struct-level-gts-attributes)
- [Field-level `#[gts(...)]` attributes](#field-level-gts-attributes)
- [Identity fields](#identity-fields-type_field-vs-instance_id)
- [Generated `new(...)` constructor](#generated-new-constructor)
- [Inheritance](#inheritance)
- [Serialization](#serialization)
- [Runtime API](#runtime-api)
- [Schema generation (CLI)](#schema-generation-cli)
- [Rust → JSON Schema type mapping](#rust--json-schema-type-mapping)
- [Compile-time validation](#compile-time-validation)
- [Legacy `#[struct_to_gts_schema]`](#legacy-struct_to_gts_schema)

---

## Installation

```toml
[dependencies]
gts = { path = "path/to/gts-rust/gts" }
gts-macros = { path = "path/to/gts-rust/gts-macros" }
serde = { version = "1", features = ["derive"] }
schemars = { version = "1.2", features = ["uuid1"] }
```

`schemars::JsonSchema` must be derived on every GTS struct — `#[derive(GtsSchema)]` uses `schemars::schema_for!(Self)` internally to generate the properties object.

---

## Quick start

```rust
use gts::{GtsSchema, GtsSchemaId, GtsInstanceId};
use gts_macros::GtsSchema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A root event type (anonymous instance: carries its GTS type via `type_field`).
/// Generic-root — the macro emits its own `Serialize`/`Deserialize`; do not derive them.
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

/// A well-known topic (carries its identity via `instance_id`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Event topic definition",
)]
pub struct EventTopicV1 {
    #[gts(instance_id)]
    pub id: GtsInstanceId,
    pub name: String,
}

// The macro generates `new(...)` on both — the identity field in
// `BaseEventV1` is auto-populated from the generic parameter's SCHEMA_ID;
// the `instance_id` in `EventTopicV1` is passed by the caller.
fn example() {
    // `P = ()` terminates the inheritance chain. For a real event, `P` would
    // be another type that derives `GtsSchema` (see the Inheritance section).
    let event = BaseEventV1::<()>::new(Uuid::new_v4(), Uuid::new_v4(), ());
    // `event.event_type` was populated from `<() as GtsSchema>::SCHEMA_ID`.

    let topic = EventTopicV1::new(
        GtsInstanceId::new("gts.x.core.events.topic.v1~", "x.orders.placed.v1"),
        "orders".to_string(),
    );
    assert_eq!(topic.id.as_ref(), "gts.x.core.events.topic.v1~x.orders.placed.v1");
}
```

The unfamiliar shapes — `#[gts(type_field)]`, `#[gts(instance_id)]`, the generated `new(...)` — are explained in the sections below.

---

## Struct-level `#[gts(...)]` attributes

| Attribute | Cardinality | Description |
|---|---|---|
| `schema_id = "..."` | required | GTS schema identifier. Maps to `$id` in the generated JSON Schema. Validated against the identifier rules in [Spec §2.1, §2.3, §8.1](https://github.com/GlobalTypeSystem/gts-spec) and version-matched against the struct name suffix (e.g. `V1` / `v1~`). |
| `description = "..."` | required | Human-readable schema description. Emitted into the JSON Schema, including at runtime via `gts_schema_with_refs()`. |
| `dir_path = "..."` | required | Output directory for CLI schema generation (relative to the crate root). Stored as the `GTS_SCHEMA_FILE_PATH` associated constant. |
| `extends = Parent` | optional | Declares the parent type for a derived schema. Validated against the parent segment in `schema_id`. |
| `extends = None` | optional | Equivalent to omitting `extends` entirely — an explicit root marker for grep/IDE discoverability. |

Attributes may appear in any order. Any unknown key is rejected at compile time.

---

## Field-level `#[gts(...)]` attributes

| Attribute | Cardinality | Description |
|---|---|---|
| `#[gts(type_field)]` | 0 or 1 per root struct; mutually exclusive with `instance_id`; forbidden on derived structs | Marks the GTS type discriminator. Must be on a `GtsSchemaId` field. Generates `"x-gts-ref": "/$id"` on the property and makes the field auto-populated by `new(...)`. |
| `#[gts(instance_id)]` | 0 or 1 per root struct; mutually exclusive with `type_field`; forbidden on derived structs | Marks the GTS instance identifier for a well-known instance. Must be on a `GtsInstanceId` field. Generates `"x-gts-ref": "/$id"` on the property. The value is passed by the caller of `new(...)`. |
| `#[gts(skip)]` | any number | Excludes the field from the generated JSON Schema properties. Serde behavior is unaffected — use `#[serde(skip)]` to also skip serialization. |

Every **root** struct (no `extends`) must declare exactly one of `type_field` / `instance_id`. **Derived** structs (`extends = Parent`) must declare neither — the root's chained identifier already carries the GTS type.

---

## Identity fields: `type_field` vs `instance_id`

The two identity annotations correspond to the two GTS instance patterns in [Spec §3.7](https://github.com/GlobalTypeSystem/gts-spec#37-well-known-and-anonymous-instances):

- **`#[gts(type_field)]` — anonymous instance.** The outermost JSON object carries a `type` field whose value is the full chained schema identifier (e.g. `gts.x.core.events.type.v1~x.commerce.orders.placed.v1~`). Alongside, there is typically a separate `id: Uuid` field for the event's unique instance id. Used by the events pattern.
- **`#[gts(instance_id)]` — well-known instance.** The object itself is uniquely identified by a GTS instance id (e.g. `gts.x.core.events.topic.v1~x.orders.placed.v1`). Used for topics, registries, and similar long-lived entities that the Type Registry looks up directly.

Both emit `"x-gts-ref": "/$id"` on the identified property, matching the spec's self-reference convention. Non-identity `GtsSchemaId` fields retain the generic `"x-gts-ref": "gts.*"`.

---

## Generated `new(...)` constructor

Every non-empty `#[derive(GtsSchema)]` struct gets a `pub fn new(...) -> Self` that takes each field in struct-definition order, **except** the `#[gts(type_field)]` field (which is auto-populated).

- Generic root with `type_field`: the identity is set from `<P as GtsSchema>::SCHEMA_ID`, so `BaseEventV1<OrderPayloadV1>::new(...)` carries the child's chained identifier without the caller ever naming it.
- Non-generic root with `type_field`: the identity is set from `Self::gts_schema_id().clone()` (a LazyLock-cached accessor).
- Root with `instance_id`: the `GtsInstanceId` is a regular constructor parameter — the macro does not synthesize the instance segment.
- Derived struct: has no identity field, so `new(...)` simply takes every field in order.

`#[gts(skip)]` and `#[serde(skip)]` are schema/serde concerns respectively and do not affect the constructor: every field is still part of the struct's data model and appears in the signature.

```rust
let order = OrderV1_0::new(Uuid::new_v4(), Uuid::new_v4(), 3, 29.99);
// order.gts_type was set to OrderV1_0::gts_schema_id() — never hand-assigned.
```

If you need a different name (e.g. you want your own `new`), rename your constructor — the generated `new` is inherent and any duplicate will surface as rustc's standard duplicate-definition error.

---

## Inheritance

Derived types declare `extends = Parent`. The parent must be generic; its generic field becomes the nesting slot for the child.

```rust
/// Level 1: root (generic).
#[derive(Debug, JsonSchema, GtsSchema)]
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
    pub payload: P,
}

/// Level 2: derived, generic.
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

/// Level 3: derived, non-generic (leaf).
#[derive(Debug, JsonSchema, GtsSchema)]
#[gts(
    dir_path = "schemas",
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
    description = "Order placement audit event",
    extends = AuditPayloadV1,
)]
pub struct PlaceOrderDataV1 {
    pub order_id: Uuid,
    pub product_id: Uuid,
}
```

Compile-time checks:

- `extends` absent (or `= None`): `schema_id` must have exactly one segment.
- `extends = Parent`: `schema_id` must have 2+ segments; the parent segment must match `Parent::SCHEMA_ID`; the parent struct must have exactly one generic parameter.

The generated schema for a derived type uses `allOf` + `$ref` to compose with the parent:

```json
{
  "$id": "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~",
  "allOf": [
    { "$ref": "gts://gts.x.core.events.type.v1~" },
    { "type": "object", "properties": { /* child-specific */ } }
  ]
}
```

---

## Serialization

The derive's serde handling depends on the struct's shape:

- **Generic roots** (e.g. `BaseEventV1<P: GtsSchema>`) — the macro emits its own `Serialize` / `Deserialize` impls so nested payloads route through the `GtsSerialize` / `GtsDeserialize` bridge. Do **not** add `#[derive(Serialize, Deserialize)]`; the impls would conflict (`E0119`).
- **Non-generic roots** (e.g. `EventTopicV1`) — the user must `#[derive(Serialize, Deserialize)]`. The generated `gts_instance_json()` calls `serde_json::to_value(self)`, which requires `Self: Serialize`. Forgetting the derive produces a clear, localized error at the struct definition.
- **Derived structs** (`extends = Parent`) — **cannot** derive `Serialize` / `Deserialize`. Their JSON would be incomplete (missing the base envelope), so the macro blocks direct serde via the `GtsNoDirectSerialize` / `GtsNoDirectDeserialize` marker traits. The macro instead emits `GtsSerialize` / `GtsDeserialize` impls that bridge through the base struct's generic field. Testing and debugging nested payloads always happens through the root.

```rust
// Construct a 3-level instance through the chain.
let event: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>> = BaseEventV1::new(
    Uuid::new_v4(),
    AuditPayloadV1::new(
        "Mozilla/5.0".to_string(),
        PlaceOrderDataV1::new(Uuid::new_v4(), Uuid::new_v4()),
    ),
);

let json = serde_json::to_string_pretty(&event).unwrap();
let back: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>> =
    serde_json::from_str(&json).unwrap();
```

---

## Runtime API

Every `#[derive(GtsSchema)]` struct gets:

| Method / constant | Kind | Description |
|---|---|---|
| `gts_schema_id()` | inherent fn | `&'static GtsSchemaId` for `Self`. LazyLock-cached. |
| `gts_base_schema_id()` | inherent fn | `Option<&'static GtsSchemaId>` — parent's id for derived structs, `None` for roots. |
| `gts_make_instance_id(segment)` | inherent fn | `GtsInstanceId` by appending `segment` to the schema id. |
| `gts_schema_with_refs_as_string()` | inherent fn | Schema as a compact JSON string. |
| `gts_schema_with_refs_as_string_pretty()` | inherent fn | Schema as a pretty-printed JSON string. |
| `gts_instance_json(&self)` | inherent fn | `serde_json::Value`; requires `Self: Serialize`. |
| `gts_instance_json_as_string(&self)` | inherent fn | Compact JSON string; requires `Self: Serialize`. |
| `gts_instance_json_as_string_pretty(&self)` | inherent fn | Pretty JSON string; requires `Self: Serialize`. |
| `new(...)` | inherent fn | See [Generated `new(...)` constructor](#generated-new-constructor). |
| `SCHEMA_ID` | trait const | `&'static str`. The identifier as a plain string. |
| `GENERIC_FIELD` | trait const | `Option<&'static str>` — the serialized name of the generic field, if any. |
| `GTS_SCHEMA_FILE_PATH` | inherent const | `<dir_path>/<schema_id>.schema.json`, used by the CLI. |
| `GTS_SCHEMA_DESCRIPTION` | inherent const | Copy of the `description` attribute. |
| `GTS_SCHEMA_PROPERTIES` | inherent const | Comma-separated list of serialized property names (excludes `#[gts(skip)]` and `#[serde(skip)]`). |
| `BASE_SCHEMA_ID` | inherent const | `Option<&'static str>` — parent's identifier as a plain string. |

The `GtsSchema` trait itself (in `gts/src/schema.rs`) is also implemented; its methods `gts_schema_with_refs()` and `gts_schema_with_refs_allof()` return the composed `serde_json::Value`.

---

## Schema generation (CLI)

Generate JSON Schema files to disk using `gts-cli`:

```bash
# Walk source files, find every #[derive(GtsSchema)] / #[struct_to_gts_schema], write schemas.
gts generate-from-rust --source src/

# Override output directory.
gts generate-from-rust --source src/ --output schemas/

# Exclude specific directories (glob patterns, can repeat).
gts generate-from-rust --source . --exclude "tests/*" --exclude "examples/*"
```

Files can opt out with a top-of-file directive:

```rust
// gts:ignore
//! This file will be skipped by the CLI.
```

The CLI scans for the macro invocations, extracts `schema_id`, `description`, and the field list, maps Rust types to JSON Schema (see [Rust → JSON Schema type mapping](#rust--json-schema-type-mapping)), and writes the result to `<dir_path>/<schema_id>.schema.json`.

---

## Rust → JSON Schema type mapping

| Rust type | JSON Schema type | Format | Required by default |
|---|---|---|---|
| `String`, `&str` | `string` | — | yes |
| `i8`..`i128`, `u8`..`u128` | `integer` | — | yes |
| `f32`, `f64` | `number` | — | yes |
| `bool` | `boolean` | — | yes |
| `Vec<T>` | `array` | — | yes |
| `Option<T>` | same as `T` | — | **no** |
| `Uuid` | `string` | `uuid` | yes |
| `DateTime`, `NaiveDateTime` | `string` | `date-time` | yes |
| `NaiveDate` | `string` | `date` | yes |
| `HashMap<K,V>`, `BTreeMap<K,V>` | `object` | — | yes |
| `GtsInstanceId` | `string` | `gts-instance-id` | yes |
| `GtsSchemaId` | `string` | `gts-schema-id` | yes |

Notes:

- `Option<T>` fields are absent from the `required` array.
- Generic fields (`payload: P`) are placeholders in the root schema; inheritance composition fills them in for derived types.

---

## Compile-time validation

The macro catches the following at compile time:

- **Attribute shape.** `dir_path`, `schema_id`, and `description` are required. Unknown keys in `#[gts(...)]` or `#[gts(...)]`-on-field are rejected.
- **GTS identifier format.** `schema_id` is run through `gts_id::validate_gts_id` ([Spec §2.1, §2.3, §8.1](https://github.com/GlobalTypeSystem/gts-spec)).
- **Version consistency.** The struct name's version suffix (`V1`, `V2_0`, …) must match the schema-id's version token (`v1~`, `v2.0~`, …).
- **Segment count.** No `extends` (or `extends = None`) → exactly one segment. `extends = Parent` → two or more segments.
- **Parent match.** When `extends = Parent`, the parent segment in `schema_id` must equal `Parent::SCHEMA_ID`. Parent must have exactly one generic parameter.
- **Struct shape.** Only named-field structs and unit structs are accepted. Tuple structs, enums, and unions are rejected.
- **Generics.** At most one type parameter per struct (single-chain inheritance).
- **Identity fields.** Exactly one of `#[gts(type_field)]` / `#[gts(instance_id)]` on every root struct. Neither on any derived struct. Duplicates on the same struct are rejected. Field type must match (`GtsSchemaId` / `GtsInstanceId`).
- **Direct serde on nested structs.** A derived struct that also derives `Serialize` or `Deserialize` gets a trait-conflict error from the `GtsNoDirectSerialize` / `GtsNoDirectDeserialize` marker traits.

The full set of compile-fail fixtures lives in `tests/v2_compile_fail/`.

---

## Legacy `#[struct_to_gts_schema]`

The attribute-macro form `#[struct_to_gts_schema(...)]` predates the derive and remains fully functional. It is not yet marked `#[deprecated]` — deprecation and eventual removal are a separate, downstream-coordinated effort.

New code should use `#[derive(GtsSchema)]`. To port existing code, follow the [migration guide](../docs/002-struct-to-gts-schema-migration.md):

- `base = true` → no `extends`
- `base = Parent` → `extends = Parent`
- `properties = "a,b,c"` → drop; fields are auto-derived, use `#[gts(skip)]` to exclude
- identity field (previously required to be named `id` / `type` / `gts_type` / …) → annotate with `#[gts(type_field)]` or `#[gts(instance_id)]`; the field name is now free

Schema output is structurally identical between the two macros with one spec-correctness improvement: identity fields emit `"x-gts-ref": "/$id"` instead of the generic `"gts.*"`. The parity suite in `tests/v2_parity_tests.rs` exercises this equivalence directly.

---

## License

Apache-2.0

## See also

- [ADR-001 — Derive macro design](../docs/001-gts-schema-derive-macro-adr.md)
- [Migration guide](../docs/002-struct-to-gts-schema-migration.md)
- [GTS Specification](https://github.com/GlobalTypeSystem/gts-spec)
- [GTS CLI](../gts-cli/README.md)
