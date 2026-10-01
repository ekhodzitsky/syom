// TASK-20 lab bench: AAC.js/Aurora.js decode timing, warm-up, memory, payload.
// Usage:
//   node bench.cjs              -> warm-up curve + steady throughput + memory
//   node --expose-gc bench.cjs  -> adds gc-separated heap numbers
//   node bench.cjs --startup    -> require + first-decode latency (this process)
'use strict';

const fs = require('fs');
const path = require('path');

const INPUT = path.join(__dirname, 'out', 'bench_10s.adts'); // 10 s 48 kHz mono LC, pns/tns off
const ITERS = 60;
const WARM_SKIP = 10;

function decodeOnce(AV) {
  return new Promise((resolve, reject) => {
    const asset = AV.Asset.fromFile(INPUT);
    let floats = 0;
    let failed = false;
    asset.on('error', (e) => { failed = true; reject(new Error(String(e && e.message || e))); });
    asset.on('data', (c) => { floats += c.length; });
    asset.on('end', () => resolve(floats));
    asset.start(true);
    setTimeout(() => reject(new Error('timeout')), 30000).unref();
  });
}

function memMB() {
  const m = process.memoryUsage();
  return { rss: +(m.rss / 1048576).toFixed(1), heapUsed: +(m.heapUsed / 1048576).toFixed(1), heapTotal: +(m.heapTotal / 1048576).toFixed(1) };
}

async function main() {
  const t0 = performance.now();
  const AV = require('av');
  const tAv = performance.now();
  require('aac');
  const tAac = performance.now();

  if (process.argv.includes('--startup')) {
    const t1 = performance.now();
    const floats = await decodeOnce(AV);
    const t2 = performance.now();
    console.log(JSON.stringify({
      requireAvMs: +(tAv - t0).toFixed(2),
      requireAacMs: +(tAac - tAv).toFixed(2),
      firstDecodeMs: +(t2 - t1).toFixed(2),
      firstFloats: floats,
    }));
    return;
  }

  // payload: every file Node actually loads for require('av') + require('aac')
  const files = Object.keys(require.cache).filter((f) => f.includes(`${path.sep}node_modules${path.sep}`));
  let bytes = 0;
  for (const f of files) bytes += fs.statSync(f).size;

  const durSec = 10.0;
  const times = [];
  let floats = 0;
  const mBefore = memMB();
  if (global.gc) global.gc();
  const mGcBefore = memMB();
  for (let i = 0; i < ITERS; i++) {
    const a = performance.now();
    floats = await decodeOnce(AV);
    times.push(performance.now() - a);
  }
  const mAfter = memMB();
  if (global.gc) global.gc();
  const mGcAfter = memMB();

  const steady = times.slice(WARM_SKIP).sort((a, b) => a - b);
  const median = steady[Math.floor(steady.length / 2)];
  const p90 = steady[Math.floor(steady.length * 0.9)];
  console.log(JSON.stringify({
    input: path.basename(INPUT),
    durationSec: durSec,
    floatsPerDecode: floats,
    payloadFiles: files.length,
    payloadBytes: bytes,
    firstIterMs: +times[0].toFixed(2),
    warmupCurveMs: times.slice(0, 12).map((t) => +t.toFixed(2)),
    steadyMedianMs: +median.toFixed(2),
    steadyP90Ms: +p90.toFixed(2),
    steadyRealtimeX: +(durSec * 1000 / median).toFixed(1),
    jitSpeedupFirstToMedian: +(times[0] / median).toFixed(2),
    memBefore: mBefore,
    memGcBefore: global.gc ? mGcBefore : null,
    memAfter60: mAfter,
    memGcAfter60: global.gc ? mGcAfter : null,
  }, null, 1));
}

main().catch((e) => { console.error('BENCH FAILED:', e.message); process.exit(1); });
