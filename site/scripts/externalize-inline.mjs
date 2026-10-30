// Post-build step: move every inline <script> in dist/**/*.html into a file under dist/_astro/
// and point the page at it, so the site works under a Content Security Policy of script-src 'self'.
// Context: Starlight ships a few `is:inline` scripts (search shortcut label, sidebar state
// restore). Scripts run in the same order as before because the replacement tag stays in place.
// Rules: JSON data blocks (type application/json, application/ld+json) are left alone. Then
// scripts/check-csp.mjs confirms nothing inline is left. Run after `astro build` (npm run build).
// Do not: add a hash allow-list instead; the point is one rule for the host, 'self'.
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, writeFileSync, statSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = join(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
const rawBase = process.env.SITE_BASE || '/';
const base = rawBase === '/' ? '/' : `/${rawBase.replace(/^\/+|\/+$/g, '')}/`;
mkdirSync(join(dist, '_astro'), { recursive: true });

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* walk(p);
    else if (p.endsWith('.html')) yield p;
  }
}

let moved = 0;
const files = new Map();
for (const file of walk(dist)) {
  const html = readFileSync(file, 'utf8');
  const out = html.replace(/<script((?:\s[^>]*)?)>([\s\S]*?)<\/script>/g, (all, attrs, body) => {
    if (/\ssrc=/.test(attrs) || !body.trim()) return all;
    if (/type="(?:application\/json|application\/ld\+json|importmap)"/.test(attrs)) return all;
    const name = `inline-${createHash('sha256').update(body).digest('hex').slice(0, 10)}.js`;
    files.set(name, body);
    moved++;
    return `<script${attrs} src="${base}_astro/${name}"></script>`;
  });
  if (out !== html) writeFileSync(file, out);
}
for (const [name, body] of files) writeFileSync(join(dist, '_astro', name), body);
console.log(`externalized ${moved} inline scripts into ${files.size} files under _astro/`);
