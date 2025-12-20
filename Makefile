CI := 1

.PHONY: help build dev-fmt all check fmt clippy test deny security update-spec e2e

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

# Run all checks and build
all: check build

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
	git submodule update --remote .gts-spec

# Run end-to-end tests against gts-spec
e2e: build
	@echo "Starting server in background..."
	@./target/release/gts server --port 8000 & echo $$! > .server.pid
	@sleep 2
	@echo "Running e2e tests..."
	@export PYTHONDONTWRITEBYTECODE=1
	@pytest --log-file=e2e.log ./.gts-spec/tests || (kill `cat .server.pid` 2>/dev/null; rm -f .server.pid; exit 1)
	@echo "Stopping server..."
	@kill `cat .server.pid` 2>/dev/null || true
	@rm -f .server.pid
	@echo "E2E tests completed successfully"

# Run all quality checks
check: fmt clippy test e2e
