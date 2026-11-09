// Mapping gate for the website docs. Test names: a_page_missing_from_the_map_fails_the_build.
// Run: `npm run test:docs-map`. The same check runs before every dev, check and build command.
import test from 'node:test';
import assert from 'node:assert/strict';
import { checkMap, isExcluded } from './docs-map-core.mjs';
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

test('a_pattern_excludes_every_matching_page_without_a_line_each', () => {
  const withPattern = { ...map, excluded: {}, excludedPatterns: { 'adr-*.md': 'Decision records stay out until the work ships.' } };
  assert.deepEqual(checkMap(['a.md', 'adr-0001-x.md', 'adr-0002-y.md'], withPattern), []);
  assert.equal(isExcluded(withPattern, 'adr-0007-z.md'), true);
  assert.equal(isExcluded(withPattern, 'a.md'), false);
});

test('a_pattern_that_matches_nothing_fails_the_build', () => {
  const stale = { ...map, excluded: {}, excludedPatterns: { 'adr-*.md': 'Decision records stay out until the work ships.' } };
  assert.match(checkMap(['a.md'], stale).join('\n'), /matches no page/);
});

test('a_pattern_without_a_reason_fails_the_build', () => {
  const bad = { ...map, excluded: {}, excludedPatterns: { 'adr-*.md': 'later' } };
  assert.match(checkMap(['a.md', 'adr-1.md'], bad).join('\n'), /without a reason/);
});
