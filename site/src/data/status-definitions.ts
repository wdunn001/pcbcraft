// State labels, colors, and descriptions for the status page.
// Single source of truth for how states are presented.

export const STATE_LABELS: Record<string, string> = {
  "planned": "Planned",
  "ready": "Ready",
  "in-progress": "In progress",
  "awaiting-test": "Awaiting test",
  "built": "Built",
  "released": "Released",
  "unlinked": "Unlinked",
};

export const STATE_DESCRIPTIONS: Record<string, string> = {
  "planned": "On the roadmap but not yet started.",
  "ready": "Ready to be worked on; waiting for an assignee.",
  "in-progress": "Currently being developed.",
  "awaiting-test": "Code is complete, waiting for owner testing.",
  "built": "Code merged and CI is green; not yet in a release.",
  "released": "Included in a published release.",
  "unlinked": "Merged but not linked to any issue or feature.",
};

// Color classes for state pills (using Tailwind palette)
export const STATE_COLORS: Record<string, string> = {
  "planned": "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300",
  "ready": "bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300",
  "in-progress": "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300",
  "awaiting-test": "bg-purple-100 text-purple-700 dark:bg-purple-900 dark:text-purple-300",
  "built": "bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300",
  "released": "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300",
  "unlinked": "bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-300",
};

// Maturity levels for features
export const MATURITY_LABELS: Record<string, string> = {
  "tested": "Tested",
  "no-test": "No test",
};

export const MATURITY_DESCRIPTIONS: Record<string, string> = {
  "tested": "Has at least one automated test.",
  "no-test": "No automated test yet; owner should add one.",
};

// Milestone layout groups
export const MILESTONE_GROUPS = {
  now: ["in-progress", "awaiting-test"],
  next: ["ready"],
  later: ["planned"],
};

export const MILESTONE_GROUP_LABELS = {
  now: "Now",
  next: "Next",
  later: "Later",
};

export const MILESTONE_GROUP_DESCRIPTIONS = {
  now: "Work currently in progress or awaiting test.",
  next: "Ready to be picked up; waiting for an assignee.",
  later: "On the roadmap but not yet started.",
};
