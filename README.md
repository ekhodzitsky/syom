# syom

**съём.** SYOM: Speech Yielded from Original Media.

On-device audio extract. One Rust CLI. No cloud. No ffmpeg.

MP4 / M4A (AAC `mp4a` or PCM `sowt`/`twos`/`ipcm`/`lpcm`/`raw `) / ADTS AAC / PCM WAV in. **16 kHz s16le mono** out.

```sh
syom lecture.mp4 -o lecture.wav
syom lecture.mp4 -o lecture.pcm
syom clip.aac -o clip.wav
syom take.wav -o take16k.wav
```

`-o` defaults to `out.wav`. `.pcm` / `.raw` / `.s16` write headerless s16le.
`-o -` writes WAV to stdout.

kover and sluh spawn this binary. They do not link the decoder.

## Not in v1

Encoding, remux, video, fragmented MP4, DRM, Opus, MP3, FLAC, HTTP.

## License

MIT. AAC-LC engine is original (ISO/IEC 14496-3 / 13818-7). See
[NOTICE](NOTICE) for the AAC patent disclaimer (Via LA). Code is free;
patent questions are the shipper's.
