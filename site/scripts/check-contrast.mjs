// Contrast gate for the site theme. Context: reads the `--pc-*` tokens from
// src/styles/theme.css (dark block = first `:root`, light block = `:root[data-theme='light']`)
// and checks every pair the site uses against WCAG 2.2 AA: 4.5:1 for text, 3:1 for control
// borders and the focus ring (success criteria 1.4.3 and 1.4.11).
// Run: `npm run test:contrast` (node --test). The test name is `contrast_tokens_pass_wcag_aa`.
// Do not: add a pair here that the site does not render, or lower a threshold to pass.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const css = readFileSync(fileURLToPath(new URL('../src/styles/theme.css', import.meta.url)), 'utf8');

/** Token map of the first rule block whose selector starts with `selector`. */
function tokens(selector) {
  const start = css.indexOf(`${selector} {`);
  assert.ok(start >= 0, `selector ${selector} not found in theme.css`);
  const end = css.indexOf('}', start);
  const map = {};
  for (const m of css.slice(start, end).matchAll(/--pc-([a-z-]+):\s*(#[0-9A-Fa-f]{6})\s*;/g)) {
    map[m[1]] = m[2];
  }
  return map;
}

function lum(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function ratio(a, b) {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

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
