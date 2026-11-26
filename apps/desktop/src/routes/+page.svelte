<script lang="ts">
import type { Comparison, ComparisonMode, Project, Registration, Revisions } from "../lib/revisions.js";
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";

const statusLabels: Record<string, string> = {
  A: "Added",
  D: "Deleted",
  M: "Modified",
  R: "Renamed",
  T: "Type changed",
};

let projects = $state<Project[]>([]);
let checkout = $state("");
let activeCheckout = $state("");
let revisions = $state<Revisions>();
let base = $state("");
let target = $state("HEAD");
let mode = $state<ComparisonMode>("between");
let comparison = $state<Comparison | null>(null);
let selectedIndex = $state(0);
let selectedFile = $derived(comparison?.files[selectedIndex]);
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

async function openWorktree(path: string): Promise<void> {
  revisions = await invoke<Revisions>("open_worktree", { checkout: path });
  activeCheckout = path;
  base = "";
  target = "HEAD";
  mode = "between";
  comparison = null;
}

function addProject(event: SubmitEvent): void {
  event.preventDefault();
  void run(async () => {
    const registration = await invoke<Registration>("register_project", { checkout });
    const { project } = registration;
    projects = [...projects.filter(item => item.id !== project.id), project]
      .toSorted((first, second) => first.id - second.id);
    checkout = "";
    await openWorktree(registration.checkout);
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
    selectedIndex = 0;
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
                onclick={() => run(() => openWorktree(worktree.path))}
                disabled={busy}>
                <strong>{worktree.branch ?? "Detached HEAD"}</strong>
                <small>{worktree.path}</small>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  </aside>

  <section class="workspace" aria-label="Revision comparison" aria-busy={busy}>
    {#if error}
      <p role="alert">{error}</p>
    {/if}

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

      {#if comparison}
        <dl class="endpoints">
          <dt>Effective start</dt><dd><code>{comparison.start}</code></dd>
          <dt>Effective end</dt><dd><code>{comparison.end}</code></dd>
        </dl>
        {#if selectedFile}
          <div class="changes">
            <nav aria-label="Changed files">
              {#each comparison.files as file, index (index)}
                <button type="button" aria-pressed={selectedIndex === index} onclick={() => selectedIndex = index}>
                  <small>{statusLabels[file.status] ?? file.status}</small>
                  {#if file.previousPath}
                    {file.previousPath} →
                  {/if}
                  {file.path}
                </button>
              {/each}
            </nav>
            <!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of long patch lines.) -->
            <pre aria-label="Unified patch" tabindex="0">{selectedFile.patch}</pre>
          </div>
        {:else}
          <p>No changes between these commits.</p>
        {/if}
      {/if}
    {:else}
      <p>Open a worktree to compare revisions.</p>
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
form, label, nav { display: grid; gap: 0.35rem; }
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
li button, nav button { text-align: start; overflow-wrap: anywhere; }
small { display: block; }

#compare {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 0.65rem;
}

#compare label { flex: 1 1 9rem; min-width: 0; }
.endpoints { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0.25rem 0.65rem; }
dd { margin: 0; overflow-wrap: anywhere; }

.changes {
  display: grid;
  grid-template-columns: minmax(8rem, 1fr) minmax(0, 3fr);
  align-items: start;
  gap: 0.65rem;
}

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
