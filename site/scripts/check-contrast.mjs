// Contrast gate for the site theme. Context: reads the `--pc-*` tokens from
// src/styles/theme.css (dark block = first `:root`, light block = `:root[data-theme='light']`)
// and checks every pair the site uses against WCAG 2.2 AA: 4.5:1 for text, 3:1 for control
// borders and the focus ring (success criteria 1.4.3 and 1.4.11).
// The formula and the token reader live in src/lib/contrast.js (shared with the brand page).
// Run: `npm run test:contrast` (node --test). The test name is `contrast_tokens_pass_wcag_aa`.
// Do not: add a pair here that the site does not render, or lower a threshold to pass.
import test from 'node:test';
import assert from 'node:assert/strict';

import { tokens, ratio } from '../src/lib/contrast.js';

const grounds = ['bg', 'surface', 'raised'];
const TEXT = [
  ...['text', 'muted', 'copper', 'mask', 'gold', 'cyan', 'red'].flatMap((fg) => grounds.map((bg) => [fg, bg])),
  ['onacc', 'copper'], ['onacc', 'mask'],
  ['text', 'note-low'], ['text', 'tip-low'], ['text', 'caution-low'], ['text', 'danger-low'],
  ['cyan', 'note-low'], ['mask', 'tip-low'], ['gold', 'caution-low'], ['red', 'danger-low'],
];
const NON_TEXT = [
  ['edge', 'bg'], ['edge', 'surface'], ['edge', 'raised'],
  ['gold', 'bg'], ['gold', 'surface'], ['gold', 'raised'],
  ['copper', 'bg'], ['copper', 'surface'],
];

test('contrast_tokens_pass_wcag_aa', () => {
  const failures = [];
  for (const [theme, selector] of [['dark', ':root'], ['light', ":root[data-theme='light']"]]) {
    const t = tokens(selector);
    const check = (pairs, min, kind) => {
      for (const [fg, bg] of pairs) {
        assert.ok(t[fg] && t[bg], `${theme}: token missing for ${fg} or ${bg}`);
        const r = ratio(t[fg], t[bg]);
        if (r < min) failures.push(`${theme} ${kind} ${fg} ${t[fg]} on ${bg} ${t[bg]} = ${r.toFixed(2)} (< ${min})`);
      }
    };
    check(TEXT, 4.5, 'text');
    check(NON_TEXT, 3, 'control');
  }
  assert.deepEqual(failures, [], `contrast failures:\n${failures.join('\n')}`);
});
