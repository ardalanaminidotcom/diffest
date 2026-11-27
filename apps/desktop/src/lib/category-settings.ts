/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */

import type { DiffLayout } from "./revisions.js";

interface FileCategory {
  category: string;
  subcategory: string | null;
}

interface CategoryRule extends FileCategory {
  glob: string;
}

interface RuleSettings {
  path: string;
  version: string | null;
  error: string | null;
  custom: boolean;
  rules: CategoryRule[];
}

interface Categorization {
  defaultDiffLayout: DiffLayout;
  global: RuleSettings;
  project: RuleSettings | null;
  categories: FileCategory[];
}

type FileGroups = Map<string, Map<string | null, number[]>>;

function groupFiles(paths: string[], categories: FileCategory[]): FileGroups {
  const groups: FileGroups = new Map();
  for (const [index] of paths.entries()) {
    const category = categories[index]?.category ?? "Other";
    const subcategory = categories[index]?.subcategory ?? null;
    const subgroups = groups.get(category) ?? new Map<string | null, number[]>();
    const files = subgroups.get(subcategory) ?? [];
    files.push(index);
    subgroups.set(subcategory, files);
    groups.set(category, subgroups);
  }
  return groups;
}

export { groupFiles };
export type { Categorization, CategoryRule, FileCategory, FileGroups, RuleSettings };
