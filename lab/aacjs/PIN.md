# AAC.js / Aurora.js pin (TASK-20)

Pinned peer: **AAC.js 0.1.3** — JavaScript AAC-LC decoder for Aurora.js.
Doc-2 ledger row: revision `2d9bd01b14fb70022340560b66b577624584b878`.

## Sources

| Component | Pin | Integrity | Verified against |
|---|---|---|---|
| `aac` npm | 0.1.3 (published 2016-06-17, latest) | sha512 `MvkT0k+co6o+zLMrBFFeVhYcG/S/jzy+2p00c/VwA71q6g90J28qUNh93NabKrcN06bkwFK0OeiEpFsQd7TS7g==` | all 12 `src/*.js` byte-identical to git `2d9bd01b14fb70022340560b66b577624584b878` (fetched 2026-09-22) |
| `av` npm (Aurora.js) | 0.4.9 (published 2016-08-13, latest) | sha512 `mhkqrzCOMndmoQovW43ry8tkMttuvKcpU0u9/K7zI+lF5NtCY2NE+HwAPiEgd3rHK7IuBT1ZsGL+LKeCPW9btQ==` | `src/*.coffee` byte-identical to git tag `v0.4.9` = `e87ddfb882d85297967d698a347b3e83e653f763`; npm `.js` files are the compiled counterparts |

Licenses: aac **LGPL-3.0**, av MIT. Both packages are unmaintained since 2016
(npm deprecation warnings on install: `coffee-script@1.7.1`, `mkdirp@0.3.5`).

## Runtimes

- Node v22.19.0 (V8), npm 10.9.3 — primary pinned JS runtime.
- Firefox 155.0 headless via geckodriver (`/snap/bin/geckodriver`), W3C
  WebDriver REST — browser lane (`browser/driver.cjs`).
- ffmpeg 7.0.2-static — offline oracle for reference PCM and probe-stream
  generation only; never on any product test path.

## Install / build (offline-reproducible commands)

```sh
cd lab/aacjs
npm install --ignore-scripts --no-audit --no-fund   # lockfile committed
# --ignore-scripts: av depends on native `speaker` (playback only); it is
# require()d inside try/catch (av/src/devices/node-speaker.js), so decoding
# works without building it. Building it needs ALSA headers and is unrelated
# to this evaluation.

# Browser bundles (build lane only):
npm install -D browserify@4.1.10 browserify-shim@3.5.0 exorcist@0.1.6 \
  --ignore-scripts --no-audit --no-fund
```

Upstream's own browser build does not run on modern Node: `browserify@4` +
`coffeeify@0.6` fails to parse av's `browser.coffee`/`sources/browser/*.coffee`
("ParseError: reserved word var" / "Unexpected string"). The lab works around
it with metadata-only adjustments (no decoder source is modified):

- `build/av_browser_entry.js` — JS equivalent of av's `browser.coffee` entry
  using the compiled `src/*.js` the same npm tarball ships.
- `node_modules/av/package.json` browser field repointed from
  `browser/*.coffee` to the compiled `browser/*.js` (backup:
  `/tmp/av_pkg.orig` during the session; re-apply after any reinstall).
- `lab/aacjs/package.json` root `browser` field shims av's node file/http
  sources to the compiled browser variants.
- `browserify --standalone AV build/av_browser_entry.js > build/aurora.js`
- `browserify node_modules/aac/ > build/aac.js` (browserify-shim maps
  `require('av')` → `window.AV`, verified in the bundle).

Bundle sizes: aurora.js 127 357 B (gzip-9 22 008), aac.js 153 765 B
(gzip-9 41 349); together 281 122 B raw / 63 357 B gzip.
