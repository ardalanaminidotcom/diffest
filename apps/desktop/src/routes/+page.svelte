<script lang="ts">
import type {
  CommitReviewProgress,
  Comparison,
  ComparisonMode,
  ComparisonSession,
  ComparisonUpdate,
  DiffLayout,
  Project,
  Registration,
  ReviewSelection,
  Revisions,
  StageWorktreePatchResult,
  WorktreePatch,
} from "../lib/revisions.js";
import { invoke } from "@tauri-apps/api/core";
import { onMount, tick } from "svelte";
import CategorizedFileTree from "../lib/categorized-file-tree.svelte";
import {
  type Categorization,
  type CategoryRuleDraft,
  type FileCategory,
  groupFiles,
} from "../lib/category-settings.js";
import CategorySettings from "../lib/category-settings.svelte";
import { fileName } from "../lib/file-details.js";
import { summarizePatch } from "../lib/patch.js";
import ReviewFile from "../lib/review-file.svelte";
import RevisionPicker from "../lib/revision-picker.svelte";

const statusLabels: Record<string, string> = {
  A: "Added",
  D: "Deleted",
  M: "Modified",
  R: "Renamed",
  T: "Type changed",
  U: "Conflicted",
};

/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
type OpenReview
  = | {
    kind: "comparison";
    comparison: Comparison;
    progress: CommitReviewProgress | null;
    selection: ReviewSelection;
  }
  | {
    kind: "worktree";
    patches: WorktreePatch[];
  };

interface ComparisonOptions {
  mode: ComparisonMode;
  refresh?: ReviewSelection | null;
}

interface CategorizedFiles {
  checkout: string | null;
  paths: string[];
  categories: FileCategory[];
}

interface ReviewPosition {
  file: HTMLElement;
  offset: number;
  selectedIndex: number;
}
/* eslint-enable @stylistic/key-spacing */

function samePaths(first: string[], second: string[]): boolean {
  return first.length === second.length && first.every((path, index) => path === second[index]);
}

let projects = $state<Project[]>([]);
let activeCheckout = $state("");
let activeProjectId = $state<number>();
let revisions = $state<Revisions>();
let base = $state("");
let target = $state("HEAD");
let fetchStatus = $state("");
let mode = $state<ComparisonMode>("between");
let openReviewState = $state<OpenReview | null>(null);
let comparison = $derived(openReviewState?.kind === "comparison" ? openReviewState.comparison : null);
let review = $derived(openReviewState?.kind === "comparison" ? openReviewState.progress : null);
let worktreeReview = $derived(openReviewState?.kind === "worktree" ? openReviewState.patches : null);
let reviewedFiles = $derived(new Set(review?.reviewedFiles));
let defaultLayout = $state<DiffLayout>("unified");
let layoutOverride = $state<DiffLayout | null>(null);
let layout = $derived(layoutOverride ?? defaultLayout);
let jumpedFileIndex = $state<number | null>(null);
let reviewSession = $state(0);
let files = $derived(worktreeReview ?? comparison?.files ?? []);
let mainCheckout = $derived(projects.find(project => project.id === activeProjectId)?.mainCheckout ?? null);
let paths = $derived(files.map(file => file.path));
let categorizedFiles = $state.raw<CategorizedFiles>();
const categories = $derived(categorizedFiles?.checkout === mainCheckout && samePaths(categorizedFiles.paths, paths)
  ? categorizedFiles.categories
  : null);
const groups = $derived(groupFiles(paths, categories ?? []));
const orderedIndices = $derived([...groups.values()].flatMap(subgroups => [...subgroups.values()].flat()));
const selectedIndex = $derived(jumpedFileIndex ?? orderedIndices[0] ?? 0);
const selectedWorktreePatch = $derived(worktreeReview?.[selectedIndex]);
let error = $state("");
let busy = $state(false);
let comparisonUpdate = $state<ComparisonUpdate>("current");
const comparisonUpdateInterval = 5000;
let checkingUpdate = false;
let sourceOpen = $state(false);
let filesCollapsed = $state(false);
const filesAction = $derived(filesCollapsed ? "Open Files sidebar" : "Collapse Files sidebar");
let projectsCollapsed = $state(false);
const projectsAction = $derived(projectsCollapsed ? "Open Projects sidebar" : "Collapse Projects sidebar");
let settingsOpen = $state(false);
const drafts: Record<string, CategoryRuleDraft | null> = {};
let settingsAnnouncement = $state("");
let settingsButton: HTMLButtonElement | undefined = $state();
let reviewPosition: ReviewPosition | null = null;
let pendingSettingsRead = 0;
let categorizedCheckout: string | null | undefined;
let categorizedPaths: string[] = [];
let projectSidebar: HTMLElement | undefined = $state();
let fileStream: HTMLElement | undefined = $state();
const activeProject = $derived(projects.find(project => project.id === activeProjectId));
const activeBranch = $derived(activeProject?.worktrees.find(worktree => worktree.path === activeCheckout)?.branch);
const patchSummaries = $derived(files.map(file => summarizePatch(file.patch)));
const lineTotals = $derived(patchSummaries.reduce((totals, patch) => ({
  added  : totals.added + patch.added,
  removed: totals.removed + patch.removed,
}), {
  added  : 0,
  removed: 0,
}));
const mountedRowLimit = 20_000;
const windowRows = $derived(patchSummaries.reduce((rows, patch) => rows + patch.lineCount, 0) > mountedRowLimit);
const reviewedWorktreeCount = $derived(worktreeReview?.filter(patch => patch.reviewed).length);
const reviewedFileCount = $derived(reviewedWorktreeCount ?? review?.summary.reviewedCount ?? 0);

function toggleProjects(): void {
  projectsCollapsed = !projectsCollapsed;
}

async function loadReviewSettings(checkout: string | null, filePaths: string[]): Promise<void> {
  const request = ++pendingSettingsRead;
  try {
    const settings = await invoke<Categorization>("read_category_settings", {
      mainCheckout: checkout,
      paths       : filePaths,
    });
    if (request !== pendingSettingsRead) return;

    categorizedFiles = {
      checkout,
      paths     : filePaths,
      categories: settings.categories,
    };
    defaultLayout = settings.defaultDiffLayout ?? "unified";
    settingsAnnouncement = [settings.global.error, settings.project?.error].filter(Boolean).join(" ");
  } catch (error_) {
    if (request === pendingSettingsRead) {
      categorizedFiles = {
        checkout,
        paths     : filePaths,
        categories: categories ?? [],
      };
      settingsAnnouncement = String(error_);
    }
  }
}

function refreshReviewSettings(): Promise<void> {
  return loadReviewSettings(mainCheckout, paths);
}

$effect(() => {
  const checkout = mainCheckout;
  const filePaths = paths;
  if (categorizedCheckout === checkout && samePaths(categorizedPaths, filePaths)) return;

  categorizedCheckout = checkout;
  categorizedPaths = filePaths;
  void loadReviewSettings(checkout, filePaths);
});

function openSettings(): void {
  settingsOpen = true;
  if (!fileStream) return;

  const top = fileStream.getBoundingClientRect().top;
  const file = [...fileStream.querySelectorAll<HTMLElement>(".review-file")]
    .find(section => section.getBoundingClientRect().bottom > top);
  if (!file) return;

  reviewPosition = {
    file,
    offset: file.getBoundingClientRect().top - top,
    selectedIndex,
  };
}

async function closeSettings(): Promise<void> {
  settingsOpen = false;
  await tick();
  if (reviewPosition && fileStream?.contains(reviewPosition.file)) {
    const offset = reviewPosition.file.getBoundingClientRect().top - fileStream.getBoundingClientRect().top;
    if (offset !== reviewPosition.offset) fileStream.scrollTop += offset - reviewPosition.offset;
    if (selectedIndex !== reviewPosition.selectedIndex) jumpedFileIndex = reviewPosition.selectedIndex;
  }
  reviewPosition = null;
  settingsButton?.focus({ preventScroll: true });
}

function startFileStream(): void {
  jumpedFileIndex = null;
  reviewSession++;
}

function jumpToFile(index: number): void {
  jumpedFileIndex = index;
  const file = fileStream?.querySelector<HTMLElement>(`#file-review-${ index }`);
  file?.querySelector<HTMLButtonElement>("h2 button")?.focus({ preventScroll: true });
  file?.scrollIntoView({ block: "start" });
}

async function run(action: () => Promise<void>): Promise<void> {
  if (busy) return;

  busy = true;
  error = "";
  try {
    await action();
  } catch (error_) {
    error = String(error_);
  } finally {
    busy = false;
  }
}

async function checkComparisonUpdate(): Promise<void> {
  const current = openReviewState;
  const session = reviewSession;
  if (checkingUpdate || current?.kind !== "comparison" || comparisonUpdate === "branch_deleted") return;

  checkingUpdate = true;
  try {
    const status = await invoke<ComparisonUpdate>("comparison_update", {
      checkout : activeCheckout,
      selection: current.selection,
      base     : current.comparison.base,
      end      : current.comparison.end,
    });
    if (session === reviewSession) comparisonUpdate = status;
  } catch {
    // Keep the last known state when Git is temporarily unavailable.
  } finally {
    checkingUpdate = false;
  }
}

onMount(() => {
  void run(async () => {
    projects = await invoke<Project[]>("list_projects");
  });
  const timer = globalThis.setInterval(() => void checkComparisonUpdate(), comparisonUpdateInterval);
  return () => globalThis.clearInterval(timer);
});

async function leaveComparison(): Promise<void> {
  if (openReviewState?.kind === "comparison") await invoke("leave_comparison_review");
  comparisonUpdate = "current";
}

function readReviewLayout(path: string, id: number | null): Promise<DiffLayout | null> {
  return invoke("read_review_layout", {
    checkout: path,
    id,
  });
}

async function revealProject(id: number): Promise<void> {
  await tick();
  const group = projectSidebar?.querySelector<HTMLDetailsElement>(`#project-${ id }`);
  if (group) group.open = true;
}

function showWorktreeReview(patches: WorktreePatch[], side = selectedWorktreePatch?.side): void {
  const selected = selectedWorktreePatch;
  const pathBytes = selected?.pathBytes?.join(",");
  const index = patches.findIndex((patch) => {
    const samePath = patch.path === selected?.path && patch.pathBytes?.join(",") === pathBytes;
    return patch.side === side && samePath;
  });
  if (jumpedFileIndex !== null) jumpedFileIndex = index === -1 ? null : index;
  openReviewState = {
    kind: "worktree",
    patches,
  };
}

async function displayComparison(
  source: string,
  destination: string,
  { mode: comparisonMode, refresh }: ComparisonOptions,
): Promise<void> {
  if (!source) {
    await leaveComparison();
    layoutOverride = null;
    openReviewState = null;
  } else if (!refresh && destination === "Working tree" && (source === "HEAD" || source === revisions?.currentBranch)) {
    const patches = await invoke<WorktreePatch[]>("open_worktree_review", { checkout: activeCheckout });
    const override = await readReviewLayout(activeCheckout, null);
    await leaveComparison();
    layoutOverride = override;
    showWorktreeReview(patches);
  } else {
    const { progress, selection, update, layout: override, ...displayed }
      = await invoke<ComparisonSession>("compare_revisions", {
        checkout: activeCheckout,
        base    : refresh?.base ?? source,
        target  : refresh?.target ?? destination,
        mode    : comparisonMode,
        ...refresh ? { refresh: true } : {},
      });
    layoutOverride = override ?? (refresh ? layoutOverride : null);
    openReviewState = {
      kind      : "comparison",
      comparison: displayed,
      progress  : progress ?? null,
      selection,
    };
    comparisonUpdate = update;
  }
  base = source;
  target = destination;
  mode = comparisonMode;
  filesCollapsed = false;
  startFileStream();
}

function compare(source: string, destination: string, comparisonMode: ComparisonMode): void {
  void run(() => displayComparison(source, destination, { mode: comparisonMode }));
}

async function openWorktree(path: string, projectId: number): Promise<void> {
  const nextRevisions = await invoke<Revisions>("open_worktree", { checkout: path });
  await leaveComparison();
  layoutOverride = null;
  filesCollapsed = false;
  revisions = nextRevisions;
  activeCheckout = path;
  fetchStatus = "";
  activeProjectId = projectId;
  base = "";
  target = "HEAD";
  mode = "between";
  sourceOpen = false;
  openReviewState = null;
  startFileStream();

  const { defaultBranch, currentBranch } = nextRevisions;
  if (defaultBranch && currentBranch !== defaultBranch) {
    await displayComparison(defaultBranch, "HEAD", { mode: "divergence" });
    return;
  }

  const patches = await invoke<WorktreePatch[]>("open_worktree_review", { checkout: path });
  if (patches.length > 0) {
    layoutOverride = await readReviewLayout(path, null);
    base = "HEAD";
    target = "Working tree";
  }
  showWorktreeReview(patches);
}

function addProject(): void {
  void run(async () => {
    const registration = await invoke<Registration | null>("register_project");
    if (registration === null) return;
    const { project } = registration;
    projects = [...projects.filter(item => item.id !== project.id), project]
      .toSorted((first, second) => first.id - second.id);
    await openWorktree(registration.checkout, project.id);
    await revealProject(project.id);
  });
}

function reviewStatus(fileCount: number, reviewedCount: number): string {
  return fileCount === reviewedCount ? "Reviewed" : `${ reviewedCount } of ${ fileCount } reviewed`;
}

function markFile(displayed: Comparison, fileIndex: number): void {
  void run(async () => {
    const current = openReviewState;
    if (current?.kind !== "comparison" || comparisonUpdate === "branch_deleted") return;

    const progress = current.progress
      ? await invoke<CommitReviewProgress | null>("set_commit_file_reviewed", {
          checkout: activeCheckout,
          id      : current.progress.summary.id,
          fileIndex,
          reviewed: !reviewedFiles.has(fileIndex),
        })
      : await invoke<CommitReviewProgress>("start_commit_review", {
          checkout  : activeCheckout,
          comparison: displayed,
          selection : current.selection,
          layout    : layoutOverride,
          fileIndex,
        });
    openReviewState = {
      ...current,
      progress,
    };
  });
}

function fetchBranches(): void {
  void run(async () => {
    fetchStatus = "Fetching…";
    try {
      revisions = await invoke<Revisions>("fetch_revisions", { checkout: activeCheckout });
      fetchStatus = "Branches updated.";
    } catch (error_) {
      fetchStatus = String(error_);

      // Another remote may have succeeded before Git reported the error.
      revisions = await invoke<Revisions>("open_worktree", { checkout: activeCheckout }).catch(() => revisions);
    }
    await checkComparisonUpdate();
  });
}

function refreshComparison(): void {
  const projectId = activeProjectId;
  if (!base && projectId) {
    void run(() => openWorktree(activeCheckout, projectId));
    return;
  }
  if (target !== "Working tree" || !worktreeReview) {
    if (base) {
      const selection = openReviewState?.kind === "comparison" ? openReviewState.selection : null;
      void run(() => displayComparison(base, target, {
        mode,
        refresh: selection,
      }));
    }
    return;
  }
  void run(async () => {
    const patches = await invoke<WorktreePatch[]>("open_worktree_review", { checkout: activeCheckout });
    layoutOverride = await readReviewLayout(activeCheckout, null);
    showWorktreeReview(patches);
  });
}

function switchLayout(next: DiffLayout): void {
  if (next === layout) return;

  void run(async () => {
    const hasSavedReview = review && comparisonUpdate !== "branch_deleted";
    if (hasSavedReview || worktreeReview) {
      await invoke("set_review_layout", {
        checkout: activeCheckout,
        id      : review?.summary.id ?? null,
        layout  : next,
      });
    }
    layoutOverride = next;
  });
}

function markWorktreePatch(patch: WorktreePatch): void {
  void run(async () => {
    const patches = await invoke<WorktreePatch[]>("set_worktree_patch_reviewed", {
      checkout   : activeCheckout,
      fingerprint: patch.fingerprint,
      reviewed   : !patch.reviewed,
    });
    showWorktreeReview(patches);
  });
}

function toggleFileReviewed(index: number): void {
  const patch = worktreeReview?.[index];
  if (patch) markWorktreePatch(patch);
  else if (comparison) markFile(comparison, index);
}

function stageWorktreePatch(patch: WorktreePatch): void {
  void run(async () => {
    const result = await invoke<StageWorktreePatchResult>("stage_worktree_patch", {
      checkout   : activeCheckout,
      fingerprint: patch.fingerprint,
    });
    const selected = selectedWorktreePatch;
    const side = !result.error && selected?.fingerprint === patch.fingerprint ? "staged" : selected?.side;
    showWorktreeReview(result.patches, side);
    error = result.error ?? "";
  });
}
</script>

<svelte:window onfocus={() => {
  void refreshReviewSettings();
  void checkComparisonUpdate();
}} />

<main class={[
  "grid h-dvh grid-rows-[auto_minmax(0,1fr)] transition-[grid-template-columns] duration-260 ease-sidebar motion-reduce:transition-none",
  "[--projects-width:max(10rem,20vw)] [--toolbar-height:3.9rem] compact:[--projects-width:max(8rem,22vw)]",
  projectsCollapsed ? "grid-cols-[0px_minmax(0,1fr)]" : "grid-cols-[var(--projects-width)_minmax(0,1fr)]",
]} inert={settingsOpen}>
  <header class="app-bar relative z-2 col-start-2 row-start-1 box-border grid min-h-(--toolbar-height) min-w-0 grid-cols-[minmax(0,1fr)_auto]
    content-center items-center gap-x-[0.45rem] border-be border-divider bg-toolbar px-[0.9rem] py-[0.35rem]">
    <div class="flex min-w-0 items-center gap-[0.45rem] wrap-toolbar:flex-wrap">
      <button class="control workspace-button flex-none p-[0.23rem] text-[0.85rem] text-accent enabled:hover:bg-toolbar" type="button" aria-controls="projects"
        aria-expanded={!projectsCollapsed} aria-label={projectsAction} title={projectsAction} onclick={toggleProjects}>
        <svg class="toolbar-icon" aria-hidden="true" viewBox="0 0 16 16" fill="none"><rect x="2" y="3" width="12" height="10" rx="2" /><path d="M6 3v10" /></svg>
      </button>
      {#if activeProject}
        <span class="truncate text-muted">{activeProject.name}</span>
        <span class="max-w-[min(18rem,25vw)] min-w-0 truncate" title={activeBranch ?? activeCheckout}>{activeBranch ?? "Detached HEAD"}</span>
      {:else}
        <span class="truncate text-muted">Local Git review</span>
      {/if}
      <div class="ms-auto flex min-w-0 shrink items-center gap-[0.45rem] wrap-toolbar:flex-wrap">
        {#if revisions}
          <RevisionPicker {revisions} source={base} {target} {mode} {busy} {fetchStatus}
            bind:sourceOpen oncompare={compare} onfetch={fetchBranches} />
        {/if}
        {#if files.length}
          <div class="flex rounded-[5px] border border-divider" role="group" aria-label="Diff layout">
            <button class="control workspace-button rounded-none border-0 bg-transparent text-[0.85rem] aria-pressed:bg-selection aria-pressed:text-accent"
              type="button" aria-pressed={layout === "unified"} onclick={() => switchLayout("unified")} disabled={busy}>
              Unified
            </button>
            <button class="control workspace-button rounded-none border-0 border-s border-divider bg-transparent text-[0.85rem] aria-pressed:bg-selection aria-pressed:text-accent"
              type="button" aria-pressed={layout === "split"} onclick={() => switchLayout("split")} disabled={busy}>
              Split
            </button>
          </div>
        {/if}
        {#if revisions}
          {#if comparisonUpdate === "available" || comparisonUpdate === "branch_deleted"}
            <span class="text-[0.75rem] whitespace-nowrap text-accent" role="status">
              {comparisonUpdate === "available" ? "Update available" : "Branch deleted"}
            </span>
          {/if}
          <button id="worktree-review" class="control workspace-button flex-none p-[0.23rem] text-[0.85rem] enabled:hover:bg-toolbar" type="button" onclick={refreshComparison}
            aria-label="Refresh comparison" title="Refresh comparison" disabled={busy || comparisonUpdate === "branch_deleted"}>
            <svg class="toolbar-icon" aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M13 7a5 5 0 1 0-.5 4M13 3v4H9" /></svg>
          </button>
        {/if}
      </div>
    </div>
    {#if files.length}
      <button class="control workspace-button p-[0.23rem] text-[0.85rem] enabled:hover:bg-toolbar aria-expanded:bg-selection aria-expanded:text-accent" type="button" aria-controls="changed-files"
        aria-expanded={!filesCollapsed} aria-label={filesAction} title={filesAction}
        onclick={(event) => {
          event.currentTarget.focus();
          filesCollapsed = !filesCollapsed;
        }}>
        <svg class="toolbar-icon" aria-hidden="true" viewBox="0 0 16 16" fill="none"><rect x="2" y="3" width="12" height="10" rx="2" /><path d="M10 3v10" /></svg>
      </button>
    {/if}
  </header>

  <div class="col-start-1 row-[1/-1] min-h-0 min-w-0 overflow-clip">
    <aside id="projects" class="box-border grid h-full min-h-0 w-(--projects-width) min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] border-e border-divider bg-surface"
      aria-label="Projects" inert={projectsCollapsed} bind:this={projectSidebar}>
      <header class="box-border flex min-h-(--toolbar-height) items-center justify-between gap-[0.45rem] border-be border-divider px-[0.85rem] py-[0.55rem] font-bold">
        <span>Projects</span>
        <button class="control workspace-button p-[0.23rem] enabled:hover:bg-toolbar" type="button" aria-label="Add checkout" title="Add checkout"
          onclick={addProject} disabled={busy}>
          <svg class="toolbar-icon" aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M8 3v10M3 8h10" /></svg>
        </button>
      </header>
      <div class="project-list overflow-auto px-[0.65rem] py-[0.35rem]">
        {#each projects as project (project.id)}
          <details id={`project-${ project.id }`} open>
            <summary class="cursor-pointer px-[0.2rem] py-[0.4rem] font-[650] wrap-anywhere hover:bg-toolbar" title={project.mainCheckout ?? project.name}>{project.name}</summary>
            {#if project.worktreeError}
              <p class="m-0">{project.worktreeError}</p>
            {/if}
            <ul class="m-0 list-none p-0 ps-[0.45rem]">
              {#each project.worktrees as worktree (worktree.path)}
                <li class="not-first:mt-[0.2rem]">
                  <button class="control workspace-button block w-full border-transparent bg-transparent text-start wrap-anywhere
                    enabled:hover:bg-toolbar aria-pressed:border-accent aria-pressed:bg-selection aria-pressed:text-accent"
                    type="button"
                    aria-pressed={activeCheckout === worktree.path}
                    onclick={() => run(() => openWorktree(worktree.path, project.id))}
                    disabled={busy}>
                    <strong>{worktree.branch ?? "Detached HEAD"}</strong>
                    <small class="block text-[0.85rem] text-muted">{worktree.path}</small>
                  </button>
                </li>
              {/each}
            </ul>
          </details>
        {/each}
      </div>
      <footer class="relative z-2 flex items-center justify-between gap-[0.45rem] border-bs border-divider px-[0.85rem] py-[0.55rem]">
        <h1 class="m-0 flex items-center gap-[0.45rem] text-[1rem]"><svg class="size-5 stroke-accent stroke-[1.4]" aria-hidden="true" viewBox="0 0 18 18" fill="none"><rect x="1.5" y="1.5" width="15" height="15" rx="3" /><path d="M6 7h6M9 4v6m-3 3h6" /></svg>Diffest</h1>
        <button class={[
          "control workspace-button relative p-[0.23rem] enabled:hover:bg-selection",
          settingsAnnouncement && "after:absolute after:top-[0.12rem] after:right-[0.12rem] after:size-[0.35rem] after:rounded-full after:bg-removed-ink after:content-['']",
        ]}
          type="button" bind:this={settingsButton} title="Settings"
          aria-label={settingsAnnouncement ? "Settings, error" : "Settings"} onclick={openSettings}>
          <svg class="toolbar-icon" aria-hidden="true" viewBox="0 0 16 16" fill="none">
            <path d="m6.8 1.8-.4 1.4-1.1.6-1.4-.4-1.2 2 .9 1.2v1.3l-.9 1.2 1.2 2 1.4-.4 1.1.6.4 1.4h2.4l.4-1.4 1.1-.6 1.4.4 1.2-2-.9-1.2V6.6l.9-1.2-1.2-2-1.4.4-1.1-.6-.4-1.4Z" transform="translate(0 .75)" />
            <circle cx="8" cy="8" r="2" />
          </svg>
        </button>
      </footer>
    </aside>
  </div>

  <section class={[
    "@container relative col-start-2 row-start-2 flex min-h-0 min-w-0 flex-col",
    !files.length && "overflow-auto",
  ]} aria-label="Review workspace" aria-busy={busy}>
    <span class="sr-only" aria-live="polite">{settingsAnnouncement}</span>
    {#if error}
      <p class="m-0 bg-removed px-[0.85rem] py-[0.55rem] wrap-anywhere text-removed-ink" role="alert">{error}</p>
    {/if}
    {#if files.length}
      <!-- Keep the pane full width during collapse without animating when Projects resizes the workspace. -->
      <div class={[
        "grid min-h-0 min-w-0 flex-1 transition-[grid-template-columns] duration-260 ease-sidebar motion-reduce:transition-none",
        "[--files-width:max(12rem,32cqw)] compact:[--files-width:max(9rem,34cqw)]",
        filesCollapsed ? "grid-cols-[minmax(0,1fr)_0px]" : "grid-cols-[minmax(0,1fr)_max(12rem,32%)] compact:grid-cols-[minmax(0,1fr)_max(9rem,34%)]",
      ]}>
        {#key reviewSession}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of the file stream.) -->
          <section class="file-stream grid min-h-0 min-w-0 content-start gap-[0.65rem] overflow-auto p-[0.65rem] [scrollbar-gutter:stable] focus-visible:-outline-offset-2"
            aria-label="File diffs" tabindex="0" bind:this={fileStream}>
            {#if comparison}
              <p class="m-0 text-[0.8rem] text-muted" title={`${ comparison.start } ← ${ comparison.end }`}>
                {comparison.mode === "divergence" ? "Changes since divergence" : "Changes between revisions"}
                <code class="ms-[0.45rem]">{comparison.start.slice(0, 7)} ← {comparison.end.slice(0, 7)}</code>
              </p>
            {:else if worktreeReview}
              <p class="m-0 text-[0.8rem] text-muted">Uncommitted changes · staged and unstaged</p>
            {/if}
            {#each orderedIndices as index (worktreeReview?.[index]?.fingerprint ?? index)}
              {@const patch = worktreeReview?.[index]}
              <ReviewFile {index} file={files[index]} {patch} checkout={activeCheckout} {comparison} {layout} busy={busy || comparisonUpdate === "branch_deleted"}
                category={categories === null ? null : categories[index]?.category}
                {windowRows}
                reviewed={patch?.reviewed ?? reviewedFiles.has(index)}
                onreview={() => toggleFileReviewed(index)} onstage={stageWorktreePatch} />
            {/each}
          </section>
        {/key}
        <div class="flex min-h-0 min-w-0 justify-end overflow-clip">
          <aside id="changed-files" class="box-border w-(--files-width) flex-none overflow-auto border-s border-divider bg-surface" aria-label="File sidebar" inert={filesCollapsed}>
            <header class="sticky top-0 z-1 bg-surface px-[0.8rem] pt-[0.7rem] pb-[0.4rem]">
              <div class="flex items-center gap-[0.4rem]">
                <strong>Files <span class="font-normal text-muted">{files.length}</span></strong>
                {#if lineTotals.added || lineTotals.removed}
                  <span class="ms-auto flex gap-[0.3rem] font-mono text-[0.75rem]/[normal] compact:gap-[0.16rem]" role="group"
                    aria-label={`Total lines added ${ lineTotals.added }, lines removed ${ lineTotals.removed }`}>
                    {#if lineTotals.added}<span class="text-added-ink" aria-hidden="true">+{lineTotals.added}</span>{/if}
                    {#if lineTotals.removed}<span class="text-removed-ink" aria-hidden="true">−{lineTotals.removed}</span>{/if}
                  </span>
                {/if}
              </div>
              <p id="review-progress" class="m-0 text-[0.8rem] text-muted" role="status">{reviewStatus(files.length, reviewedFileCount)}</p>
              <progress class="mbs-[0.4rem] mbe-[0.2rem] block h-[0.2rem] w-full appearance-none overflow-hidden rounded-xs border-0 bg-divider
                [&::-webkit-progress-bar]:bg-divider [&::-webkit-progress-value]:bg-accent [&::-moz-progress-bar]:bg-accent"
                aria-label="Review progress" value={reviewedFileCount} max={files.length}></progress>
            </header>
            <CategorizedFileTree {groups} {patchSummaries}>
              {#snippet entry(index)}
                {@const file = files[index]}
                {@const worktreePatch = worktreeReview?.[index]}
                {@const checked = worktreePatch?.reviewed ?? reviewedFiles.has(index)}
                {@const counts = patchSummaries[index]}
                <button class="control workspace-button relative flex min-h-8 w-full items-center gap-[0.36rem] border-transparent bg-transparent text-start
                  enabled:hover:bg-toolbar aria-pressed:border-accent aria-pressed:bg-selection aria-pressed:text-accent compact:gap-[0.24rem] compact:px-[0.22rem]"
                  type="button" aria-pressed={selectedIndex === index}
                  title={file.previousPath ? `${ file.previousPath } → ${ file.path }` : file.path}
                  aria-controls={`file-review-${ index }`} onclick={() => jumpToFile(index)}>
                  <span class={["w-[1em] flex-none font-mono text-[0.76rem]/[normal]", file.status === "U" ? "text-removed-ink" : "text-muted"]}
                    aria-hidden="true">{file.status}</span>
                  <span class="sr-only">{statusLabels[file.status] ?? file.status}</span>
                  <span class="min-w-0 flex-1">
                    <strong class="block truncate text-[0.9rem] font-[550]">{fileName(file.path)}</strong>
                    {#if worktreePatch || file.previousPath || file.path.includes("/")}
                      <small class="block truncate text-[0.75rem] text-muted">
                        {#if worktreePatch}{worktreePatch.side === "staged" ? "Staged" : "Unstaged"}{/if}
                        {#if worktreePatch && (file.previousPath || file.path.includes("/"))}<span aria-hidden="true"> · </span>{/if}
                        {#if file.previousPath}{file.previousPath} → {:else}{file.path.slice(0, file.path.lastIndexOf("/") + 1)}{/if}
                      </small>
                    {/if}
                  </span>
                  <span class="ms-auto flex gap-[0.3rem] font-mono text-[0.75rem]/[normal] compact:gap-[0.16rem]">
                    {#if counts.added}<span class="text-added-ink">+{counts.added}</span>{/if}
                    {#if counts.removed}<span class="text-removed-ink">−{counts.removed}</span>{/if}
                  </span>
                  {#if worktreePatch || comparison}
                    <span class={["flex-none", checked ? "text-accent" : "text-muted"]}>
                      <span class="sr-only">{checked ? "Reviewed" : "Unreviewed"}</span>
                      <svg class="toolbar-icon" aria-hidden="true" viewBox="0 0 16 16" fill="none">
                        {#if checked}<path d="m3 8 3 3 7-7" />{:else}<circle cx="8" cy="8" r="4" />{/if}
                      </svg>
                    </span>
                  {/if}
                </button>
              {/snippet}
            </CategorizedFileTree>
          </aside>
        </div>
      </div>
    {:else if openReviewState}
      <div class="p-5">
        <h2 class="m-0 mbe-[0.35rem] text-[1.1rem]">{worktreeReview ? "No uncommitted changes." : "No differences."}</h2>
        {#if !base}
          <p class="m-0 text-muted">Choose a source branch or commit to compare with HEAD.</p>
          <button class="control workspace-button enabled:hover:bg-toolbar" type="button" onclick={() => sourceOpen = true} disabled={busy}>Choose source</button>
        {:else if review}
          <p id="review-progress" class="m-0 text-[0.8rem] text-muted" role="status">{reviewStatus(0, 0)}</p>
        {/if}
      </div>
    {:else}
      <div class="p-5">
        <h2 class="m-0 mbe-[0.35rem] text-[1.1rem]">{revisions ? "Start a review" : "Open a local checkout"}</h2>
        <p class="m-0 text-muted">{revisions ? "Choose a source branch or commit to compare with HEAD." : "Add a checkout or select a worktree from Projects."}</p>
      </div>
    {/if}
  </section>
</main>
{#if settingsOpen}
  <CategorySettings {projects} {drafts} onback={closeSettings} onsaved={refreshReviewSettings} />
{/if}
