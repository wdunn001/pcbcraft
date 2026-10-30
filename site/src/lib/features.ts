// Typed access to the feature list for pages. Validation lives in ./features-core.js and
// runs on every build, so a feature without evidence stops the build here.
import raw from '../data/features.yaml?raw';
import { fileURLToPath } from 'node:url';
import { AREAS, STATUSES, parseFeatures } from './features-core.js';

export type StatusId = keyof typeof STATUSES;
export interface Feature {
  area: string;
  name: string;
  summary: string;
  status: StatusId;
  key?: boolean;
  evidence: string[];
}
export interface Area {
  id: string;
  label: string;
  tone: 'sch' | 'pcb' | 'lib' | 'out';
}

const dataDir = fileURLToPath(new URL('../data/', import.meta.url));
const repoRoot = fileURLToPath(new URL('../../../', import.meta.url));

export const areas = AREAS as Area[];
export const statusLabels = STATUSES as Record<StatusId, string>;
export const features = parseFeatures(raw, { dataDir, repoRoot }) as Feature[];

/** Entries of one area in file order. */
export const inArea = (id: string): Feature[] => features.filter((f) => f.area === id);
/** The short list shown on the home page: entries flagged `key`. */
export const keyPoints = (id: string): Feature[] => inArea(id).filter((f) => f.key);
