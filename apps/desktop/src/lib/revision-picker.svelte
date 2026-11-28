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
  fetchStatus: string;
  onfetch(): void;
  oncompare(source: string, target: string, mode: ComparisonMode): void;
}

let {
  revisions, source, target, mode, busy, sourceOpen = $bindable(false), fetchStatus, oncompare, onfetch,
}: Properties = $props();
let targetOpen = $state(false);
let query = $state("");
let targetQuery = $state("");
let picker: HTMLDivElement | undefined = $state();
let targetInput: HTMLInputElement | undefined = $state();
let sourceInput: HTMLInputElement | undefined = $state();
let sourceTrigger: HTMLElement | undefined = $state();
let targetTrigger: HTMLElement | undefined = $state();

$effect(() => {
  if (sourceOpen) sourceInput?.focus();
  if (targetOpen) targetInput?.focus();
});
function label(reference: string): string {
  return reference.replace(/^refs\/(heads|remotes)\//u, "");
}

const branches = $derived([
  ...revisions.branches.map(branch => `refs/heads/${ branch }`),
  ...revisions.remoteBranches.map(branch => `refs/remotes/${ branch }`),
]);
const search = $derived(query.trim().toLowerCase());
const targetSearch = $derived(targetQuery.trim().toLowerCase());

function matchingBranches(term: string): string[] {
  return branches.filter(branch => label(branch).toLowerCase()
    .includes(term));
}

function matchingCommits(term: string): Revisions["commits"] {
  return revisions.commits.filter(({ id, subject }) => {
    const matchesSubject = subject.toLowerCase().includes(term);
    return id.toLowerCase().includes(term) || matchesSubject;
  });
}

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

function matchingReference(reference: string): string | null {
  const branch = branches.find(candidate => candidate === reference || label(candidate) === reference);
  if (branch) return branch;

  const synced = revisions.syncedRemoteBranches?.find(name => name === reference || `refs/remotes/${ name }` === reference);
  if (synced) return `refs/remotes/${ synced }`;

  const tag = revisions.tags?.find(name => name === reference || `refs/tags/${ name }` === reference);
  return tag ? `refs/tags/${ tag }` : null;
}

function chooseSource(reference: string): void {
  if (!reference || busy) return;

  const selected = matchingReference(reference) ?? reference;
  closeSource();
  query = "";
  oncompare(selected, target, defaultMode(selected, target));
}

function chooseTarget(destination: string): void {
  targetQuery = "";
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

function submitSource(event: SubmitEvent): void {
  event.preventDefault();
  const reference = matchingReference(query) ?? query.trim();
  if (!reference) return;

  if (matchingReference(reference) || reference === "HEAD" || reference.startsWith("refs/")) {
    chooseSource(reference);
    return;
  }
  const term = reference.toLowerCase();
  chooseSource(matchingBranches(term)[0] ?? matchingCommits(term)[0]?.id ?? reference);
}

const revisionButton = `control cursor-pointer px-[0.4rem] py-[0.3rem] enabled:hover:bg-selection
  disabled:cursor-default disabled:opacity-[0.55]`;
const optionButton = `${ revisionButton } flex w-full items-center gap-[0.4rem] border-transparent bg-transparent
  text-start wrap-anywhere aria-[current=true]:bg-selection`;
const optionNote = "ms-auto whitespace-nowrap text-muted";
const menuLabel = "mx-[0.4rem] mt-[0.4rem] mb-[0.1rem] text-[0.78rem] text-muted";
const trigger = `control flex cursor-pointer items-center gap-[0.3rem] px-[0.4rem] py-[0.3rem] whitespace-nowrap
  group-open:bg-selection hover:bg-selection [&::-webkit-details-marker]:hidden`;
const triggerLabel = "max-w-[min(10rem,16vw)] min-w-0 truncate";
const icon = "size-[0.85rem] flex-none stroke-muted stroke-[1.3] [stroke-linecap:round] [stroke-linejoin:round]";
const panel = `absolute top-[calc(100%+0.3rem)] z-3 max-h-[60vh] w-[min(20rem,80vw)] overflow-auto
  rounded-[7px] border border-divider bg-surface p-[0.4rem] shadow-[0_6px_16px_#0002]`;
const searchForm = "m-0 mb-[0.35rem] flex items-center gap-[0.3rem]";
const searchInput = "control min-w-0 flex-1 px-[0.4rem] py-[0.3rem]";
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

{#snippet results(term: string, selected: string, choose: (reference: string) => void)}
  {@const filteredBranches = matchingBranches(term)}
  {@const commits = matchingCommits(term)}
  {#if filteredBranches.length}
    <p class={menuLabel}>Branches</p>
    {#each filteredBranches as branch (branch)}
      <button class={optionButton} type="button" title={label(branch)} aria-current={selected === branch}
        onclick={() => choose(branch)} disabled={busy}>
        {label(branch)}{#if revisions.defaultBranch === branch}<small class={optionNote}>Default</small>{/if}
      </button>
    {/each}
  {/if}
  {#if commits.length}
    <p class={menuLabel}>Recent commits</p>
    {#each commits as commit (commit.id)}
      <button class={optionButton} type="button" aria-current={selected === commit.id} onclick={() => choose(commit.id)} disabled={busy}>
        <code>{commit.id.slice(0, 7)}</code> <span class="min-w-0 truncate">{commit.subject}</span>
      </button>
    {/each}
  {/if}
{/snippet}

{#snippet fetchControl()}
  <div class="sticky bottom-0 flex items-center justify-between gap-[0.4rem] border-t border-divider bg-surface pt-[0.35rem]">
    <p class="m-0 text-[0.78rem] wrap-anywhere text-muted" role="status">{fetchStatus}</p>
    <button class={[revisionButton, "flex-none"]} type="button" onclick={onfetch} disabled={busy}>Fetch branches</button>
  </div>
{/snippet}

<div class="revision-picker flex min-w-0 shrink items-center gap-[0.35rem]" bind:this={picker}>
  <details name="workspace-menu" class="source group relative min-w-0" bind:open={sourceOpen}>
    <summary class={trigger} bind:this={sourceTrigger} aria-label={source ? "Source " + label(source) : "Choose source"} title={label(source) || "Choose source"}>
      <svg class={icon} aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M4 5v6m0-3c0-3 7 0 7-3" /><circle cx="4" cy="3.5" r="1.5" /><circle cx="4" cy="12.5" r="1.5" /><circle cx="11" cy="3.5" r="1.5" /></svg>
      <span class={triggerLabel}>{label(source) || "Choose source"}</span>
      <svg class={icon} aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="m5 6 3 3 3-3"/></svg>
    </summary>
    <div class={[panel, "left-0"]}>
      <form class={searchForm} id="compare" onsubmit={submitSource}>
        <input class={searchInput} id="base" bind:this={sourceInput} aria-label="Search branches or commits" placeholder="Branch or commit" bind:value={query} disabled={busy} />
        <button class={revisionButton} type="submit" disabled={busy || !query.trim()}>Compare</button>
      </form>
      {#if !search || "head".includes(search)}
        <button class={optionButton} type="button" onclick={() => chooseSource("HEAD")} disabled={busy}>HEAD</button>
      {/if}
      {@render results(search, source, chooseSource)}
      {#if source && target !== "Working tree"}
        <button class={[optionButton, "mode border-t-divider"]} type="button" disabled={busy} onclick={() => {
          closeSource();
          oncompare(source, target, mode === "between" ? "divergence" : "between");
        }}>{mode === "between" ? "Compare since divergence" : "Compare branch tips"}</button>
      {/if}
      {@render fetchControl()}
    </div>
  </details>
  <svg class={icon} aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M13 8H3m4-4L3 8l4 4"/></svg>
  <details name="workspace-menu" class="target group relative min-w-0" bind:open={targetOpen}>
    <summary class={trigger} bind:this={targetTrigger} aria-label={"Target " + label(target)} title={label(target)}>
      <svg class={icon} aria-hidden="true" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="3"/><path d="M1 8h4m6 0h4"/></svg>
      <span class={triggerLabel}>{label(target)}</span>
      <svg class={icon} aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="m5 6 3 3 3-3"/></svg>
    </summary>
    <div class={[panel, "end-0"]}>
      <form class={searchForm} onsubmit={(event) => {
        event.preventDefault();
        const reference = matchingReference(targetQuery) ?? targetQuery.trim();
        if (reference) chooseTarget(matchingReference(reference) ?? reference);
      }}>
        <input class={searchInput} aria-label="Target revision" placeholder="Branch or commit" bind:this={targetInput}
          bind:value={targetQuery} disabled={busy} />
        <button class={revisionButton} type="submit" disabled={busy || !targetQuery.trim()}>Use</button>
      </form>
      {#if "head".includes(targetSearch)}
        <button class={optionButton} type="button" aria-current={target === "HEAD"} onclick={() => chooseTarget("HEAD")}
          disabled={busy}>HEAD <small class={optionNote}>Current commit</small></button>
      {/if}
      {#if "working tree".includes(targetSearch)}
        <button class={optionButton} type="button" title="Working tree" aria-current={target === "Working tree"}
          onclick={() => chooseTarget("Working tree")}
          disabled={busy}>Working tree <small class={optionNote}>Local edits</small></button>
      {/if}
      {@render results(targetSearch, target, chooseTarget)}
      {@render fetchControl()}
    </div>
  </details>
</div>
