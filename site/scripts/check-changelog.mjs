// Gate for the changelog source. Test names follow the issue: changelog_page_lists_user_visible_entries,
// new_change_file_appears_after_site_build, summary_follows_writing_rules, change_files_preserved_after_release.
// Run: `npm run test:changelog`.
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, renameSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseChange, summaryProblems, loadEntries, renderNotes, entryLinks } from '../src/lib/changelog-core.js';

const make = (extra = {}) => {
  const f = { type: 'fix', area: 'layout', issues: '[12, 13]', user_visible: 'true', docs: '[docs/ci.md]', summary: 'Tracks now snap to the grid.', ...extra };
  return `---\n${Object.entries(f).map(([k, v]) => `${k}: ${v}`).join('\n')}\n---\n`;
};
const git = (cwd, ...a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@example.com', ...a], { cwd, stdio: 'ignore' });
function repo() {
  const dir = mkdtempSync(join(tmpdir(), 'chg-'));
  git(dir, 'init', '-q', '-b', 'main');
  mkdirSync(join(dir, '.changes'));
  return dir;
}
function commit(dir, msg, date) {
  git(dir, 'add', '-A');
  execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@example.com', 'commit', '-q', '-m', msg], {
    cwd: dir, stdio: 'ignore', env: { ...process.env, GIT_COMMITTER_DATE: date, GIT_AUTHOR_DATE: date },
  });
}

test('changelog_page_lists_user_visible_entries', () => {
  const dir = repo();
  writeFileSync(join(dir, '.changes/old.md'), make());
  commit(dir, 'Merge pull request #5 from x/old', '2026-10-01T12:00:00Z');
  writeFileSync(join(dir, '.changes/hidden.md'), make({ user_visible: 'false' }));
  writeFileSync(join(dir, '.changes/new.md'), make({ area: 'site', summary: 'The changelog lists changes.' }));
  commit(dir, 'Merge pull request #9 from x/new', '2026-10-20T12:00:00Z');
  const entries = loadEntries(dir);
  assert.deepEqual(entries.map((e) => [e.slug, e.date, e.pr]), [['new', '2026-10-20', 9], ['old', '2026-10-01', 5]]);
  assert.deepEqual(entryLinks(entries[1]).map((l) => l.label), ['Pull request #5', 'Issue #12', 'Issue #13']);
});

test('new_change_file_appears_after_site_build', () => {
  const dir = repo();
  assert.equal(loadEntries(dir).length, 0);
  writeFileSync(join(dir, '.changes/added.md'), make());
  commit(dir, 'Add (#7)', '2026-11-01T12:00:00Z');
  assert.equal(loadEntries(dir).length, 1);
});

test('change_files_preserved_after_release', () => {
  const dir = repo();
  writeFileSync(join(dir, '.changes/kept.md'), make());
  commit(dir, 'Merge pull request #3 from x/kept', '2026-09-01T12:00:00Z');
  mkdirSync(join(dir, '.changes/released/1.0'), { recursive: true });
  renameSync(join(dir, '.changes/kept.md'), join(dir, '.changes/released/1.0/kept.md'));
  commit(dir, 'Release 1.0', '2026-09-15T12:00:00Z');
  const entries = loadEntries(dir);
  assert.deepEqual(entries.map((e) => [e.slug, e.date, e.pr]), [['kept', '2026-09-01', 3]]);
});

test('release_notes_include_entries_since_last_release', () => {
  const dir = repo();
  writeFileSync(join(dir, '.changes/a.md'), make());
  commit(dir, 'Merge pull request #1 from x/a', '2026-10-01T12:00:00Z');
  const prev = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: dir, encoding: 'utf8' }).trim();
  writeFileSync(join(dir, '.changes/b.md'), make({ summary: 'Parts snap too.' }));
  commit(dir, 'Merge pull request #2 from x/b', '2026-10-02T12:00:00Z');
  const notes = renderNotes(loadEntries(dir, { since: prev }), 'https://example.test/changelog/');
  assert.match(notes, /Parts snap too\./);
  assert.doesNotMatch(notes, /Tracks now snap/);
  assert.match(notes, /\[pull request #2\]\(https:\/\/github\.com\/wdunn001\/pcbcraft\/pull\/2\)/);
});

test('summary_follows_writing_rules', () => {
  assert.deepEqual(summaryProblems('Tracks now snap to the grid.'), []);
  assert.ok(summaryProblems('Tracks snap — always.').length > 0);
  assert.ok(summaryProblems('Tracks snap - always.').length > 0);
  assert.ok(summaryProblems('Tracks snap. Parts snap.').length > 0);
  assert.throws(() => parseChange('x', make({ type: 'bug' })));
  assert.throws(() => parseChange('x', make({ extra: 'y' })));
});

test('a_malformed_legacy_file_is_skipped_not_fatal', () => {
  const dir = repo();
  writeFileSync(join(dir, '.changes/bad.md'), '### not a change file\n');
  writeFileSync(join(dir, '.changes/good.md'), make());
  commit(dir, 'Add (#4)', '2026-11-01T12:00:00Z');
  const warn = console.warn;
  console.warn = () => {};
  try {
    assert.deepEqual(loadEntries(dir).map((e) => e.slug), ['good']);
  } finally {
    console.warn = warn;
  }
});
