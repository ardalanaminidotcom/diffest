<script lang="ts">
import type {
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
} from "../lib/revisions.js";
import { invoke } from "@tauri-apps/api/core";
import { onMount, tick } from "svelte";
import CategorizedFileTree from "../lib/categorized-file-tree.svelte";
import { type FileCategory, groupFiles } from "../lib/category-settings.js";
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
  }
  | {
    kind: "worktree";
    patches: WorktreePatch[];
  };
/* eslint-enable @stylistic/key-spacing */

let projects = $state<Project[]>([]);
let activeCheckout = $state("");
let activeProjectId = $state<number>();
let revisions = $state<Revisions>();
let base = $state("");
let target = $state("HEAD");
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
let categories = $state<FileCategory[]>([]);
const groups = $derived(groupFiles(paths, categories));
const orderedIndices = $derived([...groups.values()].flatMap(subgroups => [...subgroups.values()].flat()));
const selectedIndex = $derived(jumpedFileIndex ?? orderedIndices[0] ?? 0);
const selectedWorktreePatch = $derived(worktreeReview?.[selectedIndex]);
let error = $state("");
let busy = $state(false);
let savedReviewsOpen = $state(false);
let sourceOpen = $state(false);
let filesCollapsed = $state(false);
const filesAction = $derived(filesCollapsed ? "Open Files sidebar" : "Collapse Files sidebar");
let projectsCollapsed = $state(false);
const projectsAction = $derived(projectsCollapsed ? "Open Projects sidebar" : "Collapse Projects sidebar");
let settingsOpen = $state(false);
let settingsAnnouncement = $state("");
let projectSidebar: HTMLElement | undefined = $state();
let fileStream: HTMLElement | undefined = $state();
const activeProject = $derived(projects.find(project => project.id === activeProjectId));
const activeBranch = $derived(activeProject?.worktrees.find(worktree => worktree.path === activeCheckout)?.branch);
const patchSummaries = $derived(files.map(file => summarizePatch(file.patch)));
const mountedRowLimit = 20_000;
const windowRows = $derived(patchSummaries.reduce((rows, patch) => rows + patch.lineCount, 0) > mountedRowLimit);
const reviewedWorktreeCount = $derived(worktreeReview?.filter(patch => patch.reviewed).length);
const reviewedFileCount = $derived(reviewedWorktreeCount ?? review?.summary.reviewedCount ?? 0);

function toggleProjects(): void {
  projectsCollapsed = !projectsCollapsed;
  if (projectsCollapsed) settingsOpen = false;
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

onMount(() => {
  void run(async () => {
    projects = await invoke<Project[]>("list_projects");
  });
});

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

async function displayComparison(source: string, destination: string, comparisonMode: ComparisonMode): Promise<void> {
  if (!source) {
    layoutOverride = null;
    openReviewState = null;
  } else if (destination === "Working tree" && (source === "HEAD" || source === revisions?.currentBranch)) {
    const patches = await invoke<WorktreePatch[]>("open_worktree_review", { checkout: activeCheckout });
    layoutOverride = await readReviewLayout(activeCheckout, null);
    showWorktreeReview(patches);
  } else {
    const { progress, ...displayed } = await invoke<Comparison & { progress: CommitReviewProgress | null; }>(
      "compare_revisions",
      {
        checkout: activeCheckout,
        base    : source,
        target  : destination,
        mode    : comparisonMode,
      },
    );
    layoutOverride = progress ? await readReviewLayout(activeCheckout, progress.summary.id) : null;
    openReviewState = {
      kind      : "comparison",
      comparison: displayed,
      progress  : progress ?? null,
    };
  }
  base = source;
  target = destination;
  mode = comparisonMode;
  filesCollapsed = false;
  startFileStream();
}

function compare(source: string, destination: string, comparisonMode: ComparisonMode): void {
  void run(() => displayComparison(source, destination, comparisonMode));
}

async function openWorktree(path: string, projectId: number, loadChanges = true): Promise<void> {
  const nextRevisions = await invoke<Revisions>("open_worktree", { checkout: path });
  layoutOverride = null;
  filesCollapsed = false;
  revisions = nextRevisions;
  activeCheckout = path;
  activeProjectId = projectId;
  base = "";
  target = "HEAD";
  mode = "between";
  sourceOpen = false;
  openReviewState = null;
  startFileStream();
  if (!loadChanges) return;

  const { defaultBranch, currentBranch } = nextRevisions;
  if (defaultBranch && currentBranch !== defaultBranch) {
    await displayComparison(defaultBranch, "HEAD", "divergence");
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

function updateReview(displayed: Comparison, progress: CommitReviewProgress): void {
  openReviewState = {
    kind      : "comparison",
    comparison: displayed,
    progress,
  };
  const { summary } = progress;
  projects = projects.map((project) => {
    if (project.id !== activeProjectId) return project;

    return {
      ...project,
      savedReviews: [...project.savedReviews.filter(item => item.id !== summary.id), summary]
        .toSorted((first, second) => second.id - first.id),
    };
  });
}

function openReview(project: Project, id: number): void {
  const path = project.id === activeProjectId ? activeCheckout : project.worktrees[0]?.path;
  if (!path) return;

  void run(async () => {
    if (path !== activeCheckout) await openWorktree(path, project.id, false);

    const saved = await invoke<CommitReview>("open_commit_review", {
      checkout: path,
      id,
    });
    layoutOverride = await readReviewLayout(path, id);
    const { comparison: displayed, summary, reviewedFiles: marks } = saved;
    base = displayed.baseLabel;
    target = displayed.targetLabel;
    mode = displayed.mode;
    savedReviewsOpen = false;
    filesCollapsed = false;
    sourceOpen = false;
    startFileStream();
    updateReview(displayed, {
      summary,
      reviewedFiles: marks,
    });
    await revealProject(project.id);
  });
}

function markFile(displayed: Comparison, fileIndex: number): void {
  void run(async () => {
    const progress = review
      ? await invoke<CommitReviewProgress>("set_commit_file_reviewed", {
          checkout: activeCheckout,
          id      : review.summary.id,
          fileIndex,
          reviewed: !reviewedFiles.has(fileIndex),
        })
      : await invoke<CommitReviewProgress>("start_commit_review", {
          checkout  : activeCheckout,
          comparison: displayed,
          layout    : layoutOverride,
          fileIndex,
        });
    updateReview(displayed, progress);
  });
}

function refreshComparison(): void {
  const projectId = activeProjectId;
  if (!base && projectId) {
    void run(() => openWorktree(activeCheckout, projectId));
    return;
  }
  if (target !== "Working tree" || !worktreeReview) {
    if (base) compare(base, target, mode);
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
    if (review || worktreeReview) {
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

<main class:projects-collapsed={projectsCollapsed}>
  <header class="app-bar">
    <div class="app-bar-content">
      <button class="icon-button projects-toggle" type="button" aria-controls="projects"
        aria-expanded={!projectsCollapsed} aria-label={projectsAction} title={projectsAction} onclick={toggleProjects}>
        <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><rect x="2" y="3" width="12" height="10" rx="2" /><path d="M6 3v10" /></svg>
      </button>
      {#if activeProject}
        <span class="project-name">{activeProject.name}</span>
        <span class="branch" title={activeBranch ?? activeCheckout}>{activeBranch ?? "Detached HEAD"}</span>
      {:else}
        <span class="project-name">Local Git review</span>
      {/if}
      <div class="workspace-actions">
        {#if revisions}
          <RevisionPicker {revisions} source={base} {target} {mode} {busy} bind:sourceOpen oncompare={compare} />
        {/if}
        {#if files.length}
          <div class="diff-layout" role="group" aria-label="Diff layout">
            <button type="button" aria-pressed={layout === "unified"} onclick={() => switchLayout("unified")} disabled={busy}>
              Unified
            </button>
            <button type="button" aria-pressed={layout === "split"} onclick={() => switchLayout("split")} disabled={busy}>
              Split
            </button>
          </div>
        {/if}
        {#if projects.some(project => project.savedReviews.length > 0)}
          <details name="workspace-menu" class="toolbar-disclosure saved-reviews" bind:open={savedReviewsOpen}>
            <summary>Saved reviews</summary>
            <div class="toolbar-panel">
              {#each projects as project (project.id)}
                {#if project.savedReviews.length}
                  <section aria-label="Saved commit reviews">
                    <h3>{project.name}</h3>
                    <ul>
                      {#each project.savedReviews as saved (saved.id)}
                        <li>
                          <button
                            type="button"
                            aria-pressed={review?.summary.id === saved.id}
                            onclick={() => openReview(project, saved.id)}
                            disabled={busy || !project.worktrees.length}>
                            {saved.baseLabel.replace(/^refs\/(heads|remotes)\//u, "")} ← {saved.targetLabel}
                            <small><code>{saved.start.slice(0, 7)} ← {saved.end.slice(0, 7)}</code></small>
                            <small>{reviewStatus(saved.fileCount, saved.reviewedCount)}</small>
                          </button>
                        </li>
                      {/each}
                    </ul>
                  </section>
                {/if}
              {/each}
            </div>
          </details>
        {/if}
        {#if revisions && !review}
          <button id="worktree-review" class="icon-button refresh" type="button" onclick={refreshComparison}
            aria-label="Refresh comparison" title="Refresh comparison" disabled={busy}>
            <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M13 7a5 5 0 1 0-.5 4M13 3v4H9" /></svg>
          </button>
        {/if}
      </div>
    </div>
    {#if files.length}
      <button class="icon-button files-toggle" type="button" aria-controls="changed-files"
        aria-expanded={!filesCollapsed} aria-label={filesAction} title={filesAction}
        onclick={(event) => {
          event.currentTarget.focus();
          filesCollapsed = !filesCollapsed;
        }}>
        <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><rect x="2" y="3" width="12" height="10" rx="2" /><path d="M10 3v10" /></svg>
      </button>
    {/if}
  </header>

  <div class="projects-column" class:settings-open={settingsOpen}>
    <aside id="projects" aria-label="Projects" inert={projectsCollapsed} bind:this={projectSidebar}>
      <header class="panel-label">
        <span>Projects</span>
        <button class="icon-button" type="button" aria-label="Add checkout" title="Add checkout"
          onclick={addProject} disabled={busy}>
          <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M8 3v10M3 8h10" /></svg>
        </button>
      </header>
      <div class="project-list">
        {#each projects as project (project.id)}
          <details id={`project-${ project.id }`} open>
            <summary title={project.mainCheckout ?? project.name}>{project.name}</summary>
            {#if project.worktreeError}
              <p>{project.worktreeError}</p>
            {/if}
            <ul>
              {#each project.worktrees as worktree (worktree.path)}
                <li>
                  <button
                    type="button"
                    aria-pressed={activeCheckout === worktree.path}
                    onclick={() => run(() => openWorktree(worktree.path, project.id))}
                    disabled={busy}>
                    <strong>{worktree.branch ?? "Detached HEAD"}</strong>
                    <small>{worktree.path}</small>
                  </button>
                </li>
              {/each}
            </ul>
          </details>
        {/each}
      </div>
      <footer class="project-footer">
        <h1><svg aria-hidden="true" viewBox="0 0 18 18" fill="none"><rect x="1.5" y="1.5" width="15" height="15" rx="3" /><path d="M6 7h6M9 4v6m-3 3h6" /></svg>Diffest</h1>
        <CategorySettings {mainCheckout} {paths} bind:categories bind:defaultLayout bind:open={settingsOpen}
          bind:announcement={settingsAnnouncement} />
      </footer>
    </aside>
  </div>

  <section class="workspace" aria-label="Review workspace" aria-busy={busy}>
    <span class="sr-only" aria-live="polite">{settingsAnnouncement}</span>
    {#if error}
      <p class="alert" role="alert">{error}</p>
    {/if}
    {#if files.length}
      <div class="changes" class:files-collapsed={filesCollapsed}>
        {#key reviewSession}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of the file stream.) -->
          <section class="file-stream" aria-label="File diffs" tabindex="0" bind:this={fileStream}>
            {#if comparison}
              <p class="comparison-note" title={`${ comparison.start } ← ${ comparison.end }`}>
                {comparison.mode === "divergence" ? "Changes since divergence" : "Changes between revisions"}
                <code>{comparison.start.slice(0, 7)} ← {comparison.end.slice(0, 7)}</code>
              </p>
            {:else if worktreeReview}
              <p class="comparison-note">Uncommitted changes · staged and unstaged</p>
            {/if}
            {#each orderedIndices as index (worktreeReview?.[index]?.fingerprint ?? index)}
              {@const patch = worktreeReview?.[index]}
              <ReviewFile {index} file={files[index]} {patch} checkout={activeCheckout} {comparison} {layout} {busy}
                {windowRows}
                reviewed={patch?.reviewed ?? reviewedFiles.has(index)}
                onreview={() => toggleFileReviewed(index)} onstage={stageWorktreePatch} />
            {/each}
          </section>
        {/key}
        <div class="files-column">
          <aside id="changed-files" class="file-list" aria-label="File sidebar" inert={filesCollapsed}>
            <header class="file-list-header">
              <strong class="file-list-title">Files <span>{files.length}</span></strong>
              <p id="review-progress" role="status">{reviewStatus(files.length, reviewedFileCount)}</p>
              <progress aria-label="Review progress" value={reviewedFileCount} max={files.length}></progress>
            </header>
            <CategorizedFileTree {groups}>
              {#snippet entry(index)}
                {@const file = files[index]}
                {@const worktreePatch = worktreeReview?.[index]}
                {@const checked = worktreePatch?.reviewed ?? reviewedFiles.has(index)}
                {@const counts = patchSummaries[index]}
                <button class="file-row" type="button" aria-pressed={selectedIndex === index}
                  title={file.previousPath ? `${ file.previousPath } → ${ file.path }` : file.path}
                  aria-controls={`file-review-${ index }`} onclick={() => jumpToFile(index)}>
                  <span class="file-status" class:conflict={file.status === "U"} aria-hidden="true">{file.status}</span>
                  <span class="sr-only">{statusLabels[file.status] ?? file.status}</span>
                  <span class="file-label">
                    <strong>{fileName(file.path)}</strong>
                    {#if worktreePatch || file.previousPath || file.path.includes("/")}
                      <small>
                        {#if worktreePatch}{worktreePatch.side === "staged" ? "Staged" : "Unstaged"}{/if}
                        {#if worktreePatch && (file.previousPath || file.path.includes("/"))}<span aria-hidden="true"> · </span>{/if}
                        {#if file.previousPath}{file.previousPath} → {:else}{file.path.slice(0, file.path.lastIndexOf("/") + 1)}{/if}
                      </small>
                    {/if}
                  </span>
                  <span class="line-counts">
                    {#if counts.added}<span class="additions">+{counts.added}</span>{/if}
                    {#if counts.removed}<span class="deletions">−{counts.removed}</span>{/if}
                  </span>
                  {#if worktreePatch || comparison}
                    <span class="review-mark" class:checked>
                      <span class="sr-only">{checked ? "Reviewed" : "Unreviewed"}</span>
                      <svg aria-hidden="true" viewBox="0 0 16 16" fill="none">
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
      <div class="empty-state">
        <h2>{worktreeReview ? "No uncommitted changes." : "No differences."}</h2>
        {#if !base}
          <p>Choose a source branch or commit to compare with HEAD.</p>
          <button type="button" onclick={() => sourceOpen = true} disabled={busy}>Choose source</button>
        {:else if review}
          <p id="review-progress" role="status">{reviewStatus(0, 0)}</p>
        {/if}
      </div>
    {:else}
      <div class="empty-state">
        <h2>{revisions ? "Start a review" : "Open a local checkout"}</h2>
        <p>{revisions ? "Choose a source branch or commit to compare with HEAD." : "Add a checkout or select a worktree from Projects."}</p>
      </div>
    {/if}
  </section>
</main>

<style>
:global(html) {
  color-scheme: light dark;
  --canvas: light-dark(#fbfcfb, #171b1e);
  --surface: light-dark(#f4f6f5, #1b2024);
  --toolbar: light-dark(#edf1ef, #20262a);
  --border: light-dark(#d6dfdc, #343c41);
  --text: light-dark(#202d30, #e3e8eb);
  --muted: light-dark(#526267, #a4afb7);
  --accent: light-dark(#25596a, #a4d7e6);
  --selection: light-dark(#e2edf0, #273941);
  --added: light-dark(#e2f3e9, #21392f);
  --removed: light-dark(#f9e7ea, #402b32);
  --added-ink: light-dark(#276044, #9bdcb4);
  --removed-ink: light-dark(#8f3b4b, #f2aeba);
  --syntax-keyword: light-dark(#65478e, #c5b9ee);
  --syntax-name: light-dark(#215d78, #a9cee5);
  --syntax-string: light-dark(#765424, #e7d3a0);
  --syntax-number: light-dark(#8a4c17, #e4b595);
  background: var(--canvas);
  color: var(--text);
  accent-color: var(--accent);
  font: 13px/1.45 system-ui, sans-serif;
}

:global(body) { margin: 0; }
:global(*::selection) { background: var(--selection); color: var(--text); }
:global(:focus-visible) { outline: 2px solid var(--accent); outline-offset: 2px; }
main {
  --projects-width: max(10rem, 20vw);
  --toolbar-height: 3.9rem;
  --sidebar-motion: 260ms cubic-bezier(0.2, 0.72, 0.28, 1);
  display: grid;
  grid-template-columns: var(--projects-width) minmax(0, 1fr);
  grid-template-rows: auto minmax(0, 1fr);
  height: 100dvh;
  transition: grid-template-columns var(--sidebar-motion);
}
main.projects-collapsed { grid-template-columns: 0px minmax(0, 1fr); }

.app-bar-content, .workspace-actions {
  display: flex;
  align-items: center;
  gap: 0.45rem;
}
.app-bar {
  grid-column: 2;
  grid-row: 1;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  column-gap: 0.45rem;
  align-items: center;
  position: relative;
  min-width: 0;
  align-content: center;
  z-index: 2;
  box-sizing: border-box;
  min-height: var(--toolbar-height);
  padding: 0.35rem 0.9rem;
  border-block-end: 1px solid var(--border);
  background: var(--toolbar);
}
.app-bar-content { min-width: 0; }
.projects-toggle { flex: none; color: var(--accent); }
.files-toggle[aria-expanded="true"] { background: var(--selection); color: var(--accent); }
h1, h2, h3, p { margin: 0; }
h1 { display: flex; align-items: center; gap: 0.45rem; font-size: 1rem; }
h1 svg { width: 1.25rem; height: 1.25rem; stroke: var(--accent); stroke-width: 1.4; }
.project-name { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.branch { min-width: 0; max-width: min(18rem, 25vw); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.workspace-actions { margin-inline-start: auto; flex: 0 1 auto; min-width: 0; }
.refresh { flex: none; }
.toolbar-disclosure > summary {
  cursor: pointer;
  list-style: none;
  background: var(--canvas);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding: 0.3rem 0.52rem;
  font-size: 0.85rem;
}
.toolbar-disclosure > summary::-webkit-details-marker { display: none; }
.toolbar-disclosure[open] > summary { background: var(--selection); }
.toolbar-panel {
  position: absolute;
  inset: 100% 0 auto auto;
  z-index: 2;
  box-sizing: border-box;
  width: min(100%, 24rem);
  max-height: min(65dvh, calc(100dvh - 9rem));
  overflow: auto;
  padding: 0.85rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 0 0 0 8px;
  box-shadow: 0 6px 18px #0002;
}
.projects-column, .workspace, .changes, .file-stream { min-width: 0; min-height: 0; }
.projects-column { grid-column: 1; grid-row: 1 / -1; overflow: clip; }
main:not(.projects-collapsed) .projects-column.settings-open { overflow: visible; }
.workspace {
  grid-column: 2;
  grid-row: 2;
  position: relative;
  display: flex;
  flex-direction: column;
  container-type: inline-size;
}
.workspace:has(.empty-state) { overflow: auto; }
#projects {
  box-sizing: border-box;
  width: var(--projects-width);
  height: 100%;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  min-height: 0;
  min-width: 0;
  background: var(--surface);
  border-inline-end: 1px solid var(--border);
}
.project-list { overflow: auto; padding: 0.35rem 0.65rem; }
.project-list summary { padding: 0.4rem 0.2rem; cursor: pointer; font-weight: 650; overflow-wrap: anywhere; }
.project-list summary:hover { background: var(--toolbar); }
.project-footer {
  position: relative;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.45rem;
  border-block-start: 1px solid var(--border);
  padding: 0.55rem 0.85rem;
}
.panel-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  box-sizing: border-box;
  min-height: var(--toolbar-height);
  gap: 0.45rem;
  padding: 0.55rem 0.85rem;
  border-block-end: 1px solid var(--border);
  font-weight: 700;
}
.saved-reviews h3 { margin-block: 1rem 0.35rem; font-size: 0.85rem; overflow-wrap: anywhere; }
aside ul, .saved-reviews ul { list-style: none; padding: 0; margin: 0; }
.project-list ul { padding-inline-start: 0.45rem; }
aside li + li, .saved-reviews li + li { margin-block-start: 0.2rem; }
aside li button, .saved-reviews li button {
  display: block;
  width: 100%;
  text-align: start;
  overflow-wrap: anywhere;
  background: transparent;
  border-color: transparent;
}
aside small, .saved-reviews small { display: block; }
small { color: var(--muted); font-size: 0.85rem; }
button {
  min-width: 0;
  box-sizing: border-box;
  font: inherit;
  color: inherit;
  background: var(--canvas);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding: 0.3rem 0.52rem;
}
button { cursor: pointer; }
button:disabled { cursor: default; opacity: 0.58; }
button:not(:disabled):hover { background: var(--toolbar); }
button[aria-pressed="true"] {
  color: var(--accent);
  border-color: var(--accent);
  background: var(--selection);
}

.alert { padding: 0.55rem 0.85rem; color: var(--removed-ink); background: var(--removed); overflow-wrap: anywhere; }
.empty-state { padding: 1.25rem; }
.empty-state h2 { font-size: 1.1rem; margin-block-end: 0.35rem; }
.empty-state p { color: var(--muted); }

.changes {
  /* Keep the pane full width during collapse without animating the track when Projects resizes the workspace. */
  --files-width: max(12rem, 32cqw);
  flex: 1;
  display: grid;
  grid-template-columns: minmax(0, 1fr) max(12rem, 32%);
  transition: grid-template-columns var(--sidebar-motion);
}
.changes.files-collapsed { grid-template-columns: minmax(0, 1fr) 0px; }
.files-column {
  display: flex;
  justify-content: flex-end;
  min-width: 0;
  min-height: 0;
  overflow: clip;
}
.file-list {
  flex: none;
  box-sizing: border-box;
  width: var(--files-width);
  background: var(--surface);
  border-inline-start: 1px solid var(--border);
  overflow: auto;
}
@media (prefers-reduced-motion: reduce) {
  main, .changes { transition: none; }
}
.file-list-header {
  position: sticky;
  top: 0;
  z-index: 1;
  padding: 0.7rem 0.8rem 0.4rem;
  background: var(--surface);
}
.file-list-title span, #review-progress { color: var(--muted); font-weight: 400; }
#review-progress { font-size: 0.8rem; }
progress {
  appearance: none;
  display: block;
  width: 100%;
  height: 0.2rem;
  margin-block: 0.4rem 0.2rem;
  border: 0;
  border-radius: 2px;
  background: var(--border);
  overflow: hidden;
}
progress::-webkit-progress-bar { background: var(--border); }
progress::-webkit-progress-value { background: var(--accent); }
progress::-moz-progress-bar { background: var(--accent); }
.file-row {
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.36rem;
  width: 100%;
  min-height: 2rem;
  background: transparent;
  border-color: transparent;
  text-align: start;
}
.file-status { flex: none; width: 1em; color: var(--muted); font: 0.76rem ui-monospace, monospace; }
.file-label { flex: 1; min-width: 0; }
.file-label strong, .file-label small { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.file-label strong { font-size: 0.9rem; font-weight: 550; }
.file-label small { font-size: 0.75rem; }
.line-counts { display: flex; gap: 0.3rem; margin-inline-start: auto; font: 0.75rem ui-monospace, monospace; }
.additions { color: var(--added-ink); }
.deletions, .conflict { color: var(--removed-ink); }
.review-mark { flex: none; color: var(--muted); }
.review-mark.checked { color: var(--accent); }
.review-mark svg, .icon-button svg { display: block; width: 1rem; height: 1rem; stroke: currentColor; stroke-width: 1.4; }
.icon-button { padding: 0.23rem; }
.sr-only { position: absolute; clip-path: inset(50%); width: 1px; height: 1px; overflow: hidden; white-space: nowrap; }

.app-bar button { font-size: 0.85rem; }
.file-stream { display: grid; align-content: start; gap: 0.65rem; padding: 0.65rem; overflow: auto; scrollbar-gutter: stable; }
.comparison-note { margin: 0; color: var(--muted); font-size: 0.8rem; }
.comparison-note code { margin-inline-start: 0.45rem; }
.file-stream:focus-visible { outline-offset: -2px; }
.diff-layout { display: flex; border: 1px solid var(--border); border-radius: 5px; }
.diff-layout button { background: transparent; border: 0; border-radius: 0; }
.diff-layout button + button { border-inline-start: 1px solid var(--border); }
.diff-layout button[aria-pressed="true"] { background: var(--selection); }
@media (max-width: 680px) {
  .app-bar-content, .workspace-actions { flex-wrap: wrap; }
}
@media (max-width: 520px) {
  main { --projects-width: max(8rem, 22vw); }
  .changes {
    --files-width: max(9rem, 34cqw);
    grid-template-columns: minmax(0, 1fr) max(9rem, 34%);
  }
  .file-row { padding-inline: 0.22rem; gap: 0.24rem; }
  .line-counts { gap: 0.16rem; }
}
</style>
