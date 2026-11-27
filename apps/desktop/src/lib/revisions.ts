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
  savedReviews: CommitReviewSummary[];
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

interface CommitReview extends CommitReviewProgress {
  comparison: Comparison;
}

type DiffLayout = "unified" | "split";

type ComparisonMode = "between" | "divergence";

export type {
  ChangedFile,
  CommitReview,
  CommitReviewProgress,
  Comparison,
  ComparisonMode,
  DiffLayout,
  Project,
  Registration,
  Revisions,
  StageWorktreePatchResult,
  WorktreePatch,
};
