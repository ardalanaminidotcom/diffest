<script lang="ts" module>
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { ChangedFile, Comparison, WorktreePatch } from "./revisions.js";

interface ReviewEntry {
  id: string;
  index: number;
  file: ChangedFile;
  patch: WorktreePatch | undefined;
  category: string | null | undefined;
  reviewed: boolean;
  collapsed: boolean;
}

interface ReviewSlots {
  prefix: HTMLElement;
  metadata: HTMLElement;
  body: HTMLElement;
}

export type { ReviewEntry, ReviewSlots };
</script>

<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import { invoke } from "@tauri-apps/api/core";
import { untrack } from "svelte";
import { generatedCategory } from "./category-settings.js";
import {
  type FileDetails,
  type FileDetailsSource,
  fileName,
  formatSizeChange,
  isImageVersion,
} from "./file-details.js";
import FilePreviews from "./file-previews.svelte";

interface Properties {
  entry: ReviewEntry;
  host: HTMLElement;
  slots: ReviewSlots;
  checkout: string;
  comparison: Comparison | null;
  modes: string;
  busy: boolean;
  ontoggle(): void;
  onreview(index: number): void;
  onstage(patch: WorktreePatch): void;
}

const { entry, host, slots, checkout, comparison, modes, busy, ontoggle, onreview, onstage }: Properties = $props();
const { index, file, patch, category, reviewed, collapsed } = $derived(entry);
const name = $derived(fileName(file.path));
let details = $state.raw<FileDetails | null>(null);
let detailsError = $state("");
const showImagePreviews = $derived(details && (isImageVersion(details.before) || isImageVersion(details.after)));
const sizeChange = $derived.by(() => {
  if (!details || !(details.before || details.after)) return null;
  const before = details.before === null ? 0 : details.before.byteSize;
  const after = details.after === null ? 0 : details.after.byteSize;
  return before != null && after != null ? after - before : null;
});

$effect(() => {
  host.id = `file-review-${ index }`;
  host.setAttribute("role", "group");
  host.setAttribute("aria-label", file.path);
});

$effect(() => {
  if (category === null || untrack(() => details !== null)) return;
  const paths = {
    path             : file.path,
    previousPath     : file.previousPath,
    pathBytes        : file.pathBytes,
    previousPathBytes: file.previousPathBytes,
  };
  const source: FileDetailsSource | null = patch
    ? {
        kind             : "worktree",
        fingerprint      : patch.fingerprint,
        ...paths,
        pathBytes        : paths.pathBytes ?? null,
        previousPathBytes: paths.previousPathBytes ?? null,
      }
    : comparison && {
      kind  : "comparison",
      start : comparison.start,
      end   : comparison.end,
      status: file.status,
      ...paths,
    };
  if (!source) return;

  let current = true;
  detailsError = "";
  void invoke<FileDetails>("read_file_details", {
    checkout,
    source,
  }).then(
    (result) => {
      if (current) details = result;
    },
    (error: unknown) => {
      if (current) detailsError = String(error);
    },
  );
  return () => {
    current = false;
  };
});

function portal(node: HTMLElement, target: HTMLElement): { destroy(): void; } {
  target.append(node);
  return { destroy: () => node.remove() };
}
</script>

<!-- Keep the Svelte event root in the CodeView item, which owns the three light-DOM slots. -->
<div hidden>
  <div class="flex items-center gap-1" use:portal={slots.prefix}>
    <h2 class="m-0">
      <button class="control group workspace-button border-transparent bg-transparent enabled:hover:bg-toolbar"
        data-review-disclosure type="button" aria-expanded={!collapsed} onclick={ontoggle}>
        <svg class="block size-[0.9rem] stroke-muted stroke-[1.4] group-aria-expanded:rotate-90" aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="m6 4 4 4-4 4" /></svg>
        <span class="sr-only">Toggle diff for {name}</span>
      </button>
    </h2>
    <input class="m-0 size-4 flex-none cursor-pointer aria-disabled:cursor-default aria-disabled:opacity-[0.58]"
      type="checkbox" checked={reviewed} aria-disabled={busy} aria-label={"Reviewed: " + name}
      title={reviewed ? "Clear review mark" : "Mark as reviewed"} onclick={(event) => {
        event.preventDefault();
        if (!busy) onreview(index);
      }} />
  </div>

  <div class="flex flex-wrap items-center gap-1 text-foreground" use:portal={slots.metadata}>
    {#if sizeChange !== null}
      <span class={["size-change font-mono whitespace-nowrap", sizeChange > 0 && "text-added-ink", sizeChange < 0 && "text-removed-ink"]}
        title={"Net file size change: " + formatSizeChange(sizeChange, true)}>{formatSizeChange(sizeChange)}</span>
    {/if}
    {#snippet badge(text: string)}
      <span class="rounded-[5px] border border-divider px-[0.38rem] py-[0.15rem] text-[0.75rem]">{text}</span>
    {/snippet}
    {#if detailsError}<p class="m-0 text-removed-ink" role="alert">{detailsError}</p>{/if}
    {#if category === generatedCategory}{@render badge(generatedCategory)}{/if}
    {#if patch}{@render badge(patch.side === "staged" ? "Staged" : "Unstaged")}{/if}
    {#if file.status === "U"}{@render badge("Conflicted")}{/if}
    {#if patch?.side === "unstaged"}
      {@const unstaged = patch}
      <button class="control workspace-button text-[0.85rem] enabled:hover:bg-toolbar"
        type="button" onclick={() => onstage(unstaged)} disabled={busy || !unstaged.reviewed || unstaged.status === "U"}>Stage file</button>
    {/if}
  </div>

  <div class="text-foreground" use:portal={slots.body}>
    {#if !collapsed}
      {#if modes}<pre class="m-0 p-2 font-diff text-muted" aria-label="File mode change">{modes}</pre>{/if}
      {#if details && showImagePreviews}
        <FilePreviews {details} />
      {:else if !file.patch && !detailsError}
        <p class="m-0 px-[0.8rem] py-[0.55rem] text-muted" role="status">{details ? "No text diff available." : "Loading file…"}</p>
      {/if}
    {/if}
  </div>
</div>
