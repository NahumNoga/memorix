install:
	cargo build
codegen:
	(cd ../../../cli && cargo run -- fmt ../clients/redis/rust/tests/example-schema.memorix && cargo run -- codegen ../clients/redis/rust/tests/example-schema.memorix)
format:
	cargo fmt
format-check:
	cargo fmt --check
lint:
	cargo clippy -- -D warnings
lint-fix:
	cargo clippy --fix
typecheck:
	cargo check
test: FORCE
	cargo test
start: FORCE
	cargo run --example example

FORCE: ;
