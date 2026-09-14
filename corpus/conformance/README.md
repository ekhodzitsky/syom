# AAC syntax inventory (TASK-17)

Research artifact, not a conformance certificate. Ordinary `cargo test`
never downloads ISO text, never needs ISO 14496-26 vectors, and never
spawns ffmpeg/FDK.

## What this is

A clause-to-feature map for the **advertised** LC / HE v1 / HE v2 decode
path (ASC, ADTS, LATM/LOAS, in-band PCE, M4A) plus LC encode limits.
Independently authored header/config examples live in `vectors.json`.
Existing in-tree goldens are **not** treated as ISO evidence.

## What this is not

- Not ISO/IEC 14496-26 pass/fail.
- Not a license to copy standard text or competitor codec bodies.
- Not a claim that syom matches every independent layout today.

## Acquisition

| Document | Status |
|---|---|
| ISO/IEC 14496-3 (MPEG-4 Audio) full text | **not obtained** (paid) |
| ISO/IEC 14496-3:2005 Amd 2 / Amd 9 public previews | field names and bit widths cited in `clauses.json` |
| ISO/IEC 13818-7 (MPEG-2 AAC / ADTS) full text | **not obtained** |
| ISO/IEC 11172-3 CRC-16 polynomial | widely republished; used for ADTS CRC identity |
| ISO/IEC 14496-26:2024 conformance bitstreams | **not obtained** (paid; [ISO catalogue](https://committee.iso.org/standard/86376.html?browse=ics)) |
| ISO/IEC 14496-12 ISOBMFF | not downloaded this session; M4A subset is in-tree `isomp4.rs` |
| Independent parsers | FFmpeg `mpeg4audio.c` / `aacdec_latm.h` n7; FAAD2 `syntax.c`; FDK v2.0.3 pin (lab) |

Lawful next step for official vectors: buy 14496-26 with redistribution
rights, keep bytes **out** of git, and point TASK-103 at an offline cache.
Until then, use these authored headers plus committed goldens.

## Verify

```sh
python3 scripts/verify_conformance_inventory.py
```

## Follow-on

`go-nogo.md` says which S1 parser tasks can start without purchasing ISO
text, and which cells stay unqualified.
