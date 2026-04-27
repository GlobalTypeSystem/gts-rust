# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Project Overview

`gts-rust` is the Rust reference implementation of [GTS](https://github.com/GlobalTypeSystem/gts-spec) — library (`gts/`), CLI and HTTP server (`gts-cli/`, binary name `gts`), plus supporting crates (`gts-id`, `gts-macros`, `gts-macros-cli`, `gts-validator`). The server answers the REST API exercised by the shared gts-spec conformance suite.

`.gts-spec/` is the spec vendored as a git submodule; `tests/` inside it is the conformance suite. See `README.md` for API/CLI details and `make help` for all targets.

## Running the gts-spec Test Suite

The short form is `make e2e PORT=8001` (PORT defaults to 8000; override if busy — the target fails fast if the port is already in use). It bootstraps the venv, rebuilds, starts the server, runs pytest, and shuts everything down.

### Raise the file-descriptor limit (macOS)

`httprunner` creates a `requests.Session` per test class and never closes it, so a keep-alive socket leaks per class until pytest exits. With ~250 test classes today, the suite blows past macOS's default 256 soft cap (`ulimit -n`) and fails mid-run with `EMFILE: Too many open files`. Raise the limit in your shell — once, for the whole session:

```bash
ulimit -n 4096
```

Persist it by adding the same line to `~/.zshrc` (or `~/.bashrc`). Required for both `make e2e` and any direct `pytest` invocation against the spec suite.

### Bootstrap the venv (first time only)

Tests depend on `httprunner`. Installed into `.gts-spec/.venv/` (gitignored by the submodule).

```bash
make e2e-venv                     # uses python3 by default
make e2e-venv PYTHON=python3.11   # Python 3.11 is the safest (httprunner still pins pydantic<2)
```

### Run pytest manually against a running server

Useful when iterating on a single test without the full `make e2e` cycle.

```bash
cargo build --workspace --release
./target/release/gts server --port 8001 &

PYTEST=".gts-spec/.venv/bin/python -m pytest"

# Whole suite
$PYTEST .gts-spec/tests --gts-base-url http://127.0.0.1:8001

# One file / one class
$PYTEST .gts-spec/tests/test_refimpl_x_gts_final_abstract.py --gts-base-url http://127.0.0.1:8001
$PYTEST .gts-spec/tests/test_op12_schema_vs_schema_validation.py::TestCaseOp12_FinalBase_RejectDerived --gts-base-url http://127.0.0.1:8001
```

`GTS_BASE_URL` env var works too. The server holds state in memory with no reset endpoint — restart between full-suite runs.

## Working in This Repo

- `.gts-spec` is a submodule. Bump with `make update-spec`, then rerun `make e2e` to pick up new spec tests.
- Handlers in `gts-cli/src/server.rs` stay thin — logic goes in `gts/` where it is unit-testable. New REST behavior usually already has coverage in `.gts-spec/tests/`; run the relevant file before and after to confirm.
- `make check` is the full local gate: fmt + clippy + test + e2e.
