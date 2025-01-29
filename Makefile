.PHONY: build
build:
	@cargo build-sbf --manifest-path programs/limitless/Cargo.toml --features localnet

.PHONY: build-prod
build-prod:
	@cargo build-sbf --manifest-path programs/limitless/Cargo.toml

.PHONY: tests
tests:
	@cd cli && cargo build && cd -
	@cd programs/limitless && cargo test --features localnet && cd -
