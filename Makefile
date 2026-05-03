CI := 1

.PHONY: help build dev-fmt dev-clippy all check fmt clippy test deny security update-spec e2e-venv e2e generate-schemas

# Python interpreter used to bootstrap the venv; override with `make e2e PYTHON=python3.11`
PYTHON ?= python3
VENV_DIR := .gts-spec/.venv
VENV_PY  := $(VENV_DIR)/bin/python

# Port for the reference server during `make e2e`; override with `make e2e PORT=8001`
PORT ?= 8000

# Default target - show help
.DEFAULT_GOAL := help

# Show this help message
help:
	@awk '/^# / { desc=substr($$0, 3) } /^[a-zA-Z0-9_-]+:/ && desc { target=$$1; sub(/:$$/, "", target); printf "%-20s - %s\n", target, desc; desc="" }' Makefile | sort

# Build the workspace
build:
	cargo build --workspace
	cargo build --workspace --release

# Fix formatting issues
dev-fmt:
	cargo fmt --all

# Fix clippy issues
dev-clippy:
	cargo clippy --fix --workspace

# Generate schemas
generate-schemas: build
	./target/release/gts generate-from-rust --source .

# Run all checks and build
all: check deny build generate-schemas

# Check code formatting
fmt:
	cargo fmt --all -- --check

# Run clippy linter
clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run all tests
test:
	cargo test --workspace

# Check licenses and dependencies
deny:
	@command -v cargo-deny >/dev/null || (echo "Installing cargo-deny..." && cargo install cargo-deny)
	cargo deny check

# Run all security checks
security: deny

# Measure code coverage
coverage:
	@command -v cargo-llvm-cov >/dev/null || (echo "Installing cargo-llvm-cov..." && cargo install cargo-llvm-cov)
	cargo llvm-cov --workspace --lcov --output-path lcov.info
	cargo llvm-cov report

# Update gts-spec submodule to latest
update-spec:
	git submodule update --init --remote .gts-spec

# Create/refresh the Python venv used by the gts-spec e2e test suite
e2e-venv: $(VENV_PY)

$(VENV_PY):
	@if [ ! -x "$(VENV_PY)" ]; then \
		echo "Creating venv at $(VENV_DIR) using $(PYTHON)..."; \
		$(PYTHON) -m venv $(VENV_DIR); \
	fi
	@echo "Installing gts-spec e2e test dependencies..."
	@$(VENV_PY) -m pip install --quiet --upgrade pip setuptools wheel
	@$(VENV_PY) -m pip install --quiet -r .gts-spec/tests/requirements.txt

# Run end-to-end tests against gts-spec.
e2e: build e2e-venv
	@rm -rf logs e2e.log
	@set -e; \
	trap 'kill `cat .server.pid 2>/dev/null` 2>/dev/null || true; rm -f .server.pid' INT TERM EXIT; \
	echo "Starting server in background on port $(PORT)..."; \
	./target/release/gts server --port $(PORT) & echo $$! > .server.pid; \
	sleep 2; \
	echo "Running e2e tests..."; \
	PYTHONDONTWRITEBYTECODE=1 $(VENV_PY) -m pytest -p no:cacheprovider --log-file=e2e.log --gts-base-url http://127.0.0.1:$(PORT) ./.gts-spec/tests; \
	echo "E2E tests completed successfully"

# Run all quality checks
check: fmt clippy test e2e
