<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { ComparisonMode, Revisions } from "./revisions.js";

interface Properties {
  revisions: Revisions;
  source: string;
  target: string;
  mode: ComparisonMode;
  busy: boolean;
  sourceOpen?: boolean;
  oncompare(source: string, target: string, mode: ComparisonMode): void;
}

let { revisions, source, target, mode, busy, sourceOpen = $bindable(false), oncompare }: Properties = $props();
let targetOpen = $state(false);
let query = $state("");
let targetQuery = $state("");
let picker: HTMLDivElement | undefined = $state();
let sourceInput: HTMLInputElement | undefined = $state();
let sourceTrigger: HTMLElement | undefined = $state();
let targetTrigger: HTMLElement | undefined = $state();

$effect(() => {
  if (sourceOpen) sourceInput?.focus();
});
function label(reference: string): string {
  return reference.replace(/^refs\/(heads|remotes)\//u, "");
}

const branches = $derived([
  ...revisions.branches.map(branch => `refs/heads/${ branch }`),
  ...revisions.remoteBranches.map(branch => `refs/remotes/${ branch }`),
]);
const search = $derived(query.trim().toLowerCase());
const filteredBranches = $derived(branches.filter(branch => label(branch).toLowerCase()
  .includes(search)));
const commits = $derived(revisions.commits.filter(({ id, subject }) => {
  const matchesSubject = subject.toLowerCase().includes(search);
  return id.toLowerCase().includes(search) || matchesSubject;
}));

function defaultMode(reference: string, destination: string): ComparisonMode {
  const defaultBranch = revisions.defaultBranch;
  const comparesDefault = reference === defaultBranch && reference !== revisions.currentBranch;
  const comparesRemote = reference.startsWith("refs/remotes/");
  return destination === "HEAD" && (comparesDefault || comparesRemote) ? "divergence" : "between";
}

function closeSource(): void {
  sourceOpen = false;
  sourceTrigger?.focus();
}

function chooseSource(reference: string): void {
  if (!reference || busy) return;

  const branch = branches.find(candidate => candidate === reference || label(candidate) === reference);
  const selected = branch ?? reference;
  closeSource();
  query = "";
  oncompare(selected, target, defaultMode(selected, target));
}

function chooseTarget(destination: string): void {
  targetOpen = false;
  targetTrigger?.focus();
  if (busy || destination === target) return;

  if (destination === "Working tree") {
    oncompare("HEAD", destination, "between");
    return;
  }

  const defaultBranch = revisions.defaultBranch;
  const featureBase = defaultBranch && defaultBranch !== revisions.currentBranch ? defaultBranch : "";
  const nextSource = target === "Working tree" && destination === "HEAD" && source === "HEAD" ? featureBase : source;
  oncompare(nextSource, destination, defaultMode(nextSource, destination));
}

function submit(event: SubmitEvent): void {
  event.preventDefault();
  const reference = query.trim();
  if (!reference) return;

  const exactBranch = branches.some(branch => branch === reference || label(branch) === reference);
  if (exactBranch || reference === "HEAD") chooseSource(reference);
  else chooseSource(filteredBranches[0] ?? commits[0]?.id ?? reference);
}
</script>

<svelte:window onpointerdown={(event) => {
  if (event.target instanceof Node && picker?.contains(event.target)) return;
  sourceOpen = false;
  targetOpen = false;
}} onkeydown={(event) => {
  if (event.key !== "Escape") return;
  if (sourceOpen) closeSource();
  if (targetOpen) {
    targetOpen = false;
    targetTrigger?.focus();
  }
}} />

<div class="revision-picker" bind:this={picker}>
  <details name="workspace-menu" class="source" bind:open={sourceOpen}>
    <summary bind:this={sourceTrigger} aria-label={source ? "Source " + label(source) : "Choose source"} title={label(source) || "Choose source"}>
      <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M4 5v6m0-3c0-3 7 0 7-3" /><circle cx="4" cy="3.5" r="1.5" /><circle cx="4" cy="12.5" r="1.5" /><circle cx="11" cy="3.5" r="1.5" /></svg>
      <span>{label(source) || "Choose source"}</span>
      <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="m5 6 3 3 3-3"/></svg>
    </summary>
    <div class="panel">
      <form id="compare" onsubmit={submit}>
        <input id="base" bind:this={sourceInput} aria-label="Search branches or commits" placeholder="Branch or commit" bind:value={query} disabled={busy} />
        <button type="submit" disabled={busy || !query.trim()}>Compare</button>
      </form>
      {#if !search || "head".includes(search)}
        <button class="option" type="button" onclick={() => chooseSource("HEAD")} disabled={busy}>HEAD</button>
      {/if}
      {#if filteredBranches.length}
        <p>Branches</p>
        {#each filteredBranches as branch (branch)}
          <button class="option" type="button" aria-current={source === branch} onclick={() => chooseSource(branch)} disabled={busy}>
            {label(branch)}{#if revisions.defaultBranch === branch}<small>Default</small>{/if}
          </button>
        {/each}
      {/if}
      {#if commits.length}
        <p>Recent commits</p>
        {#each commits as commit (commit.id)}
          <button class="option" type="button" aria-current={source === commit.id} onclick={() => chooseSource(commit.id)} disabled={busy}>
            <code>{commit.id.slice(0, 7)}</code> <span>{commit.subject}</span>
          </button>
        {/each}
      {/if}
      {#if source && target !== "Working tree"}
        <button class="option mode" type="button" disabled={busy} onclick={() => {
          closeSource();
          oncompare(source, target, mode === "between" ? "divergence" : "between");
        }}>{mode === "between" ? "Compare since divergence" : "Compare branch tips"}</button>
      {/if}
    </div>
  </details>
  <svg class="direction" aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M13 8H3m4-4L3 8l4 4"/></svg>
  <details name="workspace-menu" class="target" bind:open={targetOpen}>
    <summary bind:this={targetTrigger} aria-label={"Target " + label(target)} title={label(target)}>
      <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="3"/><path d="M1 8h4m6 0h4"/></svg>
      <span>{label(target)}</span>
      <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="m5 6 3 3 3-3"/></svg>
    </summary>
    <div class="panel">
      <button class="option" type="button" aria-current={target === "HEAD"} onclick={() => chooseTarget("HEAD")} disabled={busy}>HEAD <small>Current commit</small></button>
      <button class="option" type="button" aria-current={target === "Working tree"} onclick={() => chooseTarget("Working tree")} disabled={busy}>Working tree <small>Local edits</small></button>
      <form class="other-target" onsubmit={(event) => {
        event.preventDefault();
        const destination = targetQuery.trim();
        if (!destination) return;
        targetQuery = "";
        chooseTarget(destination);
      }}>
        <input aria-label="Target revision" placeholder="Other branch or commit" bind:value={targetQuery} disabled={busy}/>
        <button type="submit" disabled={busy || !targetQuery.trim()}>Use</button>
      </form>
    </div>
  </details>
</div>

<style>
.revision-picker { display: flex; flex: 0 1 auto; align-items: center; gap: 0.35rem; min-width: 0; }
details { min-width: 0; position: relative; }
summary, button, input {
  font: inherit;
  color: inherit;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: var(--canvas);
}
summary {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  padding: 0.3rem 0.4rem;
  cursor: pointer;
  white-space: nowrap;
}
summary::-webkit-details-marker { display: none; }
summary span, .option span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
summary span { max-width: min(10rem, 16vw); }
summary:hover, details[open] summary, button:hover:not(:disabled), .option[aria-current="true"] { background: var(--selection); }
svg { width: 0.85rem; height: 0.85rem; flex: none; stroke: var(--muted); stroke-width: 1.3; stroke-linecap: round; stroke-linejoin: round; }
.panel {
  position: absolute;
  inset: calc(100% + 0.3rem) auto auto 0;
  z-index: 3;
  width: min(20rem, 80vw);
  max-height: 60vh;
  overflow: auto;
  padding: 0.4rem;
  border: 1px solid var(--border);
  border-radius: 7px;
  background: var(--surface);
  box-shadow: 0 6px 16px #0002;
}
.target .panel { inset-inline: auto 0; width: min(13rem, 70vw); }
form { display: flex; align-items: center; gap: 0.3rem; margin: 0 0 0.35rem; }
.other-target { margin: 0.3rem 0 0; }
input { flex: 1; min-width: 0; padding: 0.3rem 0.4rem; }
button { padding: 0.3rem 0.4rem; cursor: pointer; }
button:disabled { opacity: 0.55; cursor: default; }
.option { display: flex; width: 100%; gap: 0.4rem; align-items: center; text-align: start; border-color: transparent; background: transparent; overflow-wrap: anywhere; }
.option small { margin-inline-start: auto; color: var(--muted); white-space: nowrap; }
.option.mode { border-top-color: var(--border); }
p { margin: 0.4rem 0.4rem 0.1rem; font-size: 0.78rem; color: var(--muted); }
</style>
