# Isolated glint-audio 0.11.0 AAC smoke. Not invoked by cargo test.

.PHONY: all smoke
all: smoke

# C++17 via zig-c++ --target=x86_64-linux-gnu (host has no g++ in PATH).
smoke:
	@echo "Not invoked by cargo test. C++17 compiler required (lab/glint/bin/cxx.sh):"
	@echo "  sh lab/glint/build.sh"
	@echo "  ./lab/glint/target/release/glint_smoke decode src/goldens/sine48.adts"
	@echo "  ./lab/glint/target/release/glint_smoke encode-sine 48000 1 128 1 /tmp/out.adts"
	@echo "  ./lab/glint/target/release/glint_smoke overhead"
	@echo "  ./lab/glint/target/release/glint_smoke smoke src/goldens/sine48.adts"
