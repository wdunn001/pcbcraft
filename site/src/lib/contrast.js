// WCAG 2.2 contrast arithmetic and the token reader for src/styles/theme.css.
// Context: shared by scripts/check-contrast.mjs (the gate) and the brand page (swatches), so the
// numbers on the page and the gate always come from the same formula and the same file.
// Rules: tokens are read from the file at build time; nothing here holds a colour value.
// Do not: copy a hex value out of theme.css into another file.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

// Relative to the site root (npm runs every script and the build from site/), because the
// bundled build moves this module and import.meta.url no longer points at src/.
const css = readFileSync(join(process.cwd(), 'src', 'styles', 'theme.css'), 'utf8');

/** Token map (`bg`, `copper`, ...) of the rule whose selector list contains `selector`. */
export function tokens(selector) {
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (!m[1].split(',').some((s) => s.trim().endsWith(selector))) continue;
    const map = {};
    for (const t of m[2].matchAll(/--pc-([a-z-]+):\s*(#[0-9A-Fa-f]{6})\s*;/g)) map[t[1]] = t[2].toUpperCase();
    if (Object.keys(map).length) return map;
  }
  throw new Error(`selector ${selector} not found in theme.css`);
}

export const THEMES = { dark: tokens(':root'), light: tokens(":root[data-theme='light']") };

export function lum(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function ratio(a, b) {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}
