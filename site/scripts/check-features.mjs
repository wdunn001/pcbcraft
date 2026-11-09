// Gate for src/data/features.yaml. Test name: every_feature_has_evidence.
// Run: `npm run test:features`. Same rules the build applies (src/lib/features-core.js).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parseFeatures } from '../src/lib/features-core.js';

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const yaml = readFileSync(here('../src/data/features.yaml'), 'utf8');
const opts = { dataDir: here('../src/data/'), repoRoot: here('../../') };

test('every_feature_has_evidence', () => {
  assert.ok(parseFeatures(yaml, opts).length > 0);
});

test('a_feature_without_evidence_fails_the_build', () => {
  const bad = `- {area: schematic, name: X, summary: Y, status: browser, evidence: []}\n`;
  assert.throws(() => parseFeatures(bad, opts), /no evidence/);
});

test('a_feature_that_names_another_tool_fails', () => {
  const bad = yaml + `- {area: schematic, name: Import from KiCad, summary: Y, status: browser, evidence: ["test:x"]}\n`;
  assert.throws(() => parseFeatures(bad, opts), /names another tool/);
});
