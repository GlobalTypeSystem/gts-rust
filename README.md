# GTS Rust Implementation

A complete Rust implementation of the Global Type System (GTS)

## Overview

GTS (Global Type System)[https://github.com/globaltypesystem/gts-spec] is a simple, human-readable, globally unique identifier and referencing system for data type definitions (e.g., JSON Schemas) and data instances (e.g., JSON objects). This Rust implementation provides high-performance, type-safe operations for working with GTS identifiers.

## Roadmap

Featureset:

- [x] **OP#1 - ID Validation**: Verify identifier syntax using regex patterns
- [x] **OP#2 - ID Extraction**: Fetch identifiers from JSON objects or JSON Schema documents
- [x] **OP#3 - ID Parsing**: Decompose identifiers into constituent parts (vendor, package, namespace, type, version, etc.)
- [x] **OP#4 - ID Pattern Matching**: Match identifiers against patterns containing wildcards
- [x] **OP#5 - ID to UUID Mapping**: Generate deterministic UUIDs from GTS identifiers
- [x] **OP#6 - Schema Validation**: Validate object instances against their corresponding schemas
- [x] **OP#7 - Relationship Resolution**: Load all schemas and instances, resolve inter-dependencies, and detect broken references
- [x] **OP#8 - Compatibility Checking**: Verify that schemas with different MINOR versions are compatible
- [x] **OP#8.1 - Backward compatibility checking**
- [x] **OP#8.2 - Forward compatibility checking**
- [x] **OP#8.3 - Full compatibility checking**
- [x] **OP#9 - Version Casting**: Transform instances between compatible MINOR versions
- [x] **OP#10 - Query Execution**: Filter identifier collections using the GTS query language
- [x] **OP#11 - Attribute Access**: Retrieve property values and metadata using the attribute selector (`@`)

See details in [gts/README.md](gts/README.md)

Other features:

- [x] **Web server** - a non-production web-server with REST API for the operations processing and testing
- [x] **CLI** - command-line interface for all GTS operations
- [ ] **UUID for instances** - to support UUID as ID in JSON instances
- [x] **YAML support** - to support YAML files (*.yml, *.yaml) as input files
- [ ] **TypeSpec support** - Add [typespec.io](https://typespec.io/) files (*.tsp) support

Technical Backlog:

- [x] **Code coverage** - target is 90%
- [ ] **Documentation** - add documentation for all the features
- [ ] **Interface** - export publicly available interface and keep cli and others private
- [ ] **Server API** - finalise the server API
- [ ] **Final code cleanup** - remove unused code, denormalize, add critical comments, etc.


## Architecture

The project is organized as a Cargo workspace with two crates:

### `gts` (Library Crate)

Core library providing all GTS functionality:

- **gts.rs** - GTS ID parsing, validation, wildcard matching
- **entities.rs** - JSON entities, configuration, validation
- **path_resolver.rs** - JSON path resolution
- **schema_cast.rs** - Schema compatibility and casting
- **files_reader.rs** - File system scanning
- **store.rs** - Entity storage and querying
- **ops.rs** - High-level operations API

### `gts-cli` (Binary Crate)

Command-line tool and HTTP server:

- **cli.rs** - Full CLI with all commands
- **server.rs** - Axum-based HTTP server
- **main.rs** - Entry point

## Installation

### From Source

```bash
git clone https://github.com/globaltypesystem/gts-rust
cd gts-rust
cargo build --release
```

The binary will be available at `target/release/gts`.

### As a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
gts = { path = "path/to/gts-rust/gts" }
```

## Usage

### CLI Commands

#### Validate a GTS ID

```bash
gts validate-id --gts-id "gts.x.core.events.event.v1~"
```

#### Parse a GTS ID

```bash
gts parse-id --gts-id "gts.x.core.events.event.v1.2~"
```

#### Match ID Against Pattern

```bash
gts match-id-pattern --pattern "gts.x.core.events.*" --candidate "gts.x.core.events.event.v1~"
```

#### Generate UUID

```bash
gts uuid --gts-id "gts.x.core.events.event.v1~"
```

#### Validate Instance

```bash
gts validate-instance --gts-id "gts.x.core.events.event.v1.0" --path ./data
```

#### Check Schema Compatibility

```bash
gts compatibility --old-schema-id "gts.x.core.events.event.v1~" --new-schema-id "gts.x.core.events.event.v2~" --path ./schemas
```

#### Cast Instance

```bash
gts cast --from-id "gts.x.core.events.event.v1.0" --to-schema-id "gts.x.core.events.event.v2~" --path ./data
```

#### Query Entities

```bash
gts query --expr "gts.x.core.events.*[status=active]" --limit 50 --path ./data
```

#### Access Attribute

```bash
gts attr --gts-with-path "gts.x.core.events.event.v1.0@metadata.timestamp" --path ./data
```

#### List Entities

```bash
gts list --limit 100 --path ./data
```

#### Start HTTP Server

```bash
# Start server without HTTP logging (WARNING level only)
gts server --host 127.0.0.1 --port 8000 --path ./data

# Start server with HTTP request logging (-v or --verbose)
gts -v server --host 127.0.0.1 --port 8000 --path ./data

# Start server with detailed logging including request/response bodies (-vv)
gts -vv server --host 127.0.0.1 --port 8000 --path ./data
```

Verbose logging format (matches Python implementation):
- **No flag**: WARNING level only (no HTTP request logs)
- **`-v`**: INFO level - Logs HTTP requests with color-coded output:
  ```
  2025-11-07 22:43:17,105 - INFO - GET /match-id-pattern -> 200 in 0.2ms
  ```
  - Method (cyan), path (blue), status code (green/yellow/red), duration (magenta)
  - Colors are automatically disabled when output is not a TTY
- **`-vv`**: DEBUG level - Additionally logs request/response bodies with pretty-printed JSON

#### Generate OpenAPI Spec

```bash
gts openapi-spec --out openapi.json --host 127.0.0.1 --port 8000
```

### Library Usage

```rust
use gts::{GtsID, GtsOps, GtsConfig};

// Parse and validate GTS IDs
let id = GtsID::new("gts.x.core.events.event.v1~")?;
assert!(id.is_type());
println!("UUID: {}", id.to_uuid());

// Use high-level operations
let mut ops = GtsOps::new(
    Some(vec!["./data".to_string()]),
    None,
    0
);

// Validate an instance
let result = ops.validate_instance("gts.x.core.events.event.v1.0");
println!("Valid: {}", result.ok);

// Query entities
let results = ops.query("gts.x.core.*", 100);
println!("Found {} entities", results.count);
```

### HTTP API

Start the server:

```bash
gts server --host 127.0.0.1 --port 8000 --path ./data
```

Example API calls:

```bash
# Validate ID
curl "http://localhost:8000/validate-id?gts_id=gts.x.core.events.event.v1~"

# Parse ID
curl "http://localhost:8000/parse-id?gts_id=gts.x.core.events.event.v1.2~"

# Query entities
curl "http://localhost:8000/query?expr=gts.x.core.*&limit=10"

# Add entity
curl -X POST http://localhost:8000/entities \
  -H "Content-Type: application/json" \
  -d '{"gtsId": "gts.x.core.events.event.v1.0", "data": "..."}'
```

## Configuration

Create a `gts.config.json` file to customize entity ID field detection:

```json
{
  "entity_id_fields": [
    "$id",
    "gtsId",
    "gtsIid",
    "gtsOid",
    "gtsI",
    "gts_id",
    "gts_oid",
    "gts_iid",
    "id"
  ],
  "schema_id_fields": [
    "$schema",
    "gtsTid",
    "gtsType",
    "gtsT",
    "gts_t",
    "gts_tid",
    "gts_type",
    "type",
    "schema"
  ]
}
```

## GTS ID Format

GTS identifiers follow this format:

```
gts.<vendor>.<package>.<namespace>.<type>.v<MAJOR>[.<MINOR>][~]
```

- **Prefix**: Always starts with `gts.`
- **Vendor**: Organization or vendor code
- **Package**: Module or application name
- **Namespace**: Category within the package
- **Type**: Specific type name
- **Version**: Semantic version (major.minor)
- **Type Marker**: Trailing `~` indicates a schema/type (vs instance)

Examples:
- `gts.x.core.events.event.v1~` - Schema
- `gts.x.core.events.event.v1.0` - Instance
- `gts.x.core.events.type.v1~vendor.app._.custom.v1~` - Chained (inheritance)

## Testing

Run the test suite:

```bash
cargo test
```

Run with verbose output:

```bash
cargo test -- --nocapture
```

## Development

### Build

```bash
cargo build
```

### Build Release

```bash
cargo build --release
```

### Run Tests

```bash
cargo test
```

### Format Code

```bash
cargo fmt
```

### Lint

```bash
cargo clippy
```

## License

Apache-2.0

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Links

- [GTS Specification](https://github.com/globaltypesystem/gts-spec)
- [Python Implementation](https://github.com/globaltypesystem/gts-python)
- [Documentation](https://docs.rs/gts)

## Acknowledgments

This Rust implementation is based on the Python reference implementation and follows the GTS specification v0.4.
