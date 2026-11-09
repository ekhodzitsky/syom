# Using syom

[Documentation](README.md) · [Examples](../README.md#examples) ·
[Verification and release status](verification.md)

syom returns planar `f32` at the native sample rate. The API guide is
pinned to [0.10.3](https://docs.rs/syom/0.10.3/syom/guide/); the
[checkout guide](../src/guide.rs) describes this source tree.
[Release and checkout](verification.md#release-and-checkout) explains the gapless
boundary. The API names below are available in 0.10.3.

## Choose an API

| Task | Entry point | Important boundary |
|---|---|---|
| Decode bytes, preserving coded channels | `decode_with(bytes, &DecodeOptions::audio())` | Planar `f32`; no resampling |
| Read a file | `read_with(path, options)` | M4A uses seekable input; other supported transports use the byte path |
| Mix speech to mono | `decode(bytes)` / `read(path)` | Bounded speech defaults; non-LFE mean |
| Decode incoming chunks | `Decoder::feed` then `finish` | ADTS, LATM/LOAS, fragmented MP4; callbacks borrow PCM |
| Stream flat M4A from a reader | `decode_seek_streaming` | Requires `Read + Seek`; loads metadata and one compressed AU |
| Seek in native presentation samples | `M4aSeek` | Exact history can require prefix replay |
| Inspect metadata without PCM | `probe` | Duration/trim certainty is explicit; metadata is not source authentication |
| Encode a complete buffer | `encode` / `encode_with` | LC by default; container/profile options are explicit |
| Encode incrementally | `Encoder::feed` then `finish` | Callback AUs borrow encoder storage; raw output needs caller metadata |
| Use your own transport | `Decoder::from_asc` / `decode_au` | Caller supplies ASC and AU boundaries |
| Persist qualified native LC state | `M4aSeek::checkpoint` / `restore_checkpoint` | Source/options/build authentication belongs to the caller |

Every push feed must be finished. Copy borrowed frames if they must outlive a
callback. A callback error fails a push instance; `reset` reuses its storage.
See [compiling API examples](https://docs.rs/syom/0.10.3/syom/guide/) for
`Read`/`Write`, raw AUs, encoding options, memory budgets and typed errors.

## Formats and channel layout

| Operation | ADTS | M4A | LATM/LOAS | fMP4 |
|---|---|---|---|---|
| AAC-LC decode | Yes | Yes | Yes | Yes |
| HE-AAC v1/v2 decode | Yes | Yes | Yes | Yes |
| AAC-LD decode (512/480) | No | Yes | Yes | Same ASC path; no separate LD fMP4 golden |
| LC encode | Yes | Yes | Yes | No |
| HE v1/v2 encode | Yes | Yes | Yes | No |
| AAC-LD encode (512) | No | Yes | Yes | No |

LC encode defaults to 128 kbps ABR and per-band M/S. `with_he`, `with_he_v2`
and `with_ld` are opt-in; ADTS cannot signal LD. Bounded fragmented MP4
supports one unencrypted AAC track. Push decode needs its init segment before
fragments; flat M4A remains random-access. See the [format contract](../src/guide.rs).

Surround LC uses 3, 4, 5, 6 or 8 planes (`channel_configuration` 3–7).
Seven planes have no supported layout. `DecodeOptions::audio()` keeps
coded channels; `decode()` uses the speech mono mix. The speech duration
budget is two hours at 48 kHz; higher rates consume the same sample budget
sooner. There is no resampling. Supported HE output can run at twice the
core rate; reported PCM uses the output rate.

## Install from a checkout

The registry dependency is `syom = "0.10"`. To evaluate local changes
with a sibling checkout:

```toml
[dependencies]
syom = { version = "0.10", path = "../syom" }
```

The path selects local sources; the matching version does not turn those
sources into the published archive. [Installation](../README.md#install) and
[release verification](verification.md#release-and-checkout) distinguish them.

## Exact interval acquisition since 0.9.0

`exact_seek_plan` and budgeted `seek_exact` cover LC/LD mono/stereo intervals.
Every nonempty exact plan needs the coded prefix from AU zero. Arbitrary cold
sparse seeking is not qualified. The older `seek` API remains available.

`save_exact_position` is source-instance-bound and restores by replay, not by
importing DSP state. Persistent checkpoints are a different contract. Follow
[checkpoint eligibility, binding and completion](persistent-checkpoints.md)
and the [source guide](../src/guide.rs) for native coordinates, borrowed byte
ranges and resource limits.

## Gapless timing

One-shot ADTS encode writes a delay tag; LATM/LOAS has no equivalent tag and
retains whole-frame padding. M4A edit lists take precedence over tags.
Version 0.10.3 rounds fractional movie-clock durations down and caps small
overhangs at the available media, consistently across probe and playback.
The 0.10.2 standard Apple tag reader validates counts against the AAC
packet grid, while tag-only persistent checkpoints stay excluded.
Version 0.10.1 has the [known iTunSMPB limitation](verification.md#release-and-checkout).

Push ADTS applies modest tag edges before final length confirmation under its
bounded-memory contract; finish reports trim only when counts match. Push
fragmented MP4 does not consume iTunSMPB. A complete bounded one-shot or
seekable MP4 path has a different input scope. See the [source guide](../src/guide.rs).

## Validation and integration

Validation moved to [release status and evidence](verification.md): registry
receipts, current hosted/native checks, consumer qualifications and remaining
measurement gaps. Historical reports retain their original source identities.

## Badges and measurement gaps

See [badge sources and dated coverage](verification.md#badges-and-measurement-gaps).
These anchors remain for links from earlier reports; verification has one
maintained home rather than duplicated status paragraphs.
