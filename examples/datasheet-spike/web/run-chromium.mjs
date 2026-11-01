// Drives headless Chromium over the DevTools protocol (Node 22+, no dependencies) so the wasm check
// runs on a real clock. Usage: node run-chromium.mjs <url> [port]. Prints the page's RESULT line.
import { spawn } from 'node:child_process';
const url = process.argv[2];
const port = process.argv[3] || '9333';
const bin = process.env.CHROMIUM || 'chromium-browser';
const proc = spawn(bin, ['--headless=new', '--no-sandbox', '--disable-gpu', `--remote-debugging-port=${port}`,
  '--user-data-dir=' + (process.env.TMPDIR || '/tmp') + '/spike-chromium-' + port, 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
try {
  let targets;
  for (let i = 0; i < 50; i++) {
    try { targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (targets.length) break; } catch {}
    await sleep(200);
  }
  const page = targets.find((t) => t.type === 'page');
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0; const pending = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
  const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Page.navigate', { url });
  for (let i = 0; i < 300; i++) {
    await sleep(500);
    const r = await send('Runtime.evaluate', { expression: "document.getElementById('out') && document.getElementById('out').textContent", returnByValue: true });
    const v = r.result && r.result.result && r.result.result.value;
    if (v && v.startsWith('RESULT')) { console.log(v); break; }
  }
  ws.close();
} finally { proc.kill(); }
process.exit(0);
