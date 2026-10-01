# Isolated encoder quality baseline. Not invoked by cargo test.

.PHONY: run ath tonality hequal tune133 det133
run:
	cargo run --release --manifest-path Cargo.toml --bin syom_quality
ath:
	cargo run --release --manifest-path Cargo.toml --bin syom_ath
tonality:
	cargo run --release --manifest-path Cargo.toml --bin syom_tonality
hequal:
	cargo run --release --manifest-path Cargo.toml --bin syom_he_qualify
tune133:
	cargo run --release --manifest-path Cargo.toml --bin syom_tune133
det133:
	cargo run --release --manifest-path Cargo.toml --bin syom_det133
