# gts-dylint

A [Dylint](https://github.com/trailofbits/dylint) lint that flags hard-coded GTS identifier prefixes in string literals.

## What it does

String literals starting with a configured prefix (default: `"gts."`) are flagged. These should use the `GTS_ID_PREFIX` constant or the `gts_id!` macro instead, so that the prefix remains overridable at **compile time** via the `GTS_ID_PREFIX` environment variable.

The set of flagged prefixes can be customized via the `GTS_DYLINT_PREFIXES` environment variable (comma-separated, e.g. `GTS_DYLINT_PREFIXES="gts.,acme."`). Defaults to `gts.`. If `GTS_ID_PREFIX` is set, that active prefix is also flagged automatically.

This lint intentionally matches by prefix, not by fully validating each string as a GTS ID. That means non-ID strings such as `"gts.config.json"` are also flagged if they start with a configured prefix. Treat those cases as either naming collisions to avoid or suppress them locally with `#[allow(unknown_lints, gts_id_hardcoded_prefix)]`.

Expansions from the built-in GTS macros (`gts_id`, `struct_to_gts_schema`, `gts_instance`, `gts_instance_raw`) are allowed because those macros apply the configured prefix deliberately. Project-specific wrapper macros can be added via `GTS_DYLINT_ALLOWED_MACROS` (comma-separated, e.g. `GTS_DYLINT_ALLOWED_MACROS="my_gts_id,my_schema"`).

Only add trusted wrappers to `GTS_DYLINT_ALLOWED_MACROS`: allowing a macro name suppresses every prefixed string literal produced anywhere in that macro's expansion. Wrapper macros should delegate to `gts_id!` or the official GTS macros instead of emitting full hard-coded IDs themselves.

The default lint level is **deny** (compilation error). To downgrade to a warning, use `#![warn(gts_id_hardcoded_prefix)]` at the crate level or `--cap-lints warn` on the command line.

### Suggested replacements

| Pattern | Replacement |
|---------|-------------|
| `"gts.x.core.events.topic.v1~"` | `GTS_ID_PREFIX` compile-time constant from the `gts-id` crate |
| Constructing GTS IDs at compile time | `gts_id!` macro from the `gts-macros` crate |

### Suppressing

Use `#[allow(gts_id_hardcoded_prefix)]` on specific items or `#![allow(gts_id_hardcoded_prefix)]` at the crate level. Since the lint is only known when dylint is loaded, pair it with `#[allow(unknown_lints)]` to avoid "unknown lint" warnings during normal `cargo check`:

```rust
#[allow(unknown_lints, gts_id_hardcoded_prefix)]
pub const DEFAULT_GTS_ID_PREFIX: &str = "gts.";
```

For test code, add at the crate root:

```rust
#![cfg_attr(test, allow(unknown_lints, gts_id_hardcoded_prefix))]
```

## Examples

### `gts_id!` as a standalone expression

Expands to a `&'static str` literal with the configured prefix prepended at compile time:

```rust
use gts_macros::gts_id;

// With the default prefix "gts.":
let id: &str = gts_id!("x.core.events.topic.v1~");
assert_eq!(id, "gts.x.core.events.topic.v1~");
```

### `gts_id!` inside `gts_instance!`

The `gts_id!("...")` marker is recognized inside `gts_instance!` — write the suffix without the prefix:

```rust
use gts_macros::{gts_id, gts_instance};

let t: TopicV1 = gts_instance!(TopicV1 {
    id: gts_id!("x.core.events.topic.v1~vendor.app.orders.created.v1"),
    name: "orders".to_owned(),
    retention: "P30D".to_owned(),
});
```

### `gts_id!` inside `#[struct_to_gts_schema]`

The same marker works in the `type_id` argument of `#[struct_to_gts_schema]`:

```rust
use gts_macros::{struct_to_gts_schema, gts_id};

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = gts_id!("x.core.events.topic.v1~"),
    description = "Topic type",
    properties = "id,name"
)]
pub struct TopicV1 {
    pub id: gts::GtsInstanceId,
    pub name: String,
}
```

### `gts_id!` inside `gts_instance_raw!`

For JSON-shaped instances without a Rust struct:

```rust
use gts_macros::{gts_id, gts_instance_raw};

let v: serde_json::Value = gts_instance_raw!({
    "id": gts_id!("x.core.events.topic.v1~vendor.app.events.audit.v1"),
    "name": "audit",
});
```

## Requirements

- **Nightly Rust** with `rustc-dev` and `llvm-tools-preview` components:
  ```bash
  rustup toolchain install nightly
  rustup component add rustc-dev llvm-tools-preview --toolchain nightly
  ```

- **cargo-dylint** and **dylint-link**:
  ```bash
  cargo install cargo-dylint dylint-link
  ```

## Usage

### In an external project

Add to your workspace `Cargo.toml`:

```toml
[workspace.metadata.dylint]
libraries = [
    { git = "https://github.com/GlobalTypeSystem/gts-rust", tag = "v0.11.0", pattern = "gts-dylint" },
]
```

Run the lint:

```bash
cargo +nightly dylint --all
```

With a custom prefix and wrapper macros:

```bash
GTS_ID_PREFIX=acme. \
GTS_DYLINT_ALLOWED_MACROS=my_gts_id,my_schema \
cargo +nightly dylint --all
```

Run the lint with the same `GTS_ID_PREFIX` value used for build and test. The prefix is read from the Dylint process environment; if your project builds with `GTS_ID_PREFIX=acme.` but runs Dylint without that variable, hard-coded `"acme...."` literals will not be flagged automatically.

`GTS_ID_PREFIX=acme.` is enough for the lint to flag both `"gts...."` and `"acme...."` literals during that run. Set `GTS_DYLINT_PREFIXES` only when you want to scan additional legacy prefixes:

```bash
GTS_DYLINT_PREFIXES=gts.,legacy.,acme. cargo +nightly dylint --all
```

To also lint test code, examples, and benchmarks:

```bash
cargo +nightly dylint --all -- --all-targets
```

> `gts-dylint` is **not published to crates.io**. Dylint loads it as a `cdylib` via the rustc wrapper, not as a regular crate dependency, so git is the correct distribution method.

### In this repository

```bash
make dylint
```

## Testing

UI tests use [`dylint_testing`](https://docs.rs/dylint_testing):

```bash
cd gts-dylint && cargo +nightly test
```

## License

Same as the gts-rust project.
