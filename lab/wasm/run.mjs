// TASK-100: run syom's WASM build under Node and compare with the native
// reference (`syom_wasm_native`). Usage: node run.mjs
import { readFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';

const here = new URL('.', import.meta.url).pathname;
const wasmPath = here + 'target/wasm32-unknown-unknown/release/syom_lab_wasm.wasm';
const goldens = here + '../../src/goldens/';
const DECODE = ['sine48.adts', 'sine441.m4a', 'he48.adts', 'he48.m4a', 'ps48.adts', 'latm48.latm', 'mc51.adts', 'mc71.adts', 'lecture.m4a'];
const ENCODE = ['enc:0', 'enc:1', 'enc:2', 'enc:3'];

const bytes = readFileSync(wasmPath);
const t0 = performance.now();
const module = await WebAssembly.compile(bytes);
const t1 = performance.now();
const { exports: x } = await WebAssembly.instantiate(module, {});
const t2 = performance.now();
const mem = () => x.memory.buffer.byteLength;
const startMem = mem();

const result = () => new Uint32Array(x.memory.buffer, x.syom_result(), 12);
const hex = (lo, hi) => (BigInt(hi) << 32n | BigInt(lo)).toString(16).padStart(16, '0');
const median = (v) => [...v].sort((a, b) => a - b)[v.length >> 1];

function decode(buf) {
  const p = x.syom_alloc(buf.length);
  new Uint8Array(x.memory.buffer, p, buf.length).set(buf);   // the one input copy
  const t = performance.now();
  const status = x.syom_decode(p, buf.length);
  const ms = performance.now() - t;
  x.syom_free(p, buf.length);
  const r = result();
  return { status, channels: r[1], rate: r[2], frames: r[3], samples: r[4] + r[5] * 2 ** 32, hash: hex(r[6], r[7]), first: r[10], ms };
}

function encode(mode) {
  const planes = mode === 1 ? 1 : 2, n = 96000;
  const p = x.syom_alloc(planes * n * 4);
  x.syom_test_pcm(p, planes, n);
  const t = performance.now();
  const status = x.syom_encode(p, planes, n, 48000, mode);
  const ms = performance.now() - t;
  x.syom_free(p, planes * n * 4);
  const r = result();
  return { status, bytes: r[9], hash: hex(r[6], r[7]), ms };
}

const dump = mkdtempSync(tmpdir() + '/syom-wasm-');
const native = Object.fromEntries(
  execFileSync(here + 'target/release/syom_wasm_native', [...DECODE, ...ENCODE], { encoding: 'utf8', env: { ...process.env, SYOM_DUMP: dump } })
    .trim().split('\n').map((l) => JSON.parse(l)).map((o) => [o.case, o]));

const rows = [];
let bad = 0;
for (const name of DECODE) {
  const buf = readFileSync(goldens + name);
  const runs = Array.from({ length: 7 }, () => decode(buf));
  const w = runs[0], n = native[name];
  const shape = w.samples === n.samples && w.channels === n.channels && w.rate === n.rate && w.frames === n.frames;
  // Bit-exactness is not promised for decode (table setup uses the
  // platform's sin/cos); the contract is the s16 sample. Measure the gap.
  const p = x.syom_alloc(buf.length);
  new Uint8Array(x.memory.buffer, p, buf.length).set(buf);
  x.syom_decode_pcm(p, buf.length);
  x.syom_free(p, buf.length);
  const rr = result();
  const wasmPcm = new Float32Array(x.memory.buffer.slice(rr[8], rr[8] + rr[9]));
  const nat = readFileSync(dump + '/' + name + '.f32');
  const natPcm = new Float32Array(nat.buffer, nat.byteOffset, nat.length / 4);
  let worst = 0, s16diff = 0;
  for (let i = 0; i < natPcm.length; i++) {
    worst = Math.max(worst, Math.abs(natPcm[i] - wasmPcm[i]) * 32768);
    if (Math.round(natPcm[i] * 32768) !== Math.round(wasmPcm[i] * 32768)) s16diff++;
  }
  const lenOk = natPcm.length === wasmPcm.length;
  const same = w.hash === n.hash ? 'bit-identical' : `max ${worst.toExponential(1)} LSB, ${s16diff} of ${natPcm.length} s16 samples differ`;
  bad += !(shape && lenOk && worst < 0.01) || w.status !== 0;
  const secs = w.samples / w.rate;
  rows.push(`| decode ${name} | ${w.channels} ch ${w.rate} Hz, ${w.frames} frames | ${same} | ${median(runs.map((r) => r.ms)).toFixed(2)} | ${n.ms.toFixed(2)} | ${(secs * 1000 / median(runs.map((r) => r.ms))).toFixed(0)}× |`);
}
for (const name of ENCODE) {
  const mode = Number(name.slice(4));
  const runs = Array.from({ length: 5 }, () => encode(mode));
  const w = runs[0], n = native[name];
  const same = w.hash === n.hash && w.bytes === n.bytes;
  bad += !same || w.status !== 0;
  rows.push(`| encode ${['LC 128k ADTS', 'HE v1 48k ADTS (mono)', 'HE v2 32k ADTS', 'LC 128k M4A'][mode]} 2 s | ${w.bytes} B | ${same ? 'identical' : 'DIFFERENT'} | ${median(runs.map((r) => r.ms)).toFixed(2)} | ${n.ms.toFixed(2)} | ${(2000 / median(runs.map((r) => r.ms))).toFixed(0)}× |`);
}
// First output: the time to decode a stream holding only the first ADTS frame.
const adts = readFileSync(goldens + 'sine48.adts');
const len = ((adts[3] & 3) << 11) | (adts[4] << 3) | (adts[5] >> 5);
const firsts = Array.from({ length: 9 }, () => decode(adts.subarray(0, len)).ms);
// Memory growth: 200 more decodes must not grow linear memory.
const before = mem();
for (let i = 0; i < 200; i++) decode(adts);
const after = mem();

console.log(`node ${process.version}, wasm ${bytes.length} bytes`);
console.log(`compile ${(t1 - t0).toFixed(1)} ms, instantiate ${(t2 - t1).toFixed(1)} ms, linear memory at start ${startMem} bytes`);
console.log(`first frame decode (one ADTS frame in, PCM out): median ${median(firsts).toFixed(3)} ms`);
console.log(`linear memory after the table: ${before} bytes; after 200 more decodes: ${after} bytes`);
console.log('| case | output | vs native | wasm ms | native ms | wasm ×realtime |');
console.log('|---|---|---|---:|---:|---:|');
rows.forEach((r) => console.log(r));
process.exit(bad ? 1 : 0);
