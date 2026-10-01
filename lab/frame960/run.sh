#!/bin/sh
# TASK-63 lab pipeline (offline oracles; never invoked by cargo test).
# Usage: lab/frame960/run.sh   (from anywhere)
set -e
ROOT=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
cd "$ROOT"

echo "== generate vectors"
python3 gen960.py vectors >/dev/null
sha256sum vectors/*.loas vectors/*.m4a vectors/*.au vectors/*.asc

echo "== ffmpeg 7.0.2 static CLI (lavc): LOAS + M4A"
for v in lc960-m48 lc960-s48; do
  ffmpeg -v error -f loas -i "vectors/$v.loas" -f f32le -acodec pcm_f32le "vectors/$v.loas.pcm" -y
done
ffmpeg -v error -i vectors/lc960-s48.m4a -f f32le -acodec pcm_f32le vectors/lc960-s48.m4a.pcm -y
python3 - <<'EOF'
import struct
for f in ['vectors/lc960-m48.loas.pcm', 'vectors/lc960-s48.loas.pcm',
          'vectors/lc960-s48.m4a.pcm']:
    d = open(f, 'rb').read()
    v = struct.unpack(f'<{len(d)//4}f', d)
    print(f, 'samples=%d peak=%.4f' % (len(v), max(abs(x) for x in v)))
EOF
ffprobe -v error -show_streams -of json vectors/lc960-s48.m4a | grep -E 'codec_name|profile|sample_rate|channels|duration_ts'

echo "== avc_driver (FFmpeg 9.0.1 native AAC): M4A"
"$(CDPATH= cd -- "$ROOT/.." && pwd)/libavcodec/avc_driver" decode-container vectors/lc960-s48.m4a

echo "== faad2 / FDK runtimes via ctypes (raw AU + ASC)"
for v in lc960-m48 lc960-s48; do
  python3 -u decode_ctypes.py faad2 "vectors/$v.au" "vectors/$v.asc" | tail -2
  python3 -u decode_ctypes.py fdk "vectors/$v.au" "vectors/$v.asc" | tail -1
done

echo "== syom current behavior (lab-only probe crate)"
(cd syom_probe && cargo run --quiet --release -- ../vectors)
