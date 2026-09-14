# AAC evaluation corpus

Offline manifest for syom quality/performance work. Ordinary `cargo test`
never downloads files and never needs these natural recordings on disk.

## Layers

| Layer | Where | Bytes in git? |
|---|---|---|
| In-tree goldens | `src/goldens`, `src/engine/goldens` | yes, SHA-256 in the manifest |
| Generated PCM | `scripts/corpus_synth.py` | no; regenerated and hashed |
| Natural excerpts | origin URLs in the manifest | **no** (not vendored) |

Natural rows are **identified** (origin, license, recording identity, split)
but `presence=gap-until-obtained`. This environment could not fetch them.
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
