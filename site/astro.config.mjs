// Astro + Starlight config. Context: static site for PCBCraft, marketing front at `/`
// (src/pages), documentation under `/docs/` (src/content/docs/docs).
// Rules: `site` and `base` come from SITE_URL and SITE_BASE so one dist/ fits any host.
// Defaults are the owner's choice: https://pcbcraft.net served from the owner's server at /.
// Both stay overridable (a project Pages host needs SITE_BASE=/pcbcraft/).
// Every internal link goes through url() in src/lib/site.ts so the base is always honoured.
// Do not add src/content/docs/index.mdx: it would take over `/` from the marketing page.
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const site = process.env.SITE_URL || 'https://pcbcraft.net';
const rawBase = process.env.SITE_BASE || '/';
const base = rawBase === '/' ? '/' : `/${rawBase.replace(/^\/+|\/+$/g, '')}/`;

export default defineConfig({
  site,
  base,
  output: 'static',
  trailingSlash: 'always',
  // The live host sends script-src 'self': no inline scripts. Never inline bundled scripts.
  build: { inlineStylesheets: 'never' },
  vite: { build: { assetsInlineLimit: 0 } },
  integrations: [
    starlight({
      title: 'PCBCraft',
      description: 'Documentation for PCBCraft, a schematic and board layout tool.',
      favicon: '/favicon.svg',
      defaultLocale: 'root',
      locales: { root: { label: 'English', lang: 'en' } },
      customCss: ['./src/styles/fonts.css', './src/styles/theme.css', './src/styles/docs.css'],
      components: {
        Header: './src/components/starlight/Header.astro',
        Footer: './src/components/starlight/Footer.astro',
        ThemeProvider: './src/components/starlight/ThemeProvider.astro',
        ThemeSelect: './src/components/starlight/ThemeSelect.astro',
      },
      sidebar: [
        { label: 'Guide', items: [{ autogenerate: { directory: 'docs/guide' } }] },
        { label: 'Reference', items: [{ autogenerate: { directory: 'docs/reference' } }] },
      ],
    }),
  ],
});
