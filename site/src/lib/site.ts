// Site-wide constants for the marketing front and the docs header and footer.
// Context: the nav list is the single source for the header on every page (issue #712 item 6).
// Rules: build hrefs with `url()` so `SITE_BASE` is honoured on any host.
// Do not: hard-code a leading-slash path in a component.

/** Prefix a site path with the configured base, e.g. `url('docs/')` -> `/pcbcraft/docs/`. */
export function url(path = ''): string {
  const base = import.meta.env.BASE_URL.endsWith('/') ? import.meta.env.BASE_URL : `${import.meta.env.BASE_URL}/`;
  return `${base}${path.replace(/^\/+/, '')}`;
}

export interface NavItem {
  label: string;
  path: string;
}

/** Header navigation, in order. Same on every page. */
export const NAV: NavItem[] = [
  { label: 'Features', path: 'features/' },
  { label: 'Showcase', path: 'showcase/' },
  { label: 'Download', path: 'download/' },
  { label: 'Try it', path: 'demo/' },
  { label: 'Docs', path: 'docs/' },
  { label: 'Changelog', path: 'changelog/' },
];

/** Secondary pages, shown in the header's More menu. */
export const MORE: NavItem[] = [
  { label: 'About', path: 'about/' },
  { label: 'Brand', path: 'brand/' },
  { label: 'License', path: 'license/' },
];

export const REPO_URL = 'https://github.com/tangency-labs/pcbcraft';

export interface FooterColumn {
  title: string;
  links: { label: string; href: string; external?: boolean }[];
}

/** Footer link columns. External links open in the same tab and say so in the label text. */
export function footerColumns(): FooterColumn[] {
  return [
    {
      title: 'Product',
      links: [
        { label: 'Features', href: url('features/') },
        { label: 'Showcase', href: url('showcase/') },
        { label: 'Download', href: url('download/') },
        { label: 'Try it in the browser', href: url('demo/') },
      ],
    },
    {
      title: 'Documentation',
      links: [
        { label: 'Guide', href: url('docs/guide/getting-started/') },
        { label: 'Reference', href: url('docs/reference/overview/') },
        { label: 'Changelog', href: url('changelog/') },
      ],
    },
    {
      title: 'Project',
      links: [
        { label: 'About', href: url('about/') },
        { label: 'Brand', href: url('brand/') },
        { label: 'License', href: url('license/') },
        { label: 'Source code', href: REPO_URL, external: true },
        { label: 'Issues', href: `${REPO_URL}/issues`, external: true },
      ],
    },
  ];
}
