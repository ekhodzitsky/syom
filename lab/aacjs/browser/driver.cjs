// TASK-20 lab: drive headless Firefox via geckodriver (W3C WebDriver REST)
// to run browser/page.html and collect the JSON result.
// Usage: node browser/driver.cjs   (requires geckodriver on PATH)
'use strict';

const { spawn } = require('child_process');
const path = require('path');

const PORT = 4571;
const BASE = `http://127.0.0.1:${PORT}`;

async function waitReady() {
  for (let i = 0; i < 60; i++) {
    try {
      const r = await fetch(`${BASE}/status`);
      if (r.ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error('geckodriver did not come up');
}

async function main() {
  const gd = spawn('geckodriver', ['-p', String(PORT)], { stdio: ['ignore', 'pipe', 'inherit'] });
  try {
    await waitReady();
    const sess = await fetch(`${BASE}/session`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        capabilities: {
          alwaysMatch: {
            browserName: 'firefox',
            'moz:firefoxOptions': { args: ['-headless'] },
          },
        },
      }),
    }).then((r) => r.json());
    const sid = sess.value.sessionId;
    const page = 'file://' + path.join(__dirname, 'page.html');
    await fetch(`${BASE}/session/${sid}/url`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ url: page }),
    }).then((r) => r.json());

    let text = 'RUNNING';
    for (let i = 0; i < 240 && text === 'RUNNING'; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const res = await fetch(`${BASE}/session/${sid}/execute/sync`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          script: "return (document.getElementById('result')||{}).textContent || 'RUNNING';",
          args: [],
        }),
      }).then((r) => r.json());
      text = res.value;
    }
    console.log(text);
    await fetch(`${BASE}/session/${sid}`, { method: 'DELETE' });
  } finally {
    gd.kill();
  }
}

main().catch((e) => { console.error('DRIVER FAILED:', e.message); process.exit(1); });
