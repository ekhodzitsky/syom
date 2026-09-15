# Isolated parser / stateful stream fuzz. Not invoked by cargo test.

.PHONY: run run-state campaign
run:
	cargo run --release --manifest-path Cargo.toml --bin syom_fuzz -- --seconds 30
run-state:
	cargo run --release --manifest-path Cargo.toml --bin syom_fuzz_state -- --seconds 30
# 24 CPU-h example: 16 parser + 8 stateful workers × 3600 s (unique --seed).
campaign:
	cargo build --release --bin syom_fuzz --bin syom_fuzz_state
	@echo "launch workers with unique --seed; see lab/baseline/CAMPAIGN.md"
