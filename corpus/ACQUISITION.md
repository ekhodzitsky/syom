# Registered holdout source acquisition

The [version 1 plan](acquisition-plan-v1.json) fixes source selection before
opening codec outputs. These are acquisition amendments to formerly underspecified
recording groups, not a claim that the September registration named exact files.
The original recording IDs and dev/holdout split remain unchanged.

LibriSpeech selection was committed in `9929f9e` before archive retrieval;
LibriVox/Musopen in `01693e7` and the FMA/author track in `65776e9` before
those downloads. Selection uses file numbers or fixed first 12 seconds, with
no listening, silence skipping, content scoring or codec-output selection.

## Acquired on 2026-11-02

Six of eight registered slots have hash-verified local source excerpts. The
[run bundle](runs/acquisition-20261102/report.json) retains sanitized text
receipts, source/member/PCM hashes, probes and actual verifier output.

| Registered slot | Actual native PCM | Samples per channel |
|---|---|---:|
| LibriSpeech 1580 | 16,000 Hz mono f32le | 192,000 |
| LibriSpeech 1995 | 16,000 Hz mono f32le | 192,000 |
| LibriSpeech 2094 | 16,000 Hz mono f32le | 192,000 |
| LibriVox War of the Worlds | 44,100 Hz mono f32le | 529,200 |
| Musopen Chopin Op. 28 No. 1 | 48,000 Hz stereo f32le | 576,000 |
| FMA / author Congo Bongo Stomp | 44,100 Hz stereo f32le | 529,200 |

All six reproduced the same PCM hashes offline. Regenerating the inventory
preserved its hash and both detached receipt hashes. Strict readiness still
fails for exactly `nat-cv-ar` and `nat-cv-ja`; neither was replaced by a more
convenient source. These results apply to Linux x86_64, Python 3.14.4 and
ffmpeg 7.0.2-static; other platforms were not tested.

## Local procedure

Audio, source archives, attribution files and detached receipts stay under
ignored `target/corpus-cache/`. The generated manifest remains the portable
registration inventory, so another checkout without this cache still reports
zero acquired files. Do not copy recordings into the repository's tracked files.

Download `test-clean.tar.gz` from a pinned official mirror in the plan and
check the published MD5 `32fa31d27d2e1cad72775fee3f4849a9`. The acquisition
script independently verifies it again; it does not download the archive.

```sh
mkdir -p target/corpus-cache/openslr12
curl --fail --location --output target/corpus-cache/openslr12/test-clean.tar.gz \
  https://openslr.elda.org/resources/12/test-clean.tar.gz
python3 scripts/acquire_librispeech_holdouts.py
python3 scripts/acquire_catalog_holdouts.py
python3 scripts/acquire_catalog_holdouts.py --offline
python3 scripts/verify_corpus.py \
  --acquisitions target/corpus-cache/librispeech-acquisitions.json \
  --acquisitions target/corpus-cache/catalog-acquisitions.json
python3 scripts/verify_corpus.py \
  --acquisitions target/corpus-cache/librispeech-acquisitions.json \
  --acquisitions target/corpus-cache/catalog-acquisitions.json --require-holdouts
```

The final strict command currently must fail: the Arabic/Japanese Common Voice
slots have no authorized source archives. Ordinary verification with both
receipts validates all acquired files and split identities. Running the manifest
generator does not read or overwrite either detached receipt.

Both acquisition scripts use ffmpeg/ffprobe offline after source retrieval,
retain native sample rate/channel count and output interleaved little-endian
float32 PCM. They check finite values and exact 12-second length. LibriSpeech
concatenates numbered utterances with no artificial gaps and trims at 192,000
samples. Catalog files use their first 12 native seconds. There is no padding,
resampling, loudness normalization or codec encoding. These source excerpts are
not the finished listening stimuli.

Catalog source checksums and lengths are checked against the plan where
published. An independent persistent SHA-256 pin protects cached sources even
if a later failed run replaces the summary receipt. For the author-hosted FMA
master, the first download's SHA-256 is a local acquisition pin, not an
independently published upstream checksum. Source file hashes, decoded hashes,
probes, transformations and attribution are retained in the receipts.

## Limits before a listening study

- Common Voice release `cv-corpus-27.0-2026-09-11` Arabic/Japanese is pinned
  in the plan. Official access requires an API key and prior dataset terms
  acceptance. No account, credentials or terms acceptance were used here.
  Once authorized archives exist, the plan's metadata-only ordering rule must
  be followed and exact source member hashes recorded.
- LibriSpeech is native 16 kHz mono: it cannot provide independent source
  information above 8 kHz. Its reserved class labels do not prove phonetic
  coverage. Concatenation can cross utterance boundaries and is explicitly
  part of this acquisition amendment.
- The three catalog sources are MP3, not pristine lossless references. The
  author-distributed FMA title matches the selected catalog title; byte equality
  with FMA's hosted download is unverified. No transient/dense-music coverage
  claim follows from its title or tags.
- LibriVox's catalog declares public domain in the USA. This is recorded as
  source attribution, not universal legal clearance. Musopen and the author
  catalog supply the license references recorded in the plan/receipts.
- Native LibriVox is 44,100 Hz mono and Musopen is 48,000 Hz stereo, whereas
  the older inventory registers 22,050 and 44,100 Hz respectively. Availability
  overlays deliberately do not change those experimental fields. Rendering must
  explicitly reconcile actual source formats and the protocol before qualification.
- A current candidate freeze, actual-rate matching, independently decoded stimuli,
  loudness/fade handling, human participants and the protocol's Apple decision
  remain separate work. Source acquisition does not complete TASK-109 or establish
  audible quality, codec performance or SOTA eligibility.

## Local checks

All seven AGENTS.md checks passed: formatting, file size, changelog, clippy,
1,168 library tests (seven existing ignores), two integration tests, 29 doctests
and the package check. All 41 Python tests passed, including receipt overlays,
cross-batch duplicate rejection, deterministic sample selection, finite/length
checks and damaged-cache recovery. See [check results](runs/acquisition-20261102/checks.json).
Product codec sources and Cargo version remain unchanged at 0.9.0. The
Unreleased changelog records evaluation tooling only. No hosted CI, crate
publication or consumer deployment was performed.
