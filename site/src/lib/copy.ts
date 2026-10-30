// Marketing copy access. Context: text lives in src/content/copy/*.json, never in pages.
// Rules: an entry is `{ text, placeholder }`; `slot()` returns the attributes that draw the dashed
// "COPY #718" mark, so a placeholder can never pass as final copy. The mark is switched off site
// wide only by flipping every entry's `placeholder` to false (done by #718), never by CSS.
// Do not: write a sentence in a page; add it to the copy file.
import home from '../content/copy/home.json';

export interface Copy {
  text: string;
  placeholder: boolean;
}
export { home };

/** Attributes for an element that shows placeholder copy; empty for final copy. */
export function slot(entry: Copy, issue = '#718'): Record<string, string> {
  return entry.placeholder ? { 'data-slot': `COPY ${issue}` } : {};
}
