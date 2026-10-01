# TASK-64 fMP4 lab pins

## Generators / oracles (offline; product tests never spawn them)

- **ffmpeg / ffprobe 7.0.2 static** (johnvansickle.com build), Linux x86_64.
  `ffmpeg version 7.0.2-static https://johnvansickle.com/ffmpeg/`.
  Used as: mov muxer (`-movflags empty_moov|default_base_moof|omit_tfhd_offset|global_sidx|negative_cts_offsets|cmaf`),
  dash muxer (separate init + media segments), CENC muxer
  (`-encryption_scheme cenc-aes-ctr`), and demux oracle
  (`ffprobe -show_packets`, `ffmpeg -f s16le` decode).
  NOTE: pin is 7.0.2, not the lab-wide FFmpeg 9.0.1 codec pin — this lane
  needs the CLI muxer, which the 9.0.1 prefix build excludes
  (`--disable-programs`). The muxer is a structure generator, not a codec
  oracle; PCM comparisons still go through lavc decode.
- **syom HE goldens** `src/goldens/he48em.m4a` / `he2_48em.m4a` as HE v1/v2
  payloads; container remux by the ffmpeg mov muxer (`-c:a copy`), so the
  fMP4 structure is ffmpeg-generated, not syom-generated.
  (ADTS `-c:a copy` remux is invalid for implicit-HE: aac_adtstoasc drops
  the DecoderSpecificInfo and lavc then fails to decode — measured 2026-09-22;
  use the explicit-ASC M4A goldens.)

## Third-party vector (independent of both syom and ffmpeg)

Big Buck Bunny DASH (Akamai-hosted, historically GPAC-muxed):

- MPD: https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd
- `fixtures/thirdparty/bbb_init.m4a` = `bbb_a64k/bbb_a64k_0.m4a`,
  633 bytes, sha256 `4846d82d3e0045ba8d23b4338fa5ce2c592d70f84444df51a126567f9e80b3a0`
- `fixtures/thirdparty/bbb_seg1.m4a` = `bbb_a64k/bbb_a64k_1.m4a`,
  33632 bytes, sha256 `02f822bccf4add90234e09eea08a7b8629ae2c0f3d847d2ef995a1adce51b9ec`
- fetched 2026-09-22; HE-AAC v1 stereo 48 kHz output, AOT 5 ASC `2b118800`.

## Known-unavailable resources (recorded gaps, not silently skipped)

- GPAC `MP4Box` and Bento4 are not installed on this host: no second
  independent *muxer*. The ffmpeg mov muxer and ffmpeg dash muxer are two
  code paths of one project; the BBB vector covers a GPAC-muxed input.
- No JDK (jcodec/mp4parser lane impossible), no Apple hardware.
- No real-world Apple HLS fMP4 capture; the BBB DASH vector is the
  third-party cell.

## Reproduce

```sh
lab/fmp4/gen_fixtures.sh                      # regenerate fixtures/
cd lab/fmp4 && python3 crosscheck.py fixtures/lc_empty_moov.mp4 \
  fixtures/lc_dbmoof.mp4 fixtures/lc_omit_off.mp4 fixtures/lc_sidx.mp4 \
  fixtures/lc_negcts.mp4 fixtures/lc_cmaf.mp4 fixtures/he_empty_moov.mp4 \
  fixtures/he2_empty_moov.mp4                     # walker vs ffprobe
# (lc_cenc / lc_2track are unsupported-cell fixtures: ffprobe reports no
# decodable packets; they belong to the structure inventory only)
cargo build --release --manifest-path lab/fmp4/bridge/Cargo.toml
lab/fmp4/bridge/target/release/fmp4_bridge fixtures/lc_empty_moov.mp4 /tmp/out.s16

# third-party vector (git-ignored; re-fetch and verify):
curl -sfL https://dash.akamaized.net/akamai/bbb_30fps/bbb_a64k/bbb_a64k_0.m4a \
  -o lab/fmp4/fixtures/thirdparty/bbb_init.m4a
curl -sfL https://dash.akamaized.net/akamai/bbb_30fps/bbb_a64k/bbb_a64k_1.m4a \
  -o lab/fmp4/fixtures/thirdparty/bbb_seg1.m4a
sha256sum lab/fmp4/fixtures/thirdparty/*.m4a   # must match the pins above
```

`fixtures/`, `out/`, `dash/` and `bridge/target/` are git-ignored build
artifacts (`lab/fmp4/.gitignore`); the committed surface is this lab's
scripts, sources and reports.
