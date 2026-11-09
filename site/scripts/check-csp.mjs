// CSP gate: the built pages must contain no inline script, no inline event handler attribute
// and no javascript: URL, because the host sends script-src 'self'. Test name:
// no_inline_scripts_in_dist. Run after `npm run build`.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = join(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* walk(p);
    else if (p.endsWith('.html')) yield p;
  }
}
const problems = [];
let pages = 0;
for (const file of walk(dist)) {
  pages++;
  const html = readFileSync(file, 'utf8');
  for (const m of html.matchAll(/<script((?:\s[^>]*)?)>([\s\S]*?)<\/script>/g)) {
    if (/\ssrc=/.test(m[1]) || /type="(?:application\/json|application\/ld\+json|importmap)"/.test(m[1])) continue;
    problems.push(`${file}: inline script`);
  }
  if (/\son[a-z]+="/.test(html.replace(/<script[\s\S]*?<\/script>/g, ''))) problems.push(`${file}: inline event handler attribute`);
  if (/href="javascript:/i.test(html)) problems.push(`${file}: javascript: url`);
}
if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`no_inline_scripts_in_dist: ok, ${pages} pages`);
