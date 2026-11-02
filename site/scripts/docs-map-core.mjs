// Docs mapping rules. Context: the website docs section is built from the pages under docs/.
// docs-map.json maps each page to a section or lists it as excluded with a reason.
// Rules: every page under docs/ must be in exactly one list, every entry must exist on disk,
// every excluded entry needs a reason of at least 20 characters, and slugs must be unique.
// Do not: read the file system here. Callers pass the page list so tests need no fixtures.

/** Returns a list of problem strings. An empty list means the map is complete. */
export function checkMap(files, map) {
  const problems = [];
  const mapped = Object.keys(map.pages ?? {});
  const excluded = Object.keys(map.excluded ?? {});
  const known = new Set(files);
  for (const f of files) {
    const inPages = mapped.includes(f);
    const inExcluded = excluded.includes(f);
    if (!inPages && !inExcluded) problems.push(`docs/${f} is neither mapped nor excluded in docs-map.json`);
    if (inPages && inExcluded) problems.push(`docs/${f} is both mapped and excluded in docs-map.json`);
  }
  for (const f of [...mapped, ...excluded]) {
    if (!known.has(f)) problems.push(`docs-map.json lists docs/${f}, which does not exist`);
  }
  for (const [f, reason] of Object.entries(map.excluded ?? {})) {
    if (typeof reason !== 'string' || reason.trim().length < 20) {
      problems.push(`docs/${f} is excluded without a reason of at least 20 characters`);
    }
  }
  const seen = new Map();
  for (const [f, entry] of Object.entries(map.pages ?? {})) {
    if (!map.sections?.[entry.section]) problems.push(`docs/${f} uses the unknown section "${entry.section}"`);
    const slug = pageSlug(f, entry);
    const key = `${entry.section}/${slug}`;
    if (seen.has(key)) problems.push(`docs/${f} and docs/${seen.get(key)} both publish as ${key}`);
    seen.set(key, f);
  }
  return problems;
}

/** The URL slug of a mapped page: the entry's slug, else the lowercase file name. */
export function pageSlug(file, entry) {
  if (entry.slug) return entry.slug;
  return file.split('/').pop().replace(/\.md$/, '').toLowerCase();
}
