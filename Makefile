.PHONY: help build release test lint fmt check install clean fuzz

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*##' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

build: ## Build all crates (debug)
	cargo build

release: ## Build all crates (release)
	cargo build --release

test: ## Run all tests
	cargo test

lint: ## Run clippy (zero warnings)
	cargo clippy --all-targets --all-features -- -D warnings

fmt: ## Format code
	cargo fmt

check: fmt lint test ## Format, lint, and test (pre-commit)

install: ## Install client and server binaries
	cargo install --path crates/rmux-client
	cargo install --path crates/rmux-server

clean: ## Remove build artifacts
	cargo clean

fuzz: ## Run all fuzz targets briefly (requires nightly)
	@for target in $$(cd fuzz && cargo +nightly fuzz list 2>/dev/null); do \
		echo "Fuzzing $$target..."; \
		cd fuzz && cargo +nightly fuzz run $$target -- -max_total_time=10 && cd ..; \
	done
