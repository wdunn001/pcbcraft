// Feature list rules shared by the Astro pages (src/lib/features.ts) and the build gate
// (scripts/check-features.mjs). Plain JS so node can import it without a build step.
// Context: src/data/features.yaml is the one source for the home page list and /features/.
// Rules: an entry with no valid evidence is an error; evidence ids are checked against the
// generated site data (commands.json, mcp-tools.json) and the repository when those exist.
// Do not: loosen a check to make an entry pass; add the proof instead.
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { parse } from 'yaml';

/** Feature areas in display order. `tone` picks the layer colour of the pad dot. */
export const AREAS = [
  { id: 'schematic', label: 'Schematic', tone: 'sch' },
  { id: 'layout', label: 'Board layout and routing', tone: 'pcb' },
  { id: 'libraries', label: 'Libraries and part search', tone: 'lib' },
  { id: 'rules', label: 'Rules and checks', tone: 'sch' },
  { id: 'view3d', label: '3D view', tone: 'pcb' },
  { id: 'outputs', label: 'Outputs and fabrication', tone: 'out' },
  { id: 'machines', label: 'CAM and machines', tone: 'out' },
  { id: 'assistant', label: 'Assistant and MCP', tone: 'lib' },
  { id: 'access', label: 'Accessibility and languages', tone: 'sch' },
  { id: 'browser', label: 'Browser build', tone: 'pcb' },
];

/** Status marks. The label is always shown as text next to the dot. */
export const STATUSES = {
  browser: 'Desktop and browser',
  available: 'Available',
  desktop: 'Desktop only',
};

const EVIDENCE = /^(command|mcp|test|issue|file):(\S.*)$/;
const BANNED = /\b(kicad|altium|eagle|easyeda|orcad|allegro|librepcb|diptrace|proteus|autodesk|fusion 360)\b/i;

/** All Rust source text under crates/ and apps/, or null outside a full checkout. */
function rustSource(repoRoot) {
  if (!repoRoot || !existsSync(join(repoRoot, 'Cargo.toml'))) return null;
  const parts = [];
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      const p = join(dir, name);
      if (name === 'target' || name === 'node_modules') continue;
      if (statSync(p).isDirectory()) walk(p);
      else if (name.endsWith('.rs')) parts.push(readFileSync(p, 'utf8'));
    }
  };
  for (const d of ['crates', 'apps']) if (existsSync(join(repoRoot, d))) walk(join(repoRoot, d));
  return parts.join('\n');
}

function readJson(path) {
  return existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : null;
}

/**
 * Parse and validate features.yaml text. Returns the entries; throws one Error that lists
 * every problem so a contributor fixes them in one pass.
 * @param {string} text
 * @param {{ dataDir?: string, repoRoot?: string }} [opts]
 */
export function parseFeatures(text, opts = {}) {
  const list = parse(text);
  const problems = [];
  if (!Array.isArray(list)) throw new Error('features.yaml must be a list of entries');
  const areaIds = new Set(AREAS.map((a) => a.id));
  const commands = opts.dataDir ? readJson(join(opts.dataDir, 'commands.json')) : null;
  const tools = opts.dataDir ? readJson(join(opts.dataDir, 'mcp-tools.json')) : null;
  const commandIds = commands ? new Set((commands.commands ?? commands).map((c) => c.id)) : null;
  const toolNames = tools ? new Set((tools.tools ?? tools).map((t) => t.name)) : null;
  const rust = rustSource(opts.repoRoot);
  const seen = new Set();
  const keyCount = {};

  list.forEach((e, i) => {
    const where = `entry ${i + 1} (${e?.name ?? 'no name'})`;
    for (const f of ['area', 'name', 'summary', 'status']) {
      if (typeof e?.[f] !== 'string' || !e[f].trim()) problems.push(`${where}: missing ${f}`);
    }
    if (e.area && !areaIds.has(e.area)) problems.push(`${where}: unknown area ${e.area}`);
    if (e.status && !(e.status in STATUSES)) problems.push(`${where}: unknown status ${e.status}`);
    if (seen.has(`${e.area}/${e.name}`)) problems.push(`${where}: duplicate name in area`);
    seen.add(`${e.area}/${e.name}`);
    if (e.key) keyCount[e.area] = (keyCount[e.area] ?? 0) + 1;
    if (BANNED.test(`${e.name} ${e.summary}`)) problems.push(`${where}: names another tool`);
    if (typeof e.summary === 'string' && /[–—]/.test(e.summary)) problems.push(`${where}: dash in summary`);
    if (!Array.isArray(e.evidence) || e.evidence.length === 0) {
      problems.push(`${where}: no evidence`);
      return;
    }
    for (const ev of e.evidence) {
      const m = EVIDENCE.exec(String(ev));
      if (!m) {
        problems.push(`${where}: bad evidence "${ev}" (use command:, mcp:, test:, issue:#N or file:)`);
        continue;
      }
      const [, kind, id] = m;
      if (kind === 'command' && commandIds && !commandIds.has(id)) problems.push(`${where}: command ${id} not in commands.json`);
      if (kind === 'mcp' && toolNames && !toolNames.has(id) && !(commandIds && commandIds.has(id))) problems.push(`${where}: tool ${id} not in mcp-tools.json`);
      if (kind === 'test' && rust && !new RegExp(`fn ${id}\\b`).test(rust)) problems.push(`${where}: test ${id} not found in crates or apps`);
      if (kind === 'issue' && !/^#\d+$/.test(id)) problems.push(`${where}: issue evidence must look like issue:#123`);
      if (kind === 'file' && opts.repoRoot && existsSync(join(opts.repoRoot, 'Cargo.toml')) && !existsSync(join(opts.repoRoot, id))) {
        problems.push(`${where}: file ${id} not found in the repository`);
      }
    }
  });
  for (const [area, n] of Object.entries(keyCount)) if (n > 4) problems.push(`area ${area}: ${n} key entries, at most 4`);
  for (const a of AREAS) if (!list.some((e) => e.area === a.id)) problems.push(`area ${a.id} has no entries`);
  if (problems.length) throw new Error(`features.yaml:\n  ${problems.join('\n  ')}`);
  return list;
}
