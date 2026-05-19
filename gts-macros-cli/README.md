# gts-macros-cli

Demo CLI for `gts-macros`: renders the in-repo inheritance chain
(`BaseEventV1 → AuditPayloadV1 → PlaceOrderDataV1 → PlaceOrderDataPayloadV1`)
as JSON Schemas, prints a sample serialised event, and optionally dumps
everything to disk for inspection and downstream validation.

## Usage

Print the demo output (schemas, sample event, `gts_schema_for!` cases)
to stdout:

```bash
cargo run -p gts-macros-cli
```

Dump artifacts to a directory:

```bash
cargo run -p gts-macros-cli -- --dump <DIR>
```

The dump produces:

| File | Tracked? | Purpose |
|---|---|---|
| `<schema_id>.schema.json` × 4 | yes | One JSON Schema per chain level, committed under `src/schemas/` as fixtures and as the document that downstream tools (e.g. `validate.sh`) consume. |
| `<uuid>.json` | no | Sample event built by `create_sample_event()` with hard-coded UUIDs; serves as the `-d <instance>` input for `validate.sh`. |
| `validate.sh` | no | Generated `ajv-cli validate` invocation against the sample event. Requires `node` and `ajv-cli` + `ajv-formats` available via `npx`. |

Anything other than `*.schema.json` inside `src/schemas/` is gated by a
directory-local `.gitignore`; running `--dump` repeatedly never adds
clutter to `git status`.
