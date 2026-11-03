// Prints the release notes entries as Markdown. Context: the release workflow (cd.yml) puts the
// output into the notes of the rolling main-latest release, from the same change files the
// /changelog page lists. Run: `node scripts/release-notes.mjs [--since <commit>]` at any depth.
// Rules: with --since, only entries added after that commit are printed.
// Do not: call GitHub from here; the workflow passes the previous commit in.
import { fileURLToPath } from 'node:url';
import { loadEntries, renderNotes } from '../src/lib/changelog-core.js';

const root = fileURLToPath(new URL('../../', import.meta.url));
const i = process.argv.indexOf('--since');
const since = i > 0 ? (process.argv[i + 1] ?? '') : '';
process.stdout.write(renderNotes(loadEntries(root, { since }), 'https://pcbcraft.net/changelog/'));
