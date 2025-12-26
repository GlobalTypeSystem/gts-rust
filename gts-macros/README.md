# GTS Macros

Procedural macros for GTS (Global Type System) schema generation from Rust structs.

## Overview

The `#[struct_to_gts_schema]` attribute macro serves **three purposes**:

1. **Compile-Time Validation** - Catches configuration errors before runtime
2. **Schema Generation** - Enables CLI-based JSON Schema file generation
3. **Runtime API** - Provides schema access, instance ID generation, and schema composition capabilities at runtime

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
gts-macros = { path = "path/to/gts-rust/gts-macros" }
serde = { version = "1.0", features = ["derive"] }
```

## Quick Start

```rust
use gts_macros::struct_to_gts_schema;
use uuid::Uuid;
use gts::gts::{GtsInstanceId, GtsSchemaId};

// Base event type (root of the hierarchy)
// Note: #[derive(Serialize, Deserialize, JsonSchema)] is added automatically!
#[derive(Debug)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type with common fields",
    properties = "id,tenant_id,payload"
)]
pub struct BaseEventV1<P> {
    pub id: Uuid,
    pub r#type: GtsSchemaId,
    pub tenant_id: Uuid,
    pub payload: P,
}

// Audit event that inherits from BaseEventV1
#[derive(Debug)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event with user context",
    properties = "user_id,action"
)]
pub struct AuditEventV1 {
    pub user_id: Uuid,
    pub action: String,
}

// Runtime usage:
fn example() {
    // Access schema constants
    let base_schema = BaseEventV1::<()>::GTS_JSON_SCHEMA_WITH_REFS;
    let audit_schema = AuditEventV1::GTS_JSON_SCHEMA_WITH_REFS;

    // Generate instance IDs
    let event_id = AuditEventV1::gts_make_instance_id("evt-12345.v1");
    assert_eq!(event_id.as_ref(), "gts.x.core.events.type.v1~x.core.audit.event.v1~evt-12345.v1");
}
```

---

## Purpose 1: Compile-Time Validation

The macro validates your annotations at compile time, catching errors early.

### Automatic Derives

The macro automatically adds these derives to your struct:
- `serde::Serialize`
- `serde::Deserialize`
- `schemars::JsonSchema`

**Do NOT add these derives manually** - they will conflict. You can add other derives like `Debug`, `Clone`, etc.

```rust
// ✅ Correct - only add Debug, Clone, etc.
#[derive(Debug, Clone)]
#[struct_to_gts_schema(...)]
pub struct MyStructV1 { ... }

// ❌ Wrong - will cause duplicate implementation errors
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[struct_to_gts_schema(...)]
pub struct MyStructV1 { ... }
```

### What Gets Validated

| Check | Description |
|-------|-------------|
| **Required parameters** | All of `dir_path`, `base`, `schema_id`, `description`, `properties` must be present |
| **Base consistency** | `base = true` requires single-segment schema_id; `base = Parent` requires multi-segment |
| **Parent schema match** | When `base = Parent`, Parent's SCHEMA_ID must match the parent segment in schema_id |
| **Property existence** | Every property in the list must exist as a field in the struct |
| **Struct type** | Only structs with named fields are supported (no tuple structs) |
| **Generic type constraints** | Generic type parameters must implement `GtsSchema` (only `()` or other GTS structs allowed) |
| **Base struct field validation** | Base structs (`base = true`) must have either ID fields OR GTS Type fields, but not both (see below) |

### Compile Error Examples

**Missing property:**
```rust
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event",
    properties = "id,nonexistent"  // ❌ Error!
)]
pub struct BaseEventV1<P> {
    pub id: Uuid,
    pub r#type: GtsSchemaId,
    pub payload: P,
}
```
```
error: struct_to_gts_schema: Property 'nonexistent' not found in struct.
       Available fields: ["id", "payload"]
```

**Base mismatch (base = true with multi-segment schema_id):**
```rust
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,  // ❌ Error! base = true requires single-segment
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event",
    properties = "user_id"
)]
pub struct AuditEventV1 { /* ... */ }
```
```
error: struct_to_gts_schema: base = true requires single-segment schema_id,
       but found 2 segments
```

**Parent schema ID mismatch:**
```rust
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = WrongParent,  // ❌ Error! Parent's SCHEMA_ID doesn't match
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event",
    properties = "user_id"
)]
pub struct AuditEventV1 { /* ... */ }
```
```
error: struct_to_gts_schema: Base struct 'WrongParent' schema ID must match
       parent segment 'gts.x.core.events.type.v1~' from schema_id
```

**Tuple struct:**
```rust
#[struct_to_gts_schema(/* ... */)]
pub struct Data(String);  // ❌ Tuple struct not supported
```
```
error: struct_to_gts_schema: Only structs with named fields are supported
```

**Non-GTS struct as generic argument:**
```rust
// Regular struct without struct_to_gts_schema
pub struct MyStruct { pub some_id: String }

// Using it as generic argument fails
let event: BaseEventV1<MyStruct> = BaseEventV1 { /* ... */ };  // ❌ Error!
```
```
error[E0277]: the trait bound `MyStruct: GtsSchema` is not satisfied
  --> src/main.rs:10:17
   |
10 |     let event: BaseEventV1<MyStruct> = BaseEventV1 { ... };
   |                ^^^^^^^^^^^^^^^^^^^^^ the trait `GtsSchema` is not implemented for `MyStruct`
```

**Base struct field validation - ID fields:**
```rust
use gts::gts::GtsInstanceId;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Base event with ID field",
    properties = "id,name"
)]
pub struct BaseEventTopicV1<P> {
    pub id: GtsInstanceId,  // ✅ Valid ID field
    pub name: String,
    pub payload: P,
}
```

**Base struct field validation - GTS Type fields:**
```rust
use gts::gts::GtsSchemaId;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event with type field",
    properties = "r#type,name"
)]
pub struct BaseEventV1<P> {
    pub id: Uuid,             // Event UUID
    pub r#type: GtsSchemaId,  // Event Type - ✅ Valid GTS Type field
    pub name: String,
    pub payload: P,
}
```

**Invalid base struct - both ID and GTS Type fields:**
```rust
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.topic.v1~",
    description = "Invalid base with both ID and type",
    properties = "id,r#type,name"  // ❌ Error! Both ID and GTS Type fields
)]
pub struct BaseEventV1<P> {
    pub id: GtsInstanceId,     // Event topic ID field
    pub r#type: GtsSchemaId,   // Event type (schema) ID field - ❌ Cannot have both!
```

**Invalid base struct - wrong GTS Type field type:**
```rust
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event with wrong type field",
    properties = "r#type,name"
)]
pub struct BaseEventV1<P> {
    pub id: Uuid,        // Event UUID
    pub r#type: String,  // Event type (schema) - ❌ Should be GtsSchemaId
    pub name: String,
    pub payload: P,
}
```
```
error: struct_to_gts_schema: Base structs with GTS Type fields must have at least one GTS Type field (type, gts_type, gtsType, or schema) of type GtsSchemaId
```

### Base Struct Field Validation Rules

Base structs (`base = true`) must follow **exactly one** of these patterns:

#### Option 1: ID Fields
- **Supported field names**: `$id`, `id`, `gts_id`, `gtsId`
- **Required type**: `GtsInstanceId` (or `gts::GtsInstanceId`)
- **Use case**: Instance-based identification

#### Option 2: GTS Type Fields
- **Supported field names**: `type`, `r#type`, `gts_type`, `gtsType`, `schema`
- **Supported serde renames**: Fields with `#[serde(rename = "type")]`, `#[serde(rename = "gts_type")]`, `#[serde(rename = "gtsType")]`, or `#[serde(rename = "schema")]`
- **Required type**: `GtsSchemaId` (or `gts::GtsSchemaId`)
- **Use case**: Schema-based identification

**Serde rename example:**
```rust
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event with serde(rename = \"type\")",
    properties = "event_type,id,tenant_id,sequence_id,payload"
)]
pub struct BaseEventV1<P> {
    #[serde(rename = "type")]
    pub event_type: GtsSchemaId,  // ✅ Valid - renamed to "type"
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sequence_id: u64,
    pub payload: P,
}
```

**Important**: Base structs cannot have both ID fields AND GTS Type fields. They must choose one approach.

---

## Purpose 2: Schema Generation

Generate JSON Schema files using the GTS CLI tool.

### Generate Schemas

```bash
# Using paths from macro (relative to source files)
gts generate-from-rust --source src/

# Override output directory
gts generate-from-rust --source src/ --output schemas/

# Exclude specific directories (can be used multiple times)
gts generate-from-rust --source . --exclude "tests/*" --exclude "examples/*"

# Using cargo
cargo run --bin gts -- generate-from-rust --source src/
```

### Excluding Files

The CLI provides multiple ways to exclude files from scanning:

**1. `--exclude` option** (supports glob patterns):
```bash
gts generate-from-rust --source . --exclude "tests/*" --exclude "benches/*"
```

**2. Auto-ignored directories**: The following directories are automatically skipped:
- `compile_fail/` - trybuild compile-fail tests

**3. `// gts:ignore` directive**: Add this comment at the top of any `.rs` file:
```rust
// gts:ignore
//! This file will be skipped by the CLI

use gts_macros::struct_to_gts_schema;
// ...
```

### What the CLI Does

1. Scans source files for `#[struct_to_gts_schema]` annotations
2. Extracts metadata (schema_id, description, properties)
3. Maps Rust types to JSON Schema types
4. Generates valid JSON Schema files at the specified `dir_path/<schema_id>.schema.json`

### Generated Schema Examples

**Base event type** (`schemas/gts.x.core.events.type.v1~.schema.json`):

```json
{
  "$id": "gts://gts.x.core.events.type.v1~",
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "BaseEventV1",
  "type": "object",
  "description": "Base event type with common fields",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "tenant_id": { "type": "string", "format": "uuid" },
    "payload": { "type": "object" }
  },
  "required": ["id", "tenant_id", "payload"]
}
```

**Inherited audit event** (`schemas/gts.x.core.events.type.v1~x.core.audit.event.v1~.schema.json`):

```json
{
  "$id": "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~",
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "AuditEventV1",
  "type": "object",
  "description": "Audit event with user context",
  "allOf": [
    { "$ref": "gts://gts.x.core.events.type.v1~" },
    {
      "properties": {
        "payload": {
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "user_id": { "type": "string", "format": "uuid" },
            "action": { "type": "string" }
          },
          "required": ["user_id", "action"]
        }
      }
    }
  ]
}
```

### Type Mapping

The CLI automatically maps Rust types to JSON Schema types:

| Rust Type | JSON Schema Type | Format | Required |
|-----------|------------------|--------|----------|
| `String`, `&str` | `string` | - | Yes |
| `i8`-`i128`, `u8`-`u128` | `integer` | - | Yes |
| `f32`, `f64` | `number` | - | Yes |
| `bool` | `boolean` | - | Yes |
| `Vec<T>` | `array` | - | Yes |
| `Option<T>` | Same as `T` | - | **No** |
| `Uuid` | `string` | `uuid` | Yes |
| `DateTime`, `NaiveDateTime` | `string` | `date-time` | Yes |
| `NaiveDate` | `string` | `date` | Yes |
| `HashMap<K,V>`, `BTreeMap<K,V>` | `object` | - | Yes |
| `GtsInstanceId` | `string` | `gts-instance-id` | Yes |
| `GtsSchemaId` | `string` | `gts-schema-id` | Yes |

**Notes**:
- `Option<T>` fields are not marked as `required` in the generated schema
- Generic type parameters (e.g., `P` in `BaseEventV1<P>`) are mapped to `{"type": "object"}` placeholders

---

## Purpose 3: Runtime API

The macro generates associated constants, methods, and implements the `GtsSchema` trait for runtime use.

### Getting Schema IDs

**Get the struct's GTS schema ID:**

```rust
// Using gts_schema_id() - returns &'static GtsSchemaId
let schema_id: &gts::gts::GtsSchemaId = AuditEventV1::gts_schema_id();
println!("Schema ID: {}", schema_id.as_ref());
// Output: gts.x.core.events.type.v1~x.core.audit.event.v1~

// For generic structs, use () as type parameter
let base_id = BaseEventV1::<()>::gts_schema_id();
println!("Base schema ID: {}", base_id.as_ref());
// Output: gts.x.core.events.type.v1~
```

**Get the parent (base) schema ID:**

```rust
// Using gts_base_schema_id() - returns Option<&'static GtsSchemaId>
let parent_id: Option<&gts::gts::GtsSchemaId> = AuditEventV1::gts_base_schema_id();
match parent_id {
    Some(id) => println!("Parent schema ID: {}", id.as_ref()),
    // Output: gts.x.core.events.type.v1~
    None => println!("This is a base struct (no parent)"),
}

// Base structs return None
assert!(BaseEventV1::<()>::gts_base_schema_id().is_none());

// Child structs return Some(&GtsSchemaId)
assert_eq!(
    AuditEventV1::gts_base_schema_id().map(|id| id.as_ref()),
    Some("gts.x.core.events.type.v1~")
);
```

### Getting Schemas

**Get the struct's JSON schema with references:**

```rust
use gts::GtsSchema;

// Using gts_schema_with_refs() - returns serde_json::Value
let schema_value = AuditEventV1::gts_schema_with_refs();
println!("Schema $id: {}", schema_value["$id"]);
// Output: gts://gts.x.core.events.type.v1~x.core.audit.event.v1~

// Using gts_schema_with_refs_as_string() - returns compact JSON String
let schema_json = AuditEventV1::gts_schema_with_refs_as_string();
println!("Schema JSON: {}", schema_json);

// Using gts_schema_with_refs_as_string_pretty() - returns pretty-printed JSON String
let schema_pretty = AuditEventV1::gts_schema_with_refs_as_string_pretty();
println!("Pretty schema:\n{}", schema_pretty);

// For generic structs, the type parameter doesn't affect the schema
// Both generate identical schemas:
let schema1 = BaseEventV1::<()>::gts_schema_with_refs();
let schema2 = BaseEventV1::<AuditEventV1>::gts_schema_with_refs();
assert_eq!(schema1, schema2);  // OK. Identical schemas
```

**Schema structure:**

- **Base structs** (single-segment schema_id): Direct properties, no `allOf`
  ```json
  {
    "$id": "gts://gts.x.core.events.type.v1~",
    "type": "object",
    "additionalProperties": false,
    "properties": { /* ... */ }
  }
  ```

- **Child structs** (multi-segment schema_id): Uses `allOf` with `$ref` to parent
  ```json
  {
    "$id": "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~",
    "type": "object",
    "allOf": [
      { "$ref": "gts://gts.x.core.events.type.v1~" },
      { "properties": { /* child-specific properties */ } }
    ]
  }
  ```

### Serialization & Deserialization

**Important**: Nested payload structs (like `AuditPayloadV1`, `PlaceOrderDataV1`) should never be serialized/deserialized alone. Always serialize/deserialize the complete event hierarchy starting from the base event.

**Serialize complete event instances to JSON:**

```rust
use serde::{Serialize, Deserialize};
use uuid::Uuid;

// Create a complete event with nested payloads
// BaseEventV1 -> AuditPayloadV1 -> PlaceOrderDataV1
let event = BaseEventV1 {
    event_type: PlaceOrderDataV1::gts_schema_id().clone(),
    id: Uuid::new_v4(),
    tenant_id: Uuid::new_v4(),
    sequence_id: 42,
    payload: AuditPayloadV1 {
        user_agent: "Mozilla/5.0".to_string(),
        user_id: Uuid::new_v4(),
        ip_address: "192.168.1.1".to_string(),
        data: PlaceOrderDataV1 {
            order_id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
        },
    },
};

// Using GTS instance methods (recommended):
// gts_instance_json() - returns serde_json::Value
let json_value = event.gts_instance_json();
println!("Value: {:?}", json_value);

// gts_instance_json_as_string() - returns compact JSON String
let json_string = event.gts_instance_json_as_string();
println!("JSON: {}", json_string);

// gts_instance_json_as_string_pretty() - returns pretty-printed JSON String
let json_pretty = event.gts_instance_json_as_string_pretty();
println!("Pretty JSON:\n{}", json_pretty);

// You can also use serde_json directly:
let json_string = serde_json::to_string(&event).unwrap();
let json_pretty = serde_json::to_string_pretty(&event).unwrap();
let json_value = serde_json::to_value(&event).unwrap();
```

**Deserialize JSON to complete event instances:**

```rust
// Deserialize from JSON string
let json_str = r#"{
    "type": "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "tenant_id": "660e8400-e29b-41d4-a716-446655440000",
    "sequence_id": 42,
    "payload": {
        "user_agent": "Mozilla/5.0",
        "user_id": "770e8400-e29b-41d4-a716-446655440000",
        "ip_address": "192.168.1.1",
        "data": {
            "order_id": "880e8400-e29b-41d4-a716-446655440000",
            "product_id": "990e8400-e29b-41d4-a716-446655440000"
        }
    }
}"#;

let event: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>> =
    serde_json::from_str(json_str).unwrap();
println!("Deserialized: {:?}", event);

// Deserialize from serde_json::Value
let json_value = serde_json::json!({
    "type": "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "tenant_id": "660e8400-e29b-41d4-a716-446655440000",
    "sequence_id": 42,
    "payload": {
        "user_agent": "Mozilla/5.0",
        "user_id": "770e8400-e29b-41d4-a716-446655440000",
        "ip_address": "192.168.1.1",
        "data": {
            "order_id": "880e8400-e29b-41d4-a716-446655440000",
            "product_id": "990e8400-e29b-41d4-a716-446655440000"
        }
    }
});

let event: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>> =
    serde_json::from_value(json_value).unwrap();
println!("Deserialized: {:?}", event);

// Deserialize from reader (file, network stream, etc.)
use std::fs::File;
let file = File::open("event.json").unwrap();
let event: BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>> =
    serde_json::from_reader(file).unwrap();
```

**Working with different payload types:**

```rust
// You can use different payload combinations with the same base event
type OrderEvent = BaseEventV1<AuditPayloadV1<PlaceOrderDataV1>>;
type SimpleEvent = BaseEventV1<()>;

// Create and serialize different event types
let order_event: OrderEvent = BaseEventV1 {
    event_type: PlaceOrderDataV1::gts_schema_id().clone(),
    id: Uuid::new_v4(),
    tenant_id: Uuid::new_v4(),
    sequence_id: 1,
    payload: AuditPayloadV1 {
        user_agent: "Mozilla/5.0".to_string(),
        user_id: Uuid::new_v4(),
        ip_address: "192.168.1.1".to_string(),
        data: PlaceOrderDataV1 {
            order_id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
        },
    },
};

let json = serde_json::to_string_pretty(&order_event).unwrap();
println!("Order event JSON:\n{}", json);

// Deserialize back to the correct type
let deserialized: OrderEvent = serde_json::from_str(&json).unwrap();
```

### Generating Instance IDs

Generate instance IDs by appending a segment to the schema ID:

```rust
// Generate event instance ID
let topic_id = AuditEventTopicV1::gts_make_instance_id("x.core.audit.topic.v1");
assert_eq!(
    topic_id.as_ref(),
    "gts.x.core.events.topic.v1~x.core.audit.topic.v1"
);

// Generate base event instance ID
let base_id = BaseEventTopicV1::<()>::gts_make_instance_id("a.b.c.topic.v1");
assert_eq!(base_id.as_ref(), "gts.x.core.events.topic.v1~a.b.c.topic.v1");

// Convert to String when needed
let id_string: String = topic_id.clone().into();

// Use as map key (implements Hash, Eq, Clone)
use std::collections::HashMap;
let mut events: HashMap<gts::GtsInstanceId, String> = HashMap::new();
events.insert(event_id, "processed".to_owned());

// Serialize/deserialize instance IDs
let id_json = serde_json::to_string(&event_id).unwrap();
let id_back: gts::GtsInstanceId = serde_json::from_str(&id_json).unwrap();
```

### Schema Composition & Inheritance (`GtsSchema` Trait)

The macro automatically implements the `GtsSchema` trait, enabling runtime schema composition for nested generic types:

```rust
use gts::GtsSchema;

// Get composed schema for nested type
let schema = BaseEventV1::<AuditPayloadV1<PlaceOrderDataV1>>::gts_schema_with_refs_allof();

// The schema will have proper nesting:
// - payload field contains AuditPayloadV1's schema
// - payload.data field contains PlaceOrderDataV1's schema
// - All with additionalProperties: false for type safety
```

**Generic Field Type Safety**: Generic fields (fields that accept nested types) automatically have `additionalProperties: false` set. This ensures:
- ✅ Only properly nested inherited structs can be used as values
- ✅ No arbitrary extra properties can be added to generic fields
- ✅ Type safety is enforced at the JSON Schema level

### Complete Runtime API Reference

| API | Type | Description |
|-----|------|-------------|
| `gts_schema_id()` | `&'static GtsSchemaId` | Get the struct's GTS schema ID |
| `gts_base_schema_id()` | `Option<&'static GtsSchemaId>` | Get parent schema ID (None for base structs) |
| `gts_schema_with_refs()` | `serde_json::Value` | Get schema as JSON value with `$ref` |
| `gts_schema_with_refs_as_string()` | `String` | Get schema as compact JSON string |
| `gts_schema_with_refs_as_string_pretty()` | `String` | Get schema as pretty-printed JSON string |
| `gts_instance_json(&self)` | `serde_json::Value` | Serialize instance to JSON value |
| `gts_instance_json_as_string(&self)` | `String` | Serialize instance to compact JSON string |
| `gts_instance_json_as_string_pretty(&self)` | `String` | Serialize instance to pretty-printed JSON string |
| `gts_make_instance_id(segment)` | `GtsInstanceId` | Generate instance ID by appending segment |

---

## Macro Parameters

All parameters are **required** (5 total):

| Parameter | Description | Example |
|-----------|-------------|---------|
| `dir_path` | Output directory for generated schema | `"schemas"` |
| `base` | Inheritance declaration (see below) | `true` or `ParentStruct` |
| `schema_id` | GTS identifier | `"gts.x.app.entities.user.v1~"` |
| `description` | Human-readable description | `"User entity"` |
| `properties` | Comma-separated field list | `"id,email,name"` |

### The `base` Attribute

The `base` attribute explicitly declares the struct's position in the inheritance hierarchy:

| Value | Meaning | Schema ID Requirement |
|-------|---------|----------------------|
| `base = true` | This is a root/base type (no parent) | Single-segment (e.g., `gts.x.core.events.type.v1~`) |
| `base = ParentStruct` | This inherits from `ParentStruct` | Multi-segment (e.g., `gts.x.core.events.type.v1~x.core.audit.event.v1~`) |

**Compile-time validation**: The macro validates that:
- `base = true` requires a single-segment `schema_id`
- `base = ParentStruct` requires a multi-segment `schema_id` where the parent segment matches `ParentStruct`'s `SCHEMA_ID`

### GTS ID Format

```
gts.<vendor>.<package>.<namespace>.<type>.v<MAJOR>[.<MINOR>]~
```

Examples:
- `gts.x.core.iam.user.v1~` - IAM user schema
- `gts.x.commerce.orders.order.v1.0~` - Order schema with minor version

---

## Complete Example

### Define Event Type Hierarchy

```rust
// src/events.rs
use gts_macros::struct_to_gts_schema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Base event type - the root of all events
#[derive(Debug, Serialize, Deserialize)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.events.type.v1~",
    description = "Base event type with common fields",
    properties = "id,tenant_id,timestamp,payload"
)]
pub struct BaseEventV1<P> {
    pub id: Uuid,
    pub r#type: GtsSchemaId,
    pub tenant_id: Uuid,
    pub timestamp: String,
    pub payload: P,
}

// Audit event - extends BaseEventV1 with user context
#[derive(Debug, Serialize, Deserialize)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = BaseEventV1,
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~",
    description = "Audit event with user tracking",
    properties = "user_id,ip_address,action"
)]
pub struct AuditEventV1<D> {
    pub user_id: Uuid,
    pub ip_address: String,
    pub action: D,
}

// Order placed event - extends AuditEventV1 for order actions
#[derive(Debug, Serialize, Deserialize)]
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AuditEventV1,
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.shop.orders.placed.v1~",
    description = "Order placement event",
    properties = "order_id,total"
)]
pub struct OrderPlacedV1 {
    pub order_id: Uuid,
    pub total: f64,
}
```

### Generate Schemas

```bash
gts generate-from-rust --source src/
# Output:
#   Generated schema: gts.x.core.events.type.v1~ @ schemas/...
#   Generated schema: gts.x.core.events.type.v1~x.core.audit.event.v1~ @ schemas/...
#   Generated schema: gts.x.core.events.type.v1~x.core.audit.event.v1~x.shop.orders.placed.v1~ @ schemas/...
```

### Use at Runtime

```rust
fn main() {
    // Access schemas at any level
    println!("Base event schema: {}", BaseEventV1::<()>::GTS_JSON_SCHEMA_WITH_REFS);
    println!("Audit event schema: {}", AuditEventV1::<()>::GTS_JSON_SCHEMA_WITH_REFS);
    println!("Order placed schema: {}", OrderPlacedV1::GTS_JSON_SCHEMA_WITH_REFS);

    // Generate instance IDs
    let event_id = OrderPlacedV1::gts_make_instance_id("evt-12345.v1");
    println!("Event ID: {}", event_id);
    // Output: gts.x.core.events.type.v1~x.core.audit.event.v1~x.shop.orders.placed.v1~evt-12345.v1

    // Use as HashMap key
    use std::collections::HashMap;
    let mut events: HashMap<gts::GtsInstanceId, String> = HashMap::new();
    events.insert(event_id, "processed".to_owned());
}
```

---

## Schema Inheritance & Compile-Time Guarantees

The macro supports **explicit inheritance declaration** through the `base` attribute and provides **compile-time validation** to ensure parent-child relationships are correct.

### Inheritance Example

See `tests/inheritance_tests.rs` for a complete working example:

```rust
// Base event type (base = true, single-segment schema_id)
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

// Extends BaseEventV1 (base = ParentStruct, multi-segment schema_id)
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

// Extends AuditPayloadV1 (3-level inheritance chain)
#[struct_to_gts_schema(
    dir_path = "schemas",
    base = AuditPayloadV1,
    schema_id = "gts.x.core.events.type.v1~x.core.audit.event.v1~x.marketplace.orders.purchase.v1~",
    description = "Order placement audit event",
    properties = "order_id,product_id"
)]
pub struct PlaceOrderDataV1 {
    pub order_id: Uuid,
    pub product_id: Uuid,
}
```

### Generated Schemas

**Single-segment schema** (no inheritance):
```json
{
  "$id": "gts://gts.x.core.events.type.v1~",
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "BaseEventV1",
  "type": "object",
  "description": "Base event type definition",
  "properties": { /* direct properties */ },
  "required": [ /* required fields */ ]
}
```

**Multi-segment schema** (with inheritance):
```json
{
  "$id": "gts://gts.x.core.events.type.v1~x.core.audit.event.v1~",
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "AuditPayloadV1",
  "type": "object",
  "description": "Audit event with user context",
  "allOf": [
    { "$ref": "gts://gts.x.core.events.type.v1~" },
    {
      "properties": {
        "payload": {
          "type": "object",
          "additionalProperties": false,
          "properties": { /* child-specific properties */ },
          "required": [ /* child-specific required fields */ ]
        }
      }
    }
  ]
}
```

**Important**: Generic fields (fields that accept nested types) automatically have `additionalProperties: false` set. This ensures that only properly nested inherited structs can be used, preventing arbitrary extra properties from being added to generic fields.

### Compile-Time Guarantees

The macro validates your configuration at compile time, preventing runtime errors:

| ✅ Guaranteed | ❌ Prevented |
|--------------|-------------|
| **All required attributes exist** | Missing `dir_path`, `base`, `schema_id`, `description`, or `properties` |
| **Base attribute consistency** | `base = true` with multi-segment schema_id, or `base = Parent` with single-segment |
| **Parent schema ID match** | `base = Parent` where Parent's SCHEMA_ID doesn't match the parent segment |
| **Properties exist in struct** | Referencing non-existent fields in `properties` list |
| **Valid struct types** | Tuple structs, unit structs, enums |
| **Single generic parameter** | Multiple type generics (prevents inheritance ambiguity) |
| **Valid GTS ID format** | Malformed schema identifiers |
| **Memory efficiency** | No unnecessary allocations in generated constants |
| **Strict generic field validation** | Generic fields have `additionalProperties: false` to ensure only nested inherited structs are allowed |
| **GTS-only generic arguments** | Using non-GTS structs as generic type parameters (see below) |

### Generic Type Parameter Constraints

The macro automatically adds a `GtsSchema` trait bound to all generic type parameters. This ensures that only valid GTS types can be used as generic arguments:

```rust
// ✅ Allowed: () is a valid GTS type (terminates the chain)
let event: BaseEventV1<()> = BaseEventV1 { /* ... */ };

// ✅ Allowed: AuditPayloadV1 has struct_to_gts_schema applied
let event: BaseEventV1<AuditPayloadV1<()>> = BaseEventV1 { /* ... */ };

// ❌ Compile error: MyStruct does not implement GtsSchema
pub struct MyStruct { pub some_id: String }
let event: BaseEventV1<MyStruct> = BaseEventV1 { /* ... */ };
// error: the trait bound `MyStruct: GtsSchema` is not satisfied
```

This prevents accidental use of arbitrary structs that haven't been properly annotated with `struct_to_gts_schema`, ensuring type safety across the entire GTS inheritance chain.

### Generic Fields and `additionalProperties`

When a struct has a generic type parameter (e.g., `BaseEventV1<P>` with field `payload: P`), the generated schema sets `additionalProperties: false` on that field's schema. This ensures:

- ✅ Only properly nested inherited structs can be used as values
- ✅ No arbitrary extra properties can be added to generic fields
- ✅ Type safety is enforced at the JSON Schema level

Example:
```json
{
  "properties": {
    "payload": {
      "type": "object",
      "additionalProperties": false,
      "properties": { /* nested schema */ }
    }
  }
}
```

### Schema Constants

The macro generates two schema variants with **zero runtime allocation**:

- **`GTS_JSON_SCHEMA_WITH_REFS`**: Uses `$ref` in `allOf` (most memory-efficient)
- **`GTS_JSON_SCHEMA_INLINE`**: Currently identical; true inlining requires runtime resolution

```rust
// Both are compile-time constants - no allocation at runtime!
let schema_with_refs = AuditEventV1::<()>::GTS_JSON_SCHEMA_WITH_REFS;
let schema_inline = AuditEventV1::<()>::GTS_JSON_SCHEMA_INLINE;

// Runtime schema resolution (when true inlining is needed)
use gts::GtsStore;
let store = GtsStore::new();
let inlined_schema = store.resolve_schema(&schema_with_refs)?;
```

---

## Security Features

The CLI includes security checks:

1. **Path traversal prevention** - Cannot write files outside the source repository
2. **File extension enforcement** - Both macro and CLI validate `.json` extension
3. **Canonicalization** - Resolves symbolic links to prevent escapes

---

## License

Apache-2.0

## See Also

- [GTS Specification](https://github.com/globaltypesystem/gts-spec)
- [GTS CLI Documentation](../gts-cli/README.md)
