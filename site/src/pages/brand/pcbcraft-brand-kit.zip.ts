// The brand kit zip, generated at build time from src/assets/brand/ and the theme tokens.
// Context: linked from /brand/. Contents: logo SVGs, logo PNGs at several sizes, the colour tokens
// as CSS, JSON and a GIMP/Inkscape .gpl palette, and a short README with the usage terms.
// Rules: nothing here is hand assembled; changing a file in assets/brand/ or a token in
// theme.css changes the next build's zip. The mtime is fixed so the zip is reproducible.
// Do not: add files that are not in assets/brand/ or derived from theme.css.
import { zipSync, strToU8 } from 'fflate';
import sharp from 'sharp';
import mark from '../../assets/brand/concept-2.svg?raw';
import mark16 from '../../assets/brand/concept-2-16.svg?raw';
import mono from '../../assets/brand/concept-2-mono.svg?raw';
import css from '../../styles/theme.css?raw';
import { tokenMap } from '../../lib/brand';

export async function GET() {
  const png = async (svg: string, size: number) => new Uint8Array(await sharp(Buffer.from(svg)).resize(size, size).png().toBuffer());
  const files: Record<string, Uint8Array> = {
    'logo/pcbcraft-mark.svg': strToU8(mark),
    'logo/pcbcraft-mark-16px.svg': strToU8(mark16),
    'logo/pcbcraft-mark-one-colour.svg': strToU8(mono),
    'logo/pcbcraft-mark-1024.png': await png(mark, 1024),
    'logo/pcbcraft-mark-512.png': await png(mark, 512),
    'logo/pcbcraft-mark-256.png': await png(mark, 256),
    'logo/pcbcraft-mark-128.png': await png(mark, 128),
    'logo/pcbcraft-mark-64.png': await png(mark, 64),
    'logo/pcbcraft-mark-32.png': await png(mark16, 32),
    'logo/pcbcraft-mark-16.png': await png(mark16, 16),
  };

  const rootBlock = css.slice(css.indexOf(":root,\n[data-theme-scope='dark']"), css.indexOf('/* Starlight mapping'));
  files['colour/tokens.css'] = strToU8(`/* PCBCraft colour tokens, dark and light. Generated from site/src/styles/theme.css. */\n${rootBlock}`);
  files['colour/tokens.json'] = strToU8(`${JSON.stringify(tokenMap, null, 2)}\n`);
  const gpl = ['GIMP Palette', 'Name: PCBCraft', 'Columns: 4', '#'];
  for (const theme of ['dark', 'light'] as const) {
    for (const [id, hex] of Object.entries(tokenMap[theme])) {
      const n = (i: number) => String(parseInt(hex.slice(i, i + 2), 16)).padStart(3, ' ');
      gpl.push(`${n(1)} ${n(3)} ${n(5)} ${theme} ${id} ${hex}`);
    }
  }
  files['colour/pcbcraft.gpl'] = strToU8(`${gpl.join('\n')}\n`);
  files['README.txt'] = strToU8(
    [
      'PCBCraft brand kit',
      '',
      'logo/    The Layer stack mark as SVG and PNG, the 16 px variant and a one-colour version.',
      'colour/  Colour tokens for dark and light as CSS, JSON and a .gpl palette.',
      '',
      'Type: IBM Plex Sans Condensed (headings), IBM Plex Sans (body) and JetBrains Mono (code).',
      'All three are licensed under the SIL Open Font License 1.1 and are not in this zip.',
      '',
      'Terms: the PCBCraft name and logo are reserved. The code is GPL-3.0. Docs and screenshots',
      'are CC BY-SA 4.0. The full acceptable-use wording is on the License page of the site.',
      '',
    ].join('\n'),
  );

  const entries = Object.fromEntries(Object.entries(files).map(([k, v]) => [k, [v, { mtime: new Date('2026-01-01T00:00:00Z') }]]));
  const zip = zipSync(entries as Parameters<typeof zipSync>[0]);
  return new Response(new Blob([zip as BlobPart]), { headers: { 'Content-Type': 'application/zip' } });
}
