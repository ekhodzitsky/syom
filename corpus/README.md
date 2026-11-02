# AAC evaluation corpus

Offline manifest for syom quality/performance work. Ordinary `cargo test`
never downloads files and never needs these natural recordings on disk.

## Layers

| Layer | Where | Bytes in git? |
|---|---|---|
| In-tree goldens | `src/goldens`, `src/engine/goldens` | yes, SHA-256 in the manifest |
| Generated PCM | `scripts/corpus_synth.py` | no; regenerated and hashed |
| Natural excerpts | origin URLs in the manifest | **no** (not vendored) |

Registered natural rows are **identified** (origin, license, recording
identity, split) but retain `presence=gap-until-obtained`. The acquisition
limitation belongs to this manifest's recorded inventory, not to every later
host or evaluation. Separate [consumer-recording qualification](../lab/release-audit/CANDIDATE-0.8.1.md)
does not establish availability of the registered holdouts or change their
split/presence. See [coverage](coverage.md) for that distinction.
Do not commit `.wav`/`.flac`/`.pcm` of those sources.

## Holdout

Natural `recording_id` values tagged `holdout` must not appear in `dev`.
The verifier fails if the same recording identity is in both splits.
At least one-third of natural recording identities are holdout.

## Verify

```sh
python3 scripts/build_corpus_manifest.py   # regenerate hashes after adding goldens
python3 scripts/verify_corpus.py           # offline; no network
```

A flipped golden or overlapping train/holdout recording fails the verifier.

To require the registered listening holdouts on this machine:

```sh
python3 scripts/verify_corpus.py --require-holdouts
python3 -m unittest discover -s scripts -p test_verify_corpus.py
```

This opt-in mode requires exactly eight natural holdout rows with distinct
recording identities, a SHA-256 and a readable matching cache file for each.
`cache_path` may be absolute or relative to the repository root. Keep recordings
outside Git. Both modes print registered and hash-verified file counts;
the default inventory check still allows missing optional natural assets.

Hash verification establishes byte availability and integrity, not audio format,
duration, source/license provenance, or readiness of a listening study. Those
require acquisition records and the listening protocol.

## Local acquisition receipts

See [source acquisition](ACQUISITION.md) for the pinned selection plan, local
commands, attribution records and remaining listening-study limits.

Keep local availability in a detached receipt, for example
`target/corpus-cache/acquisitions.json`, rather than editing the generated
inventory. The receipt has integer `version: 1` and a `records` list. Each
record requires the registered `id` and `recording_id`, a 64-digit hexadecimal
`sha256`, and a `cache_path` (absolute or repository-root-relative). Additional
source-file, transformation and attribution metadata may be kept in that
receipt; it is not automatically validated as provenance.

```sh
python3 scripts/verify_corpus.py --acquisitions target/corpus-cache/acquisitions.json
python3 scripts/verify_corpus.py --acquisitions target/corpus-cache/acquisitions.json --require-holdouts
python3 -m unittest discover -s scripts -p test_corpus_acquisition.py
```

`--acquisitions` is repeatable for separate acquisition batches; duplicate IDs
across receipts are rejected. An explicit receipt requires every listed file to
exist and match its hash,
even without the eight-holdout option. A partial receipt can verify three
available files while the strict eight-holdout check still fails. The overlay
only updates `sha256`, `cache_path` and `presence=acquired-local` in memory;
it cannot change split, source identity, registered rate/layout or license.
Unknown or duplicate IDs and recording-identity mismatches are errors.

The base manifest and receipt are never modified by verification. Regenerating
`corpus/manifest.json` leaves detached receipts intact. Default verification
without `--acquisitions` continues to report the portable registered inventory,
not another machine's local cache. Keep receipt paths and audio under ignored
`target/` or outside the repository; commit only reviewed textual evidence.

## Syntax inventory (not ISO 14496-26)

`corpus/conformance/` records clause acquisition status, a coverage matrix
for advertised tools, and independently authored ASC/ADTS/LATM/PCE
headers. It is **not** a conformance certificate. Full ISO 14496-3 / 26
text was not obtained.

```sh
python3 scripts/verify_conformance_inventory.py
```

## Oracle provenance

`corpus/oracles/provenance.json` records committed lavc/naive goldens:
hashes, comparison policy (deterministic vs PNS/stochastic), commands when
known, and **explicit gaps** where ffmpeg versions were never written down.
Ordinary tests validate that file without spawning ffmpeg.

```sh
python3 scripts/build_oracle_provenance.py
python3 scripts/verify_oracle_provenance.py
```
