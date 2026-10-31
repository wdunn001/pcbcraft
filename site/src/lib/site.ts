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
  { label: 'What it does', path: '#features' },
  { label: 'Get PCBCraft', path: '#get' },
];

/** Secondary pages, shown in the header's More menu. Empty until those pages are ready. */
export const MORE: NavItem[] = [];

export const REPO_URL = 'https://github.com/wdunn001/pcbcraft';

/** Site metadata for the status page and other pages. */
export const SITE = {
  name: 'PCBCraft',
  description: 'A free and open-source EDA tool for PCB design',
};

export interface FooterColumn {
  title: string;
  links: { label: string; href: string; external?: boolean }[];
}

/** Footer link columns. External links open in the same tab and say so in the label text. */
export function footerColumns(): FooterColumn[] {
  return [
    {
      title: 'Project',
      links: [
        { label: 'Source code', href: REPO_URL, external: true },
        { label: 'Issues', href: `${REPO_URL}/issues`, external: true },
        { label: 'License', href: url('license/') },
      ],
    },
  ];
}
