// Brand page data: token roles and the pairs shown with their contrast ratios.
// Context: hex values come from src/styles/theme.css through src/lib/contrast.js at build time;
// this file holds only names, roles and which pairs to show.
// Rules: building the page throws when a shown text pair is below 4.5:1 or a control pair is
// below 3:1, so the guide can never display a failing pair.
// Do not: type a colour value here.
import { THEMES, ratio } from './contrast.js';

export type ThemeId = 'dark' | 'light';
export interface TokenRow {
  id: string;
  name: string;
  role: string;
}

export const TOKEN_ROWS: TokenRow[] = [
  { id: 'bg', name: 'Mask', role: 'Page ground. Solder-mask green-black in dark, paper in light.' },
  { id: 'surface', name: 'Surface', role: 'Cards, tables and panels.' },
  { id: 'raised', name: 'Raised', role: 'Table heads, code blocks, empty slots.' },
  { id: 'line', name: 'Line', role: 'Decorative rules only. Never the only border of a control.' },
  { id: 'edge', name: 'Edge', role: 'Borders of inputs and buttons.' },
  { id: 'text', name: 'Silk', role: 'Body text. Silkscreen white in dark.' },
  { id: 'muted', name: 'Muted', role: 'Secondary text.' },
  { id: 'copper', name: 'Copper', role: 'Primary accent: buttons, heading stubs, top copper.' },
  { id: 'mask', name: 'Green', role: 'Libraries, success, copper pour.' },
  { id: 'gold', name: 'Gold', role: 'Focus ring, warnings, outputs, the board edge.' },
  { id: 'cyan', name: 'Link', role: 'Links, schematic, bottom copper.' },
  { id: 'red', name: 'Error', role: 'Errors and danger notes.' },
  { id: 'onacc', name: 'On accent', role: 'Text on copper and green buttons.' },
];

/** Text tokens shown against each ground. */
export const TEXT_TOKENS = ['text', 'muted', 'copper', 'mask', 'gold', 'cyan', 'red'];
export const GROUNDS = ['bg', 'surface', 'raised'];

export interface Ratio {
  value: number;
  pass: boolean;
}

/** Ratio of a text token on a ground in one theme, with the AA verdict (4.5:1). */
export function textRatio(theme: ThemeId, fg: string, bg: string): Ratio {
  const t = THEMES[theme] as Record<string, string>;
  const value = ratio(t[fg], t[bg]);
  return { value, pass: value >= 4.5 };
}

/** Ratio of a control border on a ground (3:1). */
export function controlRatio(theme: ThemeId, fg: string, bg: string): Ratio {
  const t = THEMES[theme] as Record<string, string>;
  const value = ratio(t[fg], t[bg]);
  return { value, pass: value >= 3 };
}

/** Throws one error listing every shown pair that fails AA. Called while the page builds. */
export function assertBrandPairs(): void {
  const bad: string[] = [];
  for (const theme of ['dark', 'light'] as ThemeId[]) {
    for (const fg of TEXT_TOKENS) for (const bg of GROUNDS) {
      const r = textRatio(theme, fg, bg);
      if (!r.pass) bad.push(`${theme} ${fg} on ${bg} ${r.value.toFixed(2)}`);
    }
    for (const bg of ['bg', 'surface']) {
      const r = controlRatio(theme, 'edge', bg);
      if (!r.pass) bad.push(`${theme} edge on ${bg} ${r.value.toFixed(2)} (control)`);
    }
  }
  if (bad.length) throw new Error(`brand page: pairs below WCAG AA:\n  ${bad.join('\n  ')}`);
}

export const tokenMap = THEMES as Record<ThemeId, Record<string, string>>;
