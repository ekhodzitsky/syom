#!/bin/sh
# TASK-64 — fragmented MP4 fixture generation.
# Pinned tool: ffmpeg/ffprobe 7.0.2 static (see PIN.md). Offline oracle only;
# nothing here is linked or spawned by product tests.
set -eu
cd "$(dirname "$0")"
FIX=fixtures
mkdir -p "$FIX" dash

# 6 s speech-like synthetic stereo, 48 kHz (deterministic lavfi source).
SRC='aevalsrc=sin(2*PI*440*t)*0.3+sin(2*PI*1760*t)*0.1*(0.5+0.5*sin(2*PI*3*t)):sample_rate=48000:duration=6:channel_layout=stereo'
run() {
  ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$SRC" "$@"
}

# --- LC, init+media in one file (CMAF-style / fMP4) --------------------------
# default: tfhd carries base-data-offset-present; tfdt present.
run -c:a aac -b:a 96k \
    -movflags empty_moov -frag_duration 1000000 "$FIX/lc_empty_moov.mp4"

# default_base_moof: tfhd flag 0x020000, trun data_offset relative to moof.
run -c:a aac -b:a 96k \
    -movflags empty_moov+default_base_moof -frag_duration 1000000 "$FIX/lc_dbmoof.mp4"

# omit_tfhd_offset: no base-data-offset in tfhd, trun data_offset relative to
# the end of the previous fragment's data (implicit base).
run -c:a aac -b:a 96k \
    -movflags empty_moov+omit_tfhd_offset -frag_duration 1000000 "$FIX/lc_omit_off.mp4"

# global_sidx: one sidx index box at the front of the file.
run -c:a aac -b:a 96k \
    -movflags empty_moov+global_sidx -frag_duration 1000000 "$FIX/lc_sidx.mp4"

# negative_cts_offsets: signed trun composition offsets instead of edit list.
run -c:a aac -b:a 96k \
    -movflags empty_moov+negative_cts_offsets -frag_duration 1000000 \
    "$FIX/lc_negcts.mp4"

# CMAF profile flag set (single file, cmfc brand).
run -c:a aac -b:a 96k \
    -movflags cmaf -frag_duration 1000000 "$FIX/lc_cmaf.mp4"

# --- LC, separate init + media segments (DASH muxer, styp in segments) -----
run -c:a aac -b:a 96k -f dash -seg_duration 1 -use_timeline 0 \
    -init_seg_name init.m4s -media_seg_name 'seg_$Number%02d$.m4s' \
    dash/lc.mpd

# --- Encrypted (CENC cenc-aes-ctr): expected unsupported --------------------
run -c:a aac -b:a 96k \
    -movflags empty_moov -frag_duration 1000000 \
    -encryption_scheme cenc-aes-ctr \
    -encryption_key 00112233445566778899aabbccddeeff \
    -encryption_kid 0123456789abcdeffedcba9876543210 "$FIX/lc_cenc.mp4"

# --- Two AAC audio tracks, fragmented: expected unsupported -----------------
run -map 0:a -map 0:a -c:a aac -b:a 96k \
    -movflags empty_moov -frag_duration 1000000 "$FIX/lc_2track.mp4"

# --- HE v1 / HE v2 payloads (syom-encoded M4A goldens, explicit two-rate
# --- AOT 5/29 ASC in esds) remuxed to fMP4 by the ffmpeg mov muxer.
# NOTE: ADTS -c:a copy remux is invalid for this (aac_adtstoasc drops the
# DecoderSpecificInfo for implicit HE; lavc then fails to decode) — the
# explicit-ASC M4A goldens are the correct source.
ffmpeg -hide_banner -loglevel error -y -i ../../src/goldens/he48em.m4a \
    -c:a copy -movflags empty_moov -frag_duration 1000000 "$FIX/he_empty_moov.mp4"
ffmpeg -hide_banner -loglevel error -y -i ../../src/goldens/he2_48em.m4a \
    -c:a copy -movflags empty_moov -frag_duration 1000000 "$FIX/he2_empty_moov.mp4"

# --- Flat (non-fragmented) LC control for PCM reference ---------------------
run -c:a aac -b:a 96k "$FIX/lc_flat.m4a"

# Reference PCM (ffmpeg decode, oracle lane).
ffmpeg -hide_banner -loglevel error -y -i "$FIX/lc_empty_moov.mp4" \
    -f s16le -acodec pcm_s16le "$FIX/lc_empty_moov.lavf.s16"
ffmpeg -hide_banner -loglevel error -y -i "$FIX/lc_flat.m4a" \
    -f s16le -acodec pcm_s16le "$FIX/lc_flat.lavf.s16"

echo "fixtures written to $FIX/ and dash/"
