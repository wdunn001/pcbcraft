// Builds the website docs section from the repository pages. Context: the pages under docs/
// are the source. This script checks docs-map.json, then writes one Starlight page per mapped
// page into src/content/docs/docs/<section>/<slug>.md. Those files are generated and ignored by git.
// Rules: runs before dev, check and build. Links between mapped pages become relative site links.
// A link to an excluded page goes to its siteLinks page, or becomes plain text. Any other link into the repository points to GitHub.
// Do not: edit the generated pages. Edit the page under docs/ and run the build again.
import { readdirSync, readFileSync, writeFileSync, mkdirSync, rmSync, existsSync, statSync } from 'node:fs';
import { join, dirname, posix } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkMap, pageSlug } from './docs-map-core.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const siteDir = join(here, '..');
const docsDir = join(siteDir, '..', 'docs');
const outRoot = join(siteDir, 'src', 'content', 'docs', 'docs');
const REPO_URL = 'https://github.com/wdunn001/pcbcraft';
const PAGE_EXT = /\.(md|toml)$/;

/** Every page-like file under docs/, as paths relative to docs/ with forward slashes. */
export function listDocs(dir = docsDir, prefix = '') {
  const out = [];
  for (const name of readdirSync(dir).sort()) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...listDocs(full, `${prefix}${name}/`));
    else if (PAGE_EXT.test(name)) out.push(`${prefix}${name}`);
  }
  return out;
}

/** Reads docs-map.json. */
export function loadMap() {
  return JSON.parse(readFileSync(join(siteDir, 'docs-map.json'), 'utf8'));
}

function sitePath(file, entry) {
  return `${entry.section}/${pageSlug(file, entry)}`;
}

function rewriteLinks(body, file, map) {
  const fromDir = posix.dirname(file);
  const from = sitePath(file, map.pages[file]);
  const lines = body.split('\n');
  let fenced = false;
  return lines
    .map((line) => {
      if (/^\s*(```|~~~)/.test(line)) fenced = !fenced;
      if (fenced) return line;
      return line.replace(/\[([^\]]*)\]\(([^)\s]+)\)/g, (all, text, href) => {
        if (/^([a-z][a-z0-9+.-]*:|#|\/\/)/i.test(href)) return all;
        const [path, hash = ''] = href.split('#');
        const target = posix.normalize(posix.join(fromDir, path));
        const anchor = hash ? `#${hash}` : '';
        if (map.pages[target]) {
          const to = sitePath(target, map.pages[target]);
          const rel = posix.relative(`/docs/${from}/`, `/docs/${to}/`);
          return `[${text}](${rel}/${anchor})`;
        }
        const alt = map.siteLinks?.[target];
        if (alt) return `[${text}](${posix.relative(`/docs/${from}/`, `/docs/${alt}/`)}/${anchor})`;
        if (map.excluded[target]) return text;
        const inRepo = posix.normalize(posix.join('docs', target));
        if (inRepo.startsWith('..')) return text;
        const kind = existsSync(join(siteDir, '..', inRepo)) && statSync(join(siteDir, '..', inRepo)).isDirectory() ? 'tree' : 'blob';
        return `[${text}](${REPO_URL}/${kind}/main/${inRepo.replace(/\/$/, '')}${anchor})`;
      });
    })
    .join('\n');
}

function describe(body) {
  for (const para of body.split(/\n\s*\n/)) {
    const text = para.trim();
    if (!text || /^(#|```|\||[-*] |\d+\. |>)/.test(text)) continue;
    const plain = text.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*_]/g, '').replace(/\s+/g, ' ');
    return plain.length > 160 ? `${plain.slice(0, plain.lastIndexOf(' ', 157))}...` : plain;
  }
  return '';
}

/** Writes the generated pages. Throws when the map has problems. */
export function sync() {
  const map = loadMap();
  const problems = checkMap(listDocs(), map);
  if (problems.length) throw new Error(`docs-map.json check failed:\n${problems.join('\n')}`);
  const dirs = new Set(Object.values(map.pages).map((e) => e.section));
  for (const section of dirs) {
    const dir = join(outRoot, section);
    mkdirSync(dir, { recursive: true });
    // Remove stale generated pages from an earlier run.
    for (const name of readdirSync(dir)) {
      if (name !== 'overview.md' && name.endsWith('.md') && readFileSync(join(dir, name), 'utf8').includes('generated: docs/')) {
        rmSync(join(dir, name));
      }
    }
  }
  let order = 10;
  let count = 0;
  for (const [file, entry] of Object.entries(map.pages)) {
    const raw = readFileSync(join(docsDir, file), 'utf8').replace(/\r\n/g, '\n');
    const m = raw.match(/^# (.+)\n/);
    const title = m ? m[1].trim() : pageSlug(file, entry);
    const body = rewriteLinks(m ? raw.slice(m[0].length) : raw, file, map).trim();
    const description = describe(body);
    const front = [
      '---',
      `title: ${JSON.stringify(title)}`,
      ...(description ? [`description: ${JSON.stringify(description)}`] : []),
      'sidebar:',
      `  order: ${order++}`,
      `# generated: docs/${file}`,
      '---',
    ].join('\n');
    writeFileSync(join(outRoot, entry.section, `${pageSlug(file, entry)}.md`), `${front}\n\n${body}\n`);
    count++;
  }
  return count;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    console.log(`docs synced: ${sync()} pages from docs/`);
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
