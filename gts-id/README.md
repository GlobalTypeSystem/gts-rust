# gts-id

Validation and parsing primitives for [GTS](https://github.com/GlobalTypeSystem/gts-spec) (Global Type System) identifiers.

This crate is the single source of truth for GTS identifier parsing in [`gts-rust`](https://github.com/GlobalTypeSystem/gts-rust): it is shared by the `gts` runtime library and the `gts-macros` proc-macro crate. It has no runtime dependencies beyond `thiserror` (and optionally `uuid`).

## Identifier shape

A GTS identifier is a `~`-chained sequence of segments under the `gts.` prefix:

```text
gts.<vendor>.<package>.<namespace>.<type>.v<MAJOR>[.<MINOR>]
```

* A **type** identifier ends with a `~` marker: `gts.x.core.events.topic.v1~`
* An **instance** identifier does not: `gts.x.core.events.topic.v1~acme.shop.orders.order.v1.0`
* A **combined anonymous instance** ends with a UUID tail: `gts.x.core.events.topic.v1~7a1d2f34-5678-49ab-9012-abcdef123456`

## Types

| Type | Purpose |
|------|---------|
| `GtsId` | A validated, concrete identifier. |
| `GtsIdPattern` | A match pattern — an identifier that may end in a single trailing `*`, or be fully concrete. |
| `GtsIdSegment` | One concrete segment of a `GtsId` (`Concrete` or `UuidTail`). |
| `GtsIdPatternSegment` | One segment of a pattern (`Segment` or `Wildcard`). |
| `GtsUuidTail` | An anonymous-instance UUID tail (guaranteed well-formed). |
| `GtsIdError` | The error returned by all parsing entry points. |

Every value is produced only by the validating constructors, so a parsed value is always well-formed and its invariants cannot be forged.

## Examples

The snippets below use `?` for brevity; assume they run inside a function that returns `Result<_, gts_id::GtsIdError>`.

### Validate and parse a concrete identifier

```rust
use gts_id::GtsId;

// Fallible constructor.
let id = GtsId::try_new("gts.x.core.events.topic.v1~")?;
assert_eq!(id.id(), "gts.x.core.events.topic.v1~");
assert!(id.is_type()); // ends with '~'

// `FromStr` is also available.
let id: GtsId = "gts.x.core.events.topic.v1~".parse()?;

// Cheap validity check that doesn't keep the parsed value.
assert!(GtsId::is_valid("gts.x.core.events.topic.v1~"));
assert!(!GtsId::is_valid("gts.x.Core.events.topic.v1~")); // uppercase rejected
```

### Inspect segments

```rust
use gts_id::GtsId;

let id = GtsId::try_new("gts.x.core.events.topic.v1.2~")?;
let seg = &id.segments()[0];
assert_eq!(seg.vendor(), "x");
assert_eq!(seg.package(), "core");
assert_eq!(seg.namespace(), "events");
assert_eq!(seg.type_name(), "topic");
assert_eq!(seg.ver_major(), 1);
assert_eq!(seg.ver_minor(), Some(2));
assert!(seg.is_type());
```

### Type vs. instance, and the parent type of a chain

```rust
use gts_id::GtsId;

let instance = GtsId::try_new("gts.x.core.events.topic.v1~acme.shop.orders.order.v1.0")?;
assert!(!instance.is_type());

// The parent type id (every segment but the last).
assert_eq!(
    instance.get_type_id().as_deref(),
    Some("gts.x.core.events.topic.v1~"),
);
```

### Wildcard patterns and matching

A `GtsIdPattern` may contain a single trailing `*` (e.g. `gts.x.core.*` or `gts.x.core.events.topic.v1~*`), or be fully concrete. Concrete identifiers are validated with `GtsId`, which rejects wildcards.

```rust
use gts_id::{GtsId, GtsIdPattern};

let pattern = GtsIdPattern::try_new("gts.x.core.*")?;

let id = GtsId::try_new("gts.x.core.events.topic.v1~")?;
assert!(id.matches_pattern(&pattern));

let other = GtsId::try_new("gts.y.core.events.topic.v1~")?;
assert!(!other.matches_pattern(&pattern));

// A concrete `GtsId` never accepts a wildcard:
assert!(GtsId::try_new("gts.x.core.*").is_err());
```

### Pattern coverage

`covers` answers whether one pattern is broader than another — every identifier matched by `other` is also matched by `self`:

```rust
use gts_id::GtsIdPattern;

let broad = GtsIdPattern::try_new("gts.x.core.events.topic.v1~*")?;
let narrow = GtsIdPattern::try_new("gts.x.core.events.topic.v1~acme.*")?;
assert!(broad.covers(&narrow));
assert!(!narrow.covers(&broad));
```

### Anonymous instances (UUID tail)

```rust
use gts_id::{GtsId, GtsIdSegment};

let id = GtsId::try_new("gts.x.core.events.topic.v1~7a1d2f34-5678-49ab-9012-abcdef123456")?;

// The last segment is a UUID tail; match on it to read the UUID.
if let Some(GtsIdSegment::UuidTail(tail)) = id.segments().last() {
    assert_eq!(tail.as_str(), "7a1d2f34-5678-49ab-9012-abcdef123456");
}
```

### Error handling

```rust
use gts_id::GtsId;

let err = GtsId::try_new("gts.x.core.events.topic").unwrap_err();
// `GtsIdError` implements `Display` with a human-readable cause.
println!("{err}");
```

## Feature flags

* **`uuid`** — enables `GtsId::to_uuid()`, a deterministic UUID v5 derivation (the same identifier always maps to the same UUID). Off by default so parsing-only consumers don't pull in the `uuid` crate.

```toml
[dependencies]
gts-id = { version = "0.11", features = ["uuid"] }
```

```rust
// with the `uuid` feature enabled:
use gts_id::GtsId;

let id = GtsId::try_new("gts.x.core.events.topic.v1~")?;
let uuid = id.to_uuid();
assert_eq!(id.to_uuid(), uuid); // deterministic
```

## License

Apache-2.0
