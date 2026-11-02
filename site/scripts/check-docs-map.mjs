// Mapping gate for the website docs. Test names: a_page_missing_from_the_map_fails_the_build.
// Run: `npm run test:docs-map`. The same check runs before every dev, check and build command.
import test from 'node:test';
import assert from 'node:assert/strict';
import { checkMap } from './docs-map-core.mjs';
import { listDocs, loadMap } from './sync-docs.mjs';

const map = {
  sections: { use: 'Using' },
  pages: { 'a.md': { section: 'use' } },
  excluded: { 'b.md': 'Kept out until the feature ships.' },
};

test('the_repository_docs_are_fully_mapped', () => {
  assert.deepEqual(checkMap(listDocs(), loadMap()), []);
});

test('a_page_missing_from_the_map_fails_the_build', () => {
  const problems = checkMap(['a.md', 'b.md', 'new.md'], map);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /new\.md is neither mapped nor excluded/);
});

test('an_exclusion_without_a_reason_fails_the_build', () => {
  const bad = { ...map, excluded: { 'b.md': 'later' } };
  assert.match(checkMap(['a.md', 'b.md'], bad).join('\n'), /without a reason/);
});

test('an_entry_for_a_missing_page_fails_the_build', () => {
  assert.match(checkMap(['a.md'], map).join('\n'), /b\.md, which does not exist/);
});

test('two_pages_with_one_address_fail_the_build', () => {
  const dup = { ...map, pages: { 'a.md': { section: 'use' }, 'x/a.md': { section: 'use' } }, excluded: {} };
  assert.match(checkMap(['a.md', 'x/a.md'], dup).join('\n'), /both publish as use\/a/);
});
