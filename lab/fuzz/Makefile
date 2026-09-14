# Isolated parser / stateful stream fuzz. Not invoked by cargo test.

.PHONY: run run-state
run:
	cargo run --release --manifest-path Cargo.toml --bin syom_fuzz -- --seconds 30
run-state:
	cargo run --release --manifest-path Cargo.toml --bin syom_fuzz_state -- --seconds 30
