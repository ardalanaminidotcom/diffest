/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */

interface Worktree {
  path: string;
  branch: string | null;
}

interface Project {
  id: number;
  name: string;
  mainCheckout: string | null;
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
  remoteBranches: string[];
  syncedRemoteBranches?: string[];
  tags?: string[];
  defaultBranch: string | null;
  currentBranch: string | null;
  commits: Commit[];
}

interface ChangedFile {
  status: string;
  path: string;
  previousPath: string | null;
  pathBytes?: number[];
  previousPathBytes?: number[];
  patch: string;
}

interface WorktreePatch extends ChangedFile {
  side: "staged" | "unstaged";
  fingerprint: string;
  reviewed: boolean;
}

interface StageWorktreePatchResult {
  patches: WorktreePatch[];
  error: string | null;
}

interface Comparison {
  baseLabel: string;
  targetLabel: string;
  mode: ComparisonMode;
  base: string;
  start: string;
  end: string;
  files: ChangedFile[];
}

interface CommitReviewSummary {
  id: number;
  baseLabel: string;
  targetLabel: string;
  start: string;
  end: string;
  fileCount: number;
  reviewedCount: number;
}

interface CommitReviewProgress {
  summary: CommitReviewSummary;
  reviewedFiles: number[];
}

interface ReviewSelection {
  base: string;
  target: string;
  mode: ComparisonMode;
}

type ComparisonUpdate = "current" | "available" | "branch_deleted";

interface ComparisonSession extends Comparison {
  progress: CommitReviewProgress | null;
  selection: ReviewSelection;
  update: ComparisonUpdate;
  layout: DiffLayout | null;
}

interface ReviewRetention {
  days: number;
  overrideDays: number | null;
}

type DiffLayout = "unified" | "split";

type ComparisonMode = "between" | "divergence";

export type {
  ChangedFile,
  CommitReviewProgress,
  Comparison,
  ComparisonMode,
  ComparisonSession,
  ComparisonUpdate,
  DiffLayout,
  Project,
  Registration,
  ReviewRetention,
  ReviewSelection,
  Revisions,
  StageWorktreePatchResult,
  WorktreePatch,
};
