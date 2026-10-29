// Link gate over the built site. Context: reads every dist/**/*.html, takes each href and src
// that starts with the configured base (SITE_BASE, default /pcbcraft/) and checks the target
// file exists in dist/. Also fails on root-absolute links that skip the base.
// Run: `SITE_BASE=/pcbcraft/ node scripts/check-links.mjs` after `npm run build`.
// Do not: follow external links (no network in CI for this check).
import { readdirSync, readFileSync, existsSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = join(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
const rawBase = process.env.SITE_BASE || '/pcbcraft/';
const base = rawBase === '/' ? '/' : `/${rawBase.replace(/^\/+|\/+$/g, '')}/`;

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* walk(p);
    else if (p.endsWith('.html')) yield p;
  }
}

const problems = [];
let checked = 0;
for (const file of walk(dist)) {
  const html = readFileSync(file, 'utf8');
  for (const m of html.matchAll(/\s(?:href|src)="([^"#?]*)(?:[#?][^"]*)?"/g)) {
    const link = m[1];
    if (!link.startsWith('/') || link.startsWith('//')) continue;
    checked++;
    if (!link.startsWith(base)) {
      problems.push(`${file}: ${link} skips the base ${base}`);
      continue;
    }
    const rel = link.slice(base.length);
    const target = join(dist, rel);
    const ok = existsSync(target) && (statSync(target).isFile() || existsSync(join(target, 'index.html')));
    if (!ok) problems.push(`${file}: ${link} not found in dist`);
  }
}
if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`links ok: ${checked} internal references, base ${base}`);
