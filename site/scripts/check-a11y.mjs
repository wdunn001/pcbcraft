// Browser checks over the built site: axe-core on every page in dark and light, a click-through
// of the header nav, and (with --shots DIR) full-page PNGs at 1440 and 390 px in both themes.
// Context: needs playwright-core and axe-core (see PW_DIR below); run on the build host inside the
// Playwright image, not on the laptop. Serves dist/ under the configured base like a project
// Pages site. Exits 1 on any axe violation of impact serious or critical, or any broken click.
// Run: PW_DIR=/tmp/pw SITE_BASE=/ node scripts/check-a11y.mjs [--shots out/]
// Do not: relax the impact filter to pass; fix the page.
import http from 'node:http';
import { createRequire } from 'node:module';
import { readFileSync, existsSync, statSync, mkdirSync, readdirSync } from 'node:fs';
import { join, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(join(process.env.PW_DIR || '/tmp/pw', 'x.js'));
const { chromium } = require('playwright-core');
const axeSource = readFileSync(require.resolve('axe-core/axe.min.js'), 'utf8');

const dist = join(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
const rawBase = process.env.SITE_BASE || '/';
const base = rawBase === '/' ? '/' : `/${rawBase.replace(/^\/+|\/+$/g, '')}/`;
const shotsIdx = process.argv.indexOf('--shots');
const shotsDir = shotsIdx > 0 ? process.argv[shotsIdx + 1] : null;
const MIME = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.svg': 'image/svg+xml', '.json': 'application/json', '.woff2': 'font/woff2', '.woff': 'font/woff', '.png': 'image/png', '.xml': 'application/xml', '.txt': 'text/plain', '.zip': 'application/zip', '.wasm': 'application/wasm' };

const server = http.createServer((req, res) => {
  const path = decodeURIComponent(req.url.split(/[?#]/)[0]);
  if (!path.startsWith(base)) { res.writeHead(404); return res.end('outside base'); }
  let file = join(dist, path.slice(base.length));
  if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html');
  if (!existsSync(file)) { res.writeHead(404); return res.end('not found'); }
  res.writeHead(200, { 'Content-Type': MIME[extname(file)] || 'application/octet-stream', 'Content-Security-Policy': (path.startsWith(`${base}docs/`) || path.startsWith(`${base}pagefind/`)) ? "script-src 'self' 'wasm-unsafe-eval'" : "script-src 'self'" });
  res.end(readFileSync(file));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const origin = `http://127.0.0.1:${server.address().port}`;

function* pages(dir = dist, prefix = '') {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) { if (name !== '_astro' && name !== 'pagefind') yield* pages(p, `${prefix}${name}/`); }
    else if (name === 'index.html') yield prefix;
  }
}
const routes = [...pages()].sort();
const browser = await chromium.launch();
const failures = [];

for (const theme of ['dark', 'light']) {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: theme, bypassCSP: true });
  await ctx.addInitScript((t) => { try { localStorage.setItem('starlight-theme', t); } catch {} }, theme);
  const page = await ctx.newPage();
  for (const route of routes) {
    await page.goto(`${origin}${base}${route}`, { waitUntil: 'networkidle' });
    await page.addScriptTag({ content: axeSource });
    const result = await page.evaluate(() => axe.run(document, { runOnly: ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa', 'best-practice'] }));
    for (const v of result.violations.filter((x) => x.impact === 'serious' || x.impact === 'critical')) {
      failures.push(`axe ${theme} /${route}: ${v.id} (${v.impact}) ${v.nodes.slice(0, 2).map((n) => n.target.join(' ')).join(' | ')}`);
    }
  }
  await ctx.close();
}

// Click-through: from the home page follow every header nav link, then back.
{
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  await page.goto(`${origin}${base}`, { waitUntil: 'networkidle' });
  const labels = await page.locator('header nav[aria-label="Main"] a').allInnerTexts();
  for (const label of labels) {
    await page.goto(`${origin}${base}`, { waitUntil: 'networkidle' });
    const resp = await Promise.all([page.waitForNavigation(), page.locator('header nav[aria-label="Main"] a', { hasText: label }).first().click()]);
    const status = (await resp[0]?.status?.()) ?? 200;
    const h1 = await page.locator('h1').first().innerText();
    if (status >= 400 || !h1) failures.push(`click ${label}: status ${status}, h1 "${h1}"`);
    console.log(`nav ${label.padEnd(10)} -> ${page.url().replace(origin, '')}  h1="${h1.slice(0, 50)}"`);
  }
  await ctx.close();
}

// CSP: pages are served with script-src 'self'; /docs/ also gets 'wasm-unsafe-eval' because the
// Pagefind search loads WebAssembly. The docs search is opened and used; any violation fails.
{
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: 'dark' });
  const page = await ctx.newPage();
  const csp = [];
  page.on('console', (m) => { if (/Content Security Policy/i.test(m.text())) csp.push(page.url() + ' ' + m.text().slice(0, 120)); });
  page.on('pageerror', (e) => { if (/Content Security Policy|WebAssembly/i.test(e.message)) csp.push(e.message.slice(0, 140)); });
  for (const route of ['', 'features/', 'docs/guide/getting-started/']) {
    await page.goto(`${origin}${base}${route}`, { waitUntil: 'networkidle' });
    await page.evaluate(() => { localStorage.clear(); document.documentElement.dataset.theme = 'dark'; });
    await page.click('[data-theme-toggle]');
    const t = await page.evaluate(() => document.documentElement.dataset.theme);
    if (t !== 'light') failures.push(`theme toggle on /${route}: data-theme is ${t} after click`);
  }
  await page.click('button[data-open-modal]');
  await page.waitForSelector('dialog[open] input', { timeout: 5000 }).catch(() => failures.push('docs search dialog did not open'));
  await page.keyboard.type('getting');
  await page.waitForTimeout(1500);
  const results = await page.locator('dialog[open] .pagefind-ui__result, dialog[open] li a').count();
  console.log(`docs search under CSP: ${results} result links, ${csp.length} CSP messages`);
  for (const m of new Set(csp)) failures.push(`csp: ${m}`);
  await ctx.close();
}

// Phone width: no page may scroll sideways at 360 px.
{
  const ctx = await browser.newContext({ viewport: { width: 360, height: 740 } });
  const page = await ctx.newPage();
  for (const route of routes) {
    await page.goto(`${origin}${base}${route}`, { waitUntil: 'networkidle' });
    const over = await page.evaluate(() => {
      const w = document.documentElement.clientWidth;
      const bad = [...document.querySelectorAll('body *')].filter((e) => e.getBoundingClientRect().right > w + 1 && !e.closest('pre, .expressive-code, table')).slice(0, 3).map((e) => `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 40)}`);
      return { scroll: document.documentElement.scrollWidth - w, bad };
    });
    if (over.scroll > 1) failures.push(`overflow at 360 px on /${route}: ${over.scroll}px wider; ${over.bad.join(' | ')}`);
  }
  await ctx.close();
}

// Feature page filter: search narrows the list, an area pill leaves one section.
{
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  await page.goto(`${origin}${base}features/`, { waitUntil: 'networkidle' });
  const total = await page.locator('.feat').count();
  await page.fill('#q', 'gerber');
  const hits = await page.locator('.feat:not([hidden])').count();
  if (hits < 1 || hits >= total) failures.push(`feature search: ${hits} of ${total} shown for "gerber"`);
  await page.fill('#q', '');
  await page.click('.pill[data-area="rules"]');
  const sections = await page.locator('[data-area-section]:not([hidden])').count();
  if (sections !== 1) failures.push(`feature area filter: ${sections} sections shown, expected 1`);
  console.log(`filter ok: search ${hits}/${total}, area pill leaves ${sections} section`);
  await ctx.close();
}

if (shotsDir) {
  mkdirSync(shotsDir, { recursive: true });
  for (const [w, h, tag] of [[1440, 900, '1440'], [390, 844, '390']]) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, deviceScaleFactor: 1, reducedMotion: 'reduce' });
      await ctx.addInitScript((t) => { try { localStorage.setItem('starlight-theme', t); } catch {} }, theme);
      const page = await ctx.newPage();
      for (const route of routes) {
        await page.goto(`${origin}${base}${route}`, { waitUntil: 'networkidle' });
        await page.evaluate(async () => { for (let y = 0; y < document.body.scrollHeight; y += 600) { window.scrollTo({ top: y, behavior: 'instant' }); await new Promise((r) => setTimeout(r, 40)); } window.scrollTo({ top: 0, behavior: 'instant' }); await new Promise((r) => setTimeout(r, 150)); });
        const name = (route.replace(/\/$/, '').replace(/\//g, '-') || 'home');
        await page.screenshot({ path: join(shotsDir, `${name}-${tag}-${theme}.png`), fullPage: true });
      }
      await ctx.close();
    }
  }
}
await browser.close();
server.close();
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log(`a11y ok: ${routes.length} pages x 2 themes, base ${base}`);
