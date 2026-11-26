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

export type { Categorization, CategoryRule, FileCategory, RuleSettings };
