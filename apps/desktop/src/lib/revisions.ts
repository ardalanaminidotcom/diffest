/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */

interface Worktree {
  path: string;
  branch: string | null;
}

interface Project {
  id: number;
  name: string;
  mainCheckout: string;
  worktrees: Worktree[];
  worktreeError: string | null;
}

interface Registration {
  project: Project;
  checkout: string;
}

interface Commit {
  id: string;
  subject: string;
}

interface Revisions {
  branches: string[];
  commits: Commit[];
}

interface ChangedFile {
  status: string;
  path: string;
  previousPath: string | null;
  patch: string;
}

interface Comparison {
  start: string;
  end: string;
  files: ChangedFile[];
}

type ComparisonMode = "between" | "divergence";

export type { Comparison, ComparisonMode, Project, Registration, Revisions };
