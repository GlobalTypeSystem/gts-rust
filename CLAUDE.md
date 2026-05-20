# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Project Overview

`gts-rust` is the Rust reference implementation of [GTS](https://github.com/GlobalTypeSystem/gts-spec) — library (`gts/`), CLI and HTTP server (`gts-cli/`, binary name `gts`), plus supporting crates (`gts-id`, `gts-macros`, `gts-macros-cli`, `gts-validator`). The server answers the REST API exercised by the shared gts-spec conformance suite.

The conformance suite is shipped as a Docker image — `ghcr.io/globaltypesystem/gts-spec-tests` — and the spec version this implementation targets is pinned in `.gts-spec-version` (the file's contents are used verbatim as the image tag, format `vMAJOR.MINOR.PATCH`). The pin is immutable on purpose: every commit reproduces the same test run, and rolling forward requires a deliberate bump.

## Running the gts-spec Test Suite

The short form is `make gts-spec-tests PORT=8001` (PORT defaults to 8000; override if busy — the target fails fast if the port is already in use). It builds the release binary, pulls the runner image pinned in `.gts-spec-version`, starts the server (stdout/stderr captured to `.server.log` and dumped automatically on startup failure), runs pytest inside the container against `host.docker.internal:$PORT`, and shuts everything down. Requires a working Docker daemon.

### Useful overrides

```bash
# Single test file or pytest selector
make gts-spec-tests TEST=test_op12_type_derivation_validation.py
make gts-spec-tests TEST=test_op12_type_derivation_validation.py::TestCaseOp12_FinalBase_RejectDerived

# Opt into the rolling minor tag (picks up new patches without a commit here)
make gts-spec-tests GTS_SPEC_VERSION=v0.11

# Try a different patch without touching .gts-spec-version
make gts-spec-tests GTS_SPEC_VERSION=v0.11.0

# Iterate on the tests themselves — mount a local checkout over /tests
make gts-spec-tests GTS_SPEC_TESTS_DIR=../gts-spec/tests
```

### Iterating on a single test against a long-running server

Skip the rebuild/restart cycle when working on one test:

```bash
# Terminal 1 — start the server (debug build, fast incremental rebuilds)
make gts-server PORT=8001

# Terminal 2 — rerun targeted tests as you edit them
make gts-spec-tests-run PORT=8001 TEST=test_op12_type_derivation_validation.py
make gts-spec-tests-run PORT=8001 TEST=test_op12_type_derivation_validation.py::TestCaseOp12_FinalBase_RejectDerived

# Iterating on the test suite itself? Mount a local checkout over /tests
make gts-spec-tests-run PORT=8001 GTS_SPEC_TESTS_DIR=../gts-spec/tests TEST=...
```

The server holds state in memory with no reset endpoint — restart it between full-suite runs.

## Working in This Repo

- `.gts-spec-version` is the canonical pin (`vMAJOR.MINOR.PATCH`). Bump it (commit + push) to roll the spec forward — both CI and `make gts-spec-tests` pick it up. Local cache survives across runs; `docker rmi $(GTS_SPEC_REF)` if you ever need to force a refetch.
- Handlers in `gts-cli/src/server.rs` stay thin — logic goes in `gts/` where it is unit-testable. New REST behavior usually already has coverage in the gts-spec suite; run the relevant file (`make gts-spec-tests TEST=...`) before and after to confirm.
- `make check` is the full local gate: fmt + clippy + test + gts-spec-tests.
