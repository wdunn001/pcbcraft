// Serves the trunk dist/ folder, loads the page in headless Chromium with WebGPU
// enabled, and fails on any console panic (including "time not implemented").
// Run from apps/pcbcraft-web after trunk build: node scripts/smoke-first-frame.mjs
import http from 'node:http';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { join, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'dist');
if (!existsSync(join(dist, 'index.html'))) {
  console.error('missing dist/index.html; run trunk build first');
  process.exit(1);
}

const MIME = {
  '.html': 'text/html',
  '.css': 'text/css',
  '.js': 'text/javascript',
  '.wasm': 'application/wasm',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
};

const server = http.createServer((req, res) => {
  const path = decodeURIComponent(req.url.split(/[?#]/)[0]);
  let file = join(dist, path === '/' ? 'index.html' : path.replace(/^\//, ''));
  if (existsSync(file) && statSync(file).isDirectory()) {
    file = join(file, 'index.html');
  }
  if (!existsSync(file)) {
    res.writeHead(404);
    res.end('not found');
    return;
  }
  res.writeHead(200, {
    'Content-Type': MIME[extname(file)] || 'application/octet-stream',
  });
  res.end(readFileSync(file));
});

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;

const failures = [];
const browser = await chromium.launch({
  headless: true,
  args: ['--enable-unsafe-webgpu', '--headless=new'],
});
const context = await browser.newContext();
const page = await context.newPage();

page.on('pageerror', (err) => {
  failures.push(`pageerror: ${err.message}`);
});
page.on('console', (msg) => {
  const text = msg.text();
  if (msg.type() === 'error' || /panic|unreachable|time not implemented/i.test(text)) {
    failures.push(`console ${msg.type()}: ${text}`);
  }
});

await page.goto(`${origin}/`, { waitUntil: 'networkidle', timeout: 120_000 });
await page.waitForTimeout(8_000);

const needsWebGpu = await page.locator('#needs-webgpu').isVisible();
const canvasHidden = await page.locator('#pcbcraft').evaluate((el) => el.hidden);
if (needsWebGpu || canvasHidden) {
  failures.push('the WebGPU notice appeared or the canvas stayed hidden after load');
}

await browser.close();
server.close();

if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log('browser smoke: no panic on the first frames');
