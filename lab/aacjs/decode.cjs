// TASK-20 lab harness: decode one ADTS/M4A file through Aurora.js + AAC.js.
// Usage: node decode.cjs <input> [out.f32le]
// Prints a JSON summary on stdout; raw interleaved float32 PCM to out.f32le.
'use strict';

const fs = require('fs');
const t0 = performance.now();
const AV = require('av');
const tRequireAv = performance.now();
require('aac');
const tRequireAac = performance.now();

const input = process.argv[2];
const outPath = process.argv[3];

const result = {
  input,
  requireAvMs: +(tRequireAv - t0).toFixed(3),
  requireAacMs: +(tRequireAac - tRequireAv).toFixed(3),
  format: null,
  durationMs: null,
  errors: [],
  chunks: 0,
  floats: 0,
  ended: false,
  decodeMs: null,
};

const asset = AV.Asset.fromFile(input);
asset.on('format', (f) => { result.format = f; });
asset.on('duration', (d) => { result.durationMs = d; });
asset.on('error', (e) => { result.errors.push(String(e && e.message || e)); });

const chunks = [];
let floats = 0;
asset.on('data', (chunk) => {
  result.chunks++;
  floats += chunk.length;
  chunks.push(Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength));
});
asset.once('end', () => {
  result.ended = true;
  result.floats = floats;
  result.decodeMs = +(performance.now() - tRequireAac).toFixed(3);
  if (outPath && chunks.length) fs.writeFileSync(outPath, Buffer.concat(chunks));
  console.log(JSON.stringify(result, null, 1));
  process.exit(result.errors.length ? 2 : 0);
});
asset.start(true);

// Aurora swallows some decoder throws into 'error'; guard against silence.
setTimeout(() => {
  result.decodeMs = +(performance.now() - tRequireAac).toFixed(3);
  result.errors.push('TIMEOUT: no end event within 30 s');
  console.log(JSON.stringify(result, null, 1));
  process.exit(3);
}, 30000).unref();
