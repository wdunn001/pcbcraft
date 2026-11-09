// Change file reader shared by the /changelog page, the release notes script and the gate.
// Context: each pull request adds one `.changes/<slug>.md` file (format: .changes/README.md).
// The format is line oriented, so this reader splits at the first colon and needs no YAML library.
// Rules: only entries with `user_visible: true` are listed; newest first by commit time in seconds;
// the time and the pull request come from the git history of the change file, never from the file text.
// Do not: read the network. A file that fails to parse is skipped with a warning.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

export const REPO = 'https://github.com/wdunn001/pcbcraft';
export const TYPES = ['feature', 'fix', 'refactor', 'docs', 'test', 'ci', 'chore'];
const KEYS = ['type', 'area', 'issues', 'user_visible', 'docs', 'summary'];

/** Parses one change file. Throws a plain message on any bad or missing field. */
export function parseChange(slug, text) {
  const lines = text.split(/\r?\n/);
  if (lines[0]?.trim() !== '---') throw new Error(`${slug}: the file must start with ---`);
  const map = new Map();
  let closed = false;
  for (const line of lines.slice(1)) {
    if (line.trim() === '---') {
      closed = true;
      break;
    }
    if (!line.trim()) continue;
    const i = line.indexOf(':');
    if (i < 0) throw new Error(`${slug}: line without a colon: ${line}`);
    const key = line.slice(0, i).trim();
    if (!KEYS.includes(key)) throw new Error(`${slug}: unknown key ${key}`);
    if (map.has(key)) throw new Error(`${slug}: duplicate key ${key}`);
    map.set(key, line.slice(i + 1).trim());
  }
  if (!closed) throw new Error(`${slug}: the frontmatter must end with ---`);
  for (const k of KEYS) if (!map.has(k)) throw new Error(`${slug}: missing key ${k}`);
  const kind = map.get('type');
  if (!TYPES.includes(kind)) throw new Error(`${slug}: type ${kind} must be one of ${TYPES.join(', ')}`);
  const issueText = map.get('issues');
  if (!/^\[.*\]$/.test(issueText)) throw new Error(`${slug}: issues must be a list`);
  const issues = issueText
    .slice(1, -1)
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => {
      const n = s.replace(/^#/, '');
      if (!/^\d+$/.test(n)) throw new Error(`${slug}: issue ${s} is not a number`);
      return Number(n);
    });
  const visible = map.get('user_visible');
  if (visible !== 'true' && visible !== 'false') throw new Error(`${slug}: user_visible must be true or false`);
  return {
    slug,
    kind,
    area: map.get('area'),
    issues,
    userVisible: visible === 'true',
    summary: map.get('summary'),
  };
}

/** Writing rules for a summary: one statement, no dashes used as punctuation. Returns problems. */
export function summaryProblems(summary) {
  const problems = [];
  if (!summary) problems.push('the summary is empty');
  if (/[–—]|\s-{1,2}\s/.test(summary)) problems.push('the summary uses a dash as punctuation');
  const sentences = summary.split(/[.!?]\s+(?=[A-Z])/).filter(Boolean);
  if (sentences.length > 1) problems.push('the summary holds more than one sentence');
  if (!/[.!?]$/.test(summary)) problems.push('the summary does not end with a full stop');
  return problems;
}

function git(root, args) {
  try {
    return execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch {
    return '';
  }
}

/** Pull request number from a commit subject: "Merge pull request #N", else the last "(#N)". */
export function prFromSubject(subject) {
  const merge = subject.match(/^Merge pull request #(\d+)/);
  if (merge) return Number(merge[1]);
  const all = [...subject.matchAll(/\(#(\d+)\)/g)];
  if (all.length) return Number(all[all.length - 1][1]);
  const any = subject.match(/#(\d+)/);
  return any ? Number(any[1]) : null;
}

/** "2026-11-06 19:40 UTC" from a Unix time in seconds. */
export function formatUtc(seconds) {
  return `${new Date(seconds * 1000).toISOString().slice(0, 16).replace('T', ' ')} UTC`;
}

/**
 * Time, commit and pull request number of the commit that added a change file to main.
 * The merge commit is the first parent line, so its subject carries the pull request number.
 * `time` is the commit time in Unix seconds, `date` the UTC day.
 */
export function addedBy(root, slug) {
  const paths = [`.changes/${slug}.md`, `.changes/released/*/${slug}.md`];
  const out = git(root, ['log', '--first-parent', '-m', '--diff-filter=A', '--format=%H%x09%ct%x09%s', '--', ...paths]);
  const rows = out.split('\n').filter(Boolean);
  const last = rows[rows.length - 1];
  if (!last) return null;
  const [hash, ct, subject = ''] = last.split('\t');
  const time = Number(ct);
  return { hash, time, date: new Date(time * 1000).toISOString().slice(0, 10), pr: prFromSubject(subject) };
}

/** The line at the top of the page: when it was built and from which commit. */
export function builtLine(root, now = new Date()) {
  const sha = git(root, ['rev-parse', '--short', 'HEAD']) || 'unknown';
  return `Built ${formatUtc(Math.floor(now.getTime() / 1000))} from commit ${sha}`;
}

/** Every user visible entry, newest first. `since` (a commit) drops entries already in it. */
export function loadEntries(root, { since = '' } = {}) {
  const dirs = [join(root, '.changes')];
  const released = join(root, '.changes', 'released');
  if (existsSync(released)) for (const v of readdirSync(released)) dirs.push(join(released, v));
  const entries = [];
  for (const dir of dirs) {
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir)) {
      if (!name.endsWith('.md') || name === 'README.md' || name === 'TEMPLATE.md') continue;
      const slug = name.slice(0, -3);
      let set;
      try {
        set = parseChange(slug, readFileSync(join(dir, name), 'utf8'));
      } catch (e) {
        // Older files predate the format. `cargo xtask changeset check` stops new bad ones in review.
        console.warn(`changelog: skipped ${e.message}`);
        continue;
      }
      if (!set.userVisible) continue;
      const added = addedBy(root, slug);
      entries.push({ ...set, time: added?.time ?? 0, date: added?.date ?? '', hash: added?.hash ?? '', pr: added?.pr ?? null });
    }
  }
  entries.sort((a, b) => b.time - a.time || (b.pr ?? 0) - (a.pr ?? 0) || a.slug.localeCompare(b.slug));
  if (!since) return entries;
  return entries.filter((e) => !e.hash || !isAncestor(root, e.hash, since));
}

function isAncestor(root, commit, of) {
  try {
    execFileSync('git', ['merge-base', '--is-ancestor', commit, of], { cwd: root, stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

/** Links for an entry: the pull request first, then each issue. */
export function entryLinks(entry) {
  const links = [];
  if (entry.pr) links.push({ label: `Pull request #${entry.pr}`, href: `${REPO}/pull/${entry.pr}` });
  for (const n of entry.issues) links.push({ label: `Issue #${n}`, href: `${REPO}/issues/${n}` });
  return links;
}

/** Markdown for the release notes: one bullet per entry, newest first. */
export function renderNotes(entries, siteUrl) {
  const lines = entries.map((e) => {
    const links = entryLinks(e)
      .map((l) => `[${l.label.replace('Pull request ', 'pull request ').replace('Issue ', 'issue ')}](${l.href})`)
      .join(', ');
    return `- ${e.date} ${e.area} ${e.kind}: ${e.summary}${links ? ` (${links})` : ''}`;
  });
  const body = lines.length ? lines.join('\n') : 'No user visible changes since the previous build.';
  return `## What changed\n\n${body}\n\nThe full list is on the changelog page: ${siteUrl}\n`;
}
