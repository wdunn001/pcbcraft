// Content collections: Starlight docs only. Marketing copy is read from
// src/content/copy/*.json by src/lib/copy.ts (see that file for the slot rules).
import { defineCollection } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
};
