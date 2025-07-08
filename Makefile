.DEFAULT_GOAL := help

.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

# -- variables --------------------------------------------------------------------------------------

WARNINGS=RUSTDOCFLAGS="-D warnings"
ALL_FEATURES_BUT_ASYNC=--features concurrent,testing
# Enable file generation in the `src` directory.
# This is used in the build scripts of miden-lib.
BUILD_GENERATED_FILES_IN_SRC=BUILD_GENERATED_FILES_IN_SRC=1
# Enable backtraces for tests where we return an anyhow::Result. If enabled, anyhow::Error will
# then contain a `Backtrace` and print it when a test returns an error.
BACKTRACE=RUST_BACKTRACE=1

# -- linting --------------------------------------------------------------------------------------

.PHONY: clippy
clippy: ## Runs Clippy with configs
	cargo clippy --workspace --all-targets $(ALL_FEATURES_BUT_ASYNC) -- -D warnings


.PHONY: clippy-no-std
clippy-no-std: ## Runs Clippy with configs
	cargo clippy --no-default-features --target wasm32-unknown-unknown --workspace --lib --exclude bench-prover -- -D warnings


.PHONY: fix
fix: ## Runs Fix with configs
	cargo fix --workspace --allow-staged --allow-dirty --all-targets $(ALL_FEATURES_BUT_ASYNC)


.PHONY: format
format: ## Runs Format using nightly toolchain
	cargo +nightly fmt --all


.PHONY: format-check
format-check: ## Runs Format using nightly toolchain but only in check mode
	cargo +nightly fmt --all --check


.PHONY: lint
lint: ## Runs all linting tasks at once (Clippy, fixing, formatting)
	@$(BUILD_GENERATED_FILES_IN_SRC) $(MAKE) format
	@$(BUILD_GENERATED_FILES_IN_SRC) $(MAKE) fix
	@$(BUILD_GENERATED_FILES_IN_SRC) $(MAKE) clippy
	@$(BUILD_GENERATED_FILES_IN_SRC) $(MAKE) clippy-no-std

# --- docs ----------------------------------------------------------------------------------------

.PHONY: doc
doc: ## Generates & checks documentation
	$(WARNINGS) cargo doc $(ALL_FEATURES_BUT_ASYNC) --keep-going --release


.PHONY: book
book: ## Builds the book & serves documentation site
	mdbook serve --open docs

# --- testing -------------------------------------------------------------------------------------

.PHONY: test-build
test-build: ## Build the test binary
	$(BUILD_GENERATED_FILES_IN_SRC) cargo nextest run --cargo-profile test-dev --features concurrent,testing,std --no-run


.PHONY: test
test: ## Run all tests. Running `make test name=test_name` will only run the test `test_name`.
	$(BUILD_GENERATED_FILES_IN_SRC) $(BACKTRACE) cargo nextest run --profile default --cargo-profile test-dev --features concurrent,testing,std $(name)


# This uses the std feature to be able to load the MASM source files back into the assembler
# source manager (see `source_manager_ext::load_masm_source_files`).
.PHONY: test-dev
test-dev: ## Run default tests excluding slow prove tests in debug mode intended to be run locally
	$(BUILD_GENERATED_FILES_IN_SRC) $(BACKTRACE) cargo nextest run --profile default --cargo-profile test-dev --features concurrent,testing,std --filter-expr "not test(prove)"


.PHONY: test-docs
test-docs: ## Run documentation tests
	$(WARNINGS) cargo test --doc $(ALL_FEATURES_BUT_ASYNC)


# --- checking ------------------------------------------------------------------------------------

.PHONY: check
check: ## Check all targets and features for errors without code generation
	$(BUILD_GENERATED_FILES_IN_SRC) cargo check --all-targets $(ALL_FEATURES_BUT_ASYNC)


.PHONY: check-no-std
check-no-std: ## Check the no-std target without any features for errors without code generation
	$(BUILD_GENERATED_FILES_IN_SRC) cargo check --no-default-features --target wasm32-unknown-unknown --workspace --lib

# --- building ------------------------------------------------------------------------------------

.PHONY: build
build: ## By default we should build in release mode
	$(BUILD_GENERATED_FILES_IN_SRC) cargo build --release


.PHONY: build-no-std
build-no-std: ## Build without the standard library
	$(BUILD_GENERATED_FILES_IN_SRC) cargo build --no-default-features --target wasm32-unknown-unknown --workspace --lib --exclude bench-prover


.PHONY: build-no-std-testing
build-no-std-testing: ## Build without the standard library. Includes the `testing` feature
	$(BUILD_GENERATED_FILES_IN_SRC) cargo build --no-default-features --target wasm32-unknown-unknown --workspace --exclude miden-bench-tx --features testing --exclude bench-prover


.PHONY: build-async
build-async: ## Build with the `async` feature enabled (only libraries)
	$(BUILD_GENERATED_FILES_IN_SRC) cargo build --lib --release --features async --workspace --exclude bench-prover

# --- benchmarking --------------------------------------------------------------------------------

.PHONY: bench-tx
bench-tx: ## Run transaction benchmarks
	cargo run --bin bench-tx

.PHONY: bench-prover
bench-prover: ## Run prover benchmarks and consolidate results.
	cargo bench --bin bench-prover --bench benches
	cargo run --bin bench-prover
