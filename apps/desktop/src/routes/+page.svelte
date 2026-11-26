<script lang="ts">
import type { FileCategory } from "../lib/category-settings.js";
import type {
  CommitReview,
  Comparison,
  ComparisonMode,
  Project,
  Registration,
  Revisions,
  StageWorktreePatchResult,
  WorktreePatch,
} from "../lib/revisions.js";
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";
import CategorizedFileTree from "../lib/categorized-file-tree.svelte";
import CategorySettings from "../lib/category-settings.svelte";

const statusLabels: Record<string, string> = {
  A: "Added",
  D: "Deleted",
  M: "Modified",
  R: "Renamed",
  T: "Type changed",
  U: "Conflicted",
};

type ReviewProgress = Pick<CommitReview, "reviewedFiles"> & { id: number; };

let projects = $state<Project[]>([]);
let checkout = $state("");
let activeCheckout = $state("");
let activeProjectId = $state<number>();
let revisions = $state<Revisions>();
let base = $state("");
let target = $state("HEAD");
let mode = $state<ComparisonMode>("between");
let comparison = $state<Comparison | null>(null);
let review = $state<ReviewProgress | null>(null);
let worktreeReview = $state<WorktreePatch[] | null>(null);
let reviewedFiles = $derived(new Set(review?.reviewedFiles));
let selectedIndex = $state(0);
let files = $derived(worktreeReview ?? comparison?.files ?? []);
let selectedFile = $derived(files[selectedIndex]);
let selectedWorktreePatch = $derived(worktreeReview?.[selectedIndex]);
let mainCheckout = $derived(projects.find(project => project.id === activeProjectId)?.mainCheckout);
let paths = $derived(files.map(file => file.path));
let categories = $state<FileCategory[]>([]);
let error = $state("");
let busy = $state(false);

async function run(action: () => Promise<void>): Promise<void> {
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

async function openWorktree(path: string, projectId: number, ongoing = worktreeReview !== null): Promise<void> {
  const nextRevisions = await invoke<Revisions>("open_worktree", { checkout: path });
  const patches = ongoing ? await invoke<WorktreePatch[]>("open_worktree_review", { checkout: path }) : null;
  revisions = nextRevisions;
  activeCheckout = path;
  activeProjectId = projectId;
  base = "";
  target = "HEAD";
  mode = "between";
  comparison = null;
  review = null;
  worktreeReview = patches;
  selectedIndex = 0;
}

function addProject(event: SubmitEvent): void {
  event.preventDefault();
  void run(async () => {
    const registration = await invoke<Registration>("register_project", { checkout });
    const { project } = registration;
    projects = [...projects.filter(item => item.id !== project.id), project]
      .toSorted((first, second) => first.id - second.id);
    checkout = "";
    await openWorktree(registration.checkout, project.id);
  });
}

function compare(event: SubmitEvent): void {
  event.preventDefault();
  void run(async () => {
    comparison = await invoke<Comparison>("compare_revisions", {
      checkout: activeCheckout,
      base    : base.trim(),
      target  : target.trim(),
      mode,
    });
    review = null;
    worktreeReview = null;
    selectedIndex = 0;
  });
}

function reviewStatus(fileCount: number, reviewedCount: number): string {
  return fileCount === reviewedCount ? "Reviewed" : `${ reviewedCount } of ${ fileCount } reviewed`;
}

function updateReview(id: number, displayed: Comparison, marks: number[]): void {
  review = {
    id,
    reviewedFiles: marks,
  };
  const summary = {
    id,
    baseLabel    : displayed.baseLabel,
    targetLabel  : displayed.targetLabel,
    start        : displayed.start,
    end          : displayed.end,
    fileCount    : displayed.files.length,
    reviewedCount: marks.length,
  };
  projects = projects.map((project) => {
    if (project.id !== activeProjectId) return project;

    return {
      ...project,
      savedReviews: [...project.savedReviews.filter(item => item.id !== id), summary]
        .toSorted((first, second) => second.id - first.id),
    };
  });
}

function saveReview(displayed: Comparison): void {
  void run(async () => {
    const id = await invoke<number>("save_commit_review", {
      checkout  : activeCheckout,
      comparison: displayed,
    });
    updateReview(id, displayed, []);
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
    comparison = saved.comparison;
    base = comparison.baseLabel;
    target = comparison.targetLabel;
    mode = comparison.mode;
    worktreeReview = null;
    selectedIndex = 0;
    updateReview(id, comparison, saved.reviewedFiles);
  });
}

function markFile(id: number, displayed: Comparison, fileIndex: number): void {
  void run(async () => {
    const marks = await invoke<number[]>("set_commit_file_reviewed", {
      checkout: activeCheckout,
      id,
      fileIndex,
      reviewed: !reviewedFiles.has(fileIndex),
    });
    updateReview(id, displayed, marks);
  });
}

function showWorktreeReview(patches: WorktreePatch[], side = selectedWorktreePatch?.side): void {
  const path = selectedWorktreePatch?.path;
  const index = patches.findIndex(patch => patch.side === side && patch.path === path);
  selectedIndex = Math.max(0, index);
  worktreeReview = patches;
  comparison = null;
  review = null;
}

function refreshWorktreeReview(): void {
  void run(async () => {
    const patches = await invoke<WorktreePatch[]>("open_worktree_review", { checkout: activeCheckout });
    showWorktreeReview(patches);
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

<main>
  <aside aria-label="Projects">
    <h1>Diffest</h1>
    <form onsubmit={addProject}>
      <label for="checkout">Checkout folder</label>
      <input id="checkout" bind:value={checkout} placeholder="Local Git checkout path" required />
      <button disabled={busy}>Add checkout</button>
    </form>

    {#each projects as project (project.id)}
      <section aria-label={project.name}>
        <h2 title={project.mainCheckout}>{project.name}</h2>
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
        {#if project.savedReviews.length}
          <section aria-label="Saved commit reviews">
            <h3>Saved commit reviews</h3>
            <ul>
              {#each project.savedReviews as saved (saved.id)}
                <li>
                  <button
                    type="button"
                    aria-pressed={review?.id === saved.id}
                    onclick={() => openReview(project, saved.id)}
                    disabled={busy || !project.worktrees.length}>
                    {saved.baseLabel} → {saved.targetLabel}
                    <small><code>{saved.start.slice(0, 7)} → {saved.end.slice(0, 7)}</code></small>
                    <small>{reviewStatus(saved.fileCount, saved.reviewedCount)}</small>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      </section>
    {/each}
  </aside>

  <section class="workspace" aria-label="Review workspace" aria-busy={busy}>
    {#if error}
      <p role="alert">{error}</p>
    {/if}

    <CategorySettings {mainCheckout} {paths} bind:categories />

    {#if revisions}
      <form id="compare" onsubmit={compare}>
        <label for="base">Base
          <input id="base" bind:value={base} list="revisions" placeholder="Branch or commit" required />
        </label>
        <label for="target">Target
          <input id="target" bind:value={target} list="revisions" placeholder="Branch or commit" required />
        </label>
        <datalist id="revisions">
          {#each revisions.branches as branch (branch)}
            <option value={"refs/heads/" + branch}>{branch}</option>
          {/each}
          {#each revisions.commits as commit (commit.id)}
            <option value={commit.id}>{commit.subject}</option>
          {/each}
        </datalist>
        <label for="mode">Compare mode
          <select
            id="mode"
            value={mode}
            onchange={(event) => mode = event.currentTarget.value === "divergence" ? "divergence" : "between"}>
            <option value="between">Between selected revisions</option>
            <option value="divergence">Changes since divergence</option>
          </select>
        </label>
        <button disabled={busy}>Compare</button>
      </form>

      <button id="worktree-review" type="button" onclick={refreshWorktreeReview} disabled={busy}>
        {worktreeReview ? "Refresh uncommitted changes" : "Review uncommitted changes"}
      </button>

      {#if worktreeReview}
        {#if worktreeReview.length}
          <p id="review-progress" role="status">
            {reviewStatus(worktreeReview.length, worktreeReview.filter(patch => patch.reviewed).length)}
          </p>
        {/if}
      {:else if comparison}
        {@const displayed = comparison}
        <dl class="endpoints">
          <dt>Effective start</dt><dd><code>{comparison.start}</code></dd>
          <dt>Effective end</dt><dd><code>{comparison.end}</code></dd>
        </dl>
        {#if review}
          <p id="review-progress" role="status">{reviewStatus(comparison.files.length, review.reviewedFiles.length)}</p>
        {:else}
          <button id="save-review" type="button" onclick={() => saveReview(displayed)} disabled={busy}>Save review</button>
        {/if}
      {/if}

      {#if worktreeReview || comparison}
        {#if selectedFile}
          <div class="changes">
            <CategorizedFileTree {paths} {categories}>
              {#snippet entry(index)}
                {@const file = files[index]}
                {@const worktreePatch = worktreeReview?.[index]}
                <button type="button" aria-pressed={selectedIndex === index} onclick={() => selectedIndex = index}>
                  {#if worktreePatch}<small>{worktreePatch.side === "staged" ? "Staged" : "Unstaged"}</small>{/if}
                  <small>{statusLabels[file.status] ?? file.status}</small>
                  {#if file.previousPath}
                    {file.previousPath} →
                  {/if}
                  {file.path}
                  {#if worktreePatch || review}
                    <small>{(worktreePatch?.reviewed ?? reviewedFiles.has(index)) ? "Reviewed" : "Unreviewed"}</small>
                  {/if}
                </button>
              {/snippet}
            </CategorizedFileTree>
            <div class="file-detail">
              {#if selectedWorktreePatch}
                {@const patch = selectedWorktreePatch}
                <button id="review-file" type="button" onclick={() => markWorktreePatch(patch)} disabled={busy}>
                  {patch.reviewed ? "Clear review mark" : "Mark as reviewed"}
                </button>
                {#if patch.side === "unstaged"}
                  <button
                    id="stage-file"
                    type="button"
                    onclick={() => stageWorktreePatch(patch)}
                    disabled={busy || !patch.reviewed || patch.status === "U"}>
                    Stage file
                  </button>
                {/if}
              {:else if review && comparison}
                {@const current = review}
                {@const displayed = comparison}
                <button
                  id="review-file"
                  type="button"
                  onclick={() => markFile(current.id, displayed, selectedIndex)}
                  disabled={busy}>
                  {reviewedFiles.has(selectedIndex) ? "Clear review mark" : "Mark as reviewed"}
                </button>
              {/if}
              <!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of long patch lines.) -->
              <pre aria-label="Unified patch" tabindex="0">{selectedFile.patch}</pre>
            </div>
          </div>
        {:else}
          <p>{worktreeReview ? "No uncommitted changes." : "No changes between these commits."}</p>
        {/if}
      {/if}
    {:else}
      <p>Open a worktree to review changes or compare revisions.</p>
    {/if}
  </section>
</main>

<style>
:global(html) {
  color-scheme: light dark;
  background: Canvas;
  color: CanvasText;
  font-family: system-ui, sans-serif;
}

:global(body) { margin: 0; }

main {
  --border: color-mix(in srgb, CanvasText 20%, Canvas);
  --surface: color-mix(in srgb, CanvasText 5%, Canvas);
  display: grid;
  grid-template-columns: minmax(10rem, 14rem) minmax(0, 1fr);
  height: 100dvh;
}

aside, .workspace {
  overflow: auto;
  padding: 1rem;
}

aside {
  background: var(--surface);
  border-inline-end: 1px solid var(--border);
}

h1 { margin: 0 0 1rem; font-size: 1.5rem; }
h2 { margin: 1.25rem 0 0.25rem; font-size: 1rem; overflow-wrap: anywhere; }
h3 { margin: 0.75rem 0 0.25rem; font-size: 0.85rem; }
form, label { display: grid; gap: 0.35rem; }
label, small, .endpoints { font-size: 0.8rem; }
ul { list-style: none; padding: 0; margin: 0; }

input, select, button {
  min-width: 0;
  box-sizing: border-box;
  font: inherit;
  color: inherit;
  background: Canvas;
  border: 1px solid var(--border);
  border-radius: 0.35rem;
  padding: 0.45rem 0.55rem;
}

button:not(:disabled) { cursor: pointer; }
button[aria-pressed="true"] { background: color-mix(in srgb, AccentColor 18%, Canvas); border-color: AccentColor; }
li button { width: 100%; margin-block: 0.15rem; }
li button, .changes button { text-align: start; overflow-wrap: anywhere; }
small { display: block; }

#compare {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 0.65rem;
}

#compare label { flex: 1 1 9rem; min-width: 0; }
#worktree-review { margin-block-start: 0.65rem; }
.endpoints { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0.25rem 0.65rem; }
dd { margin: 0; overflow-wrap: anywhere; }

.changes {
  display: grid;
  grid-template-columns: minmax(8rem, 1fr) minmax(0, 3fr);
  align-items: start;
  gap: 0.65rem;
  margin-block-start: 0.65rem;
}

.file-detail { min-width: 0; }
.file-detail button { margin-block-end: 0.5rem; }

pre {
  margin: 0;
  padding: 0.75rem;
  overflow: auto;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 0.35rem;
  line-height: 1.45;
}

@media (max-width: 650px) {
  .changes { grid-template-columns: minmax(0, 1fr); }
}
</style>
