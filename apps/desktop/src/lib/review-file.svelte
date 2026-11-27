<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { ChangedFile, Comparison, DiffLayout, WorktreePatch } from "./revisions.js";
import { invoke } from "@tauri-apps/api/core";
import { untrack } from "svelte";
import { generatedCategory } from "./category-settings.js";
import DiffView from "./diff-view.svelte";
import {
  type FileDetails,
  type FileDetailsSource,
  type FileVersion,
  fileName,
  formatSizeChange,
  isImageVersion,
} from "./file-details.js";
import FilePreviews from "./file-previews.svelte";

interface Properties {
  index: number;
  file: ChangedFile;
  patch: WorktreePatch | undefined;
  checkout: string;
  comparison: Comparison | null;
  layout: DiffLayout;
  windowRows: boolean;
  category?: string | null;
  reviewed: boolean | null;
  busy: boolean;
  onreview(): void;
  onstage(patch: WorktreePatch): void;
}

let {
  index, file, patch, checkout, comparison, layout, windowRows, category, reviewed, busy, onreview, onstage,
}: Properties = $props();
const isReviewed = $derived(reviewed === true);
const generated = $derived(category === generatedCategory);
let expansionOverride = $state<boolean | null>(null);
const expanded = $derived(category !== null && (expansionOverride ?? (!generated && !isReviewed)));
let wasReviewed = false;
$effect.pre(() => {
  if (wasReviewed !== isReviewed) expansionOverride = isReviewed ? null : true;
  wasReviewed = isReviewed;
});
const name = $derived(fileName(file.path));
let section: HTMLElement | undefined = $state();
let preview: HTMLDivElement | undefined = $state();
let nearViewport = $state(false);
let previewHeight = $state(0);
let details = $state.raw<FileDetails | null>(null);
let detailsError = $state("");
let previewsDiscarded = $state(false);
const showImagePreviews = $derived(details && (isImageVersion(details.before) || isImageVersion(details.after)));
const sizeChange = $derived.by(() => {
  if (!details) return null;
  if (!details.before && !details.after) return null;

  const before = details.before === null ? 0 : details.before.byteSize;
  const after = details.after === null ? 0 : details.after.byteSize;
  return before != null && after != null ? after - before : null;
});

$effect(() => {
  const fileSection = section;
  const stream = fileSection?.parentElement;
  if (!fileSection || !stream) return;

  const observer = new globalThis.IntersectionObserver((entries) => {
    nearViewport = entries.at(-1)?.isIntersecting === true;
  }, {
    root      : stream,
    rootMargin: "240px",
  });
  observer.observe(fileSection);
  return () => observer.disconnect();
});

function withoutPreview(version: FileVersion | null): FileVersion | null {
  if (!version?.preview) return version;

  return {
    ...version,
    preview: null,
  };
}

function discardPreviews(): void {
  if (!details || !(details.before?.preview || details.after?.preview)) return;

  if (preview?.clientHeight) previewHeight = preview.getBoundingClientRect().height;
  details = {
    before: withoutPreview(details.before),
    after : withoutPreview(details.after),
  };
  previewsDiscarded = true;
}

$effect(() => {
  if (!nearViewport || !expanded) untrack(discardPreviews);
});

// Each section is keyed to its patch or comparison file, so cached sizes survive review mark changes.
$effect(() => {
  if (!nearViewport || category === null) return;
  const loadPreviews = expanded;
  if (untrack(() => details && (!loadPreviews || !previewsDiscarded))) return;

  const sourceFile = patch ?? file;
  const paths = {
    path             : sourceFile.path,
    previousPath     : sourceFile.previousPath,
    pathBytes        : sourceFile.pathBytes,
    previousPathBytes: sourceFile.previousPathBytes,
  };
  const source: FileDetailsSource | null = patch
    ? {
        kind       : "worktree",
        fingerprint: patch.fingerprint,
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
      if (!current) return;

      details = result;
      previewsDiscarded = false;
      if (!loadPreviews) discardPreviews();
    },
    (error: unknown) => {
      if (current) detailsError = String(error);
    },
  );
  return () => {
    current = false;
  };
});
</script>

<section class="review-file" id={`file-review-${ index }`} aria-labelledby={`file-name-${ index }`} bind:this={section}>
  <header class="file-header">
    {#if reviewed !== null}
      <input type="checkbox" checked={reviewed} aria-disabled={busy}
        aria-label={`Reviewed: ${ name }`}
        title={reviewed ? "Clear review mark" : "Mark as reviewed"} onclick={(event) => {
          event.preventDefault();
          if (!busy) onreview();
        }} />
    {/if}
    <div class="file-heading">
      <div class="file-title">
        <h2 id={`file-name-${ index }`}>
          <button type="button" aria-controls={`file-content-${ index }`} aria-expanded={expanded}
            title={file.path} onclick={() => expansionOverride = !expanded}>
            <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="m6 4 4 4-4 4" /></svg>
            <span>{name}</span>
          </button>
        </h2>
        {#if sizeChange !== null}
          <span class="size-change" class:additions={sizeChange > 0} class:deletions={sizeChange < 0}
            title={`Net file size change: ${ formatSizeChange(sizeChange, true) }`}>
            {formatSizeChange(sizeChange)}
          </span>
        {/if}
      </div>
      {#if file.previousPath || file.path !== name}
        <p title={file.previousPath ? `${ file.previousPath } → ${ file.path }` : file.path}>
          {#if file.previousPath}{file.previousPath} → {/if}{file.path}
        </p>
      {/if}
    </div>
    {#if generated}<span class="patch-side">{generatedCategory}</span>{/if}
    {#if patch}<span class="patch-side">{patch.side === "staged" ? "Staged" : "Unstaged"}</span>{/if}
    {#if file.status === "U"}<span class="patch-side deletions">Conflicted</span>{/if}
    {#if patch?.side === "unstaged"}
      {@const unstaged = patch}
      <button class="stage-file" class:primary={unstaged.reviewed && unstaged.status !== "U"}
        type="button" onclick={() => onstage(unstaged)} disabled={busy || !unstaged.reviewed || unstaged.status === "U"}>
        Stage file
      </button>
    {/if}
  </header>
  <div class="file-content" id={`file-content-${ index }`} hidden={!expanded}>
    {#if expanded}
      {#if file.patch}
        <DiffView patch={file.patch} path={file.path} previousPath={file.previousPath} {layout}
          highlight={nearViewport} {windowRows} />
      {/if}
      {#if details && showImagePreviews}
        {#if previewsDiscarded}
          <div aria-hidden="true" style:block-size={`${ previewHeight }px`}></div>
        {:else}
          <div bind:this={preview}><FilePreviews {details} /></div>
        {/if}
      {:else if !file.patch && !detailsError}
        <p class="patch-note" role="status">{details ? "No text diff available." : "Loading file…"}</p>
      {/if}
      {#if detailsError}<p class="patch-note" role="status">{detailsError}</p>{/if}
    {/if}
  </div>
</section>

<style>
.review-file {
  min-width: 0;
  overflow: clip;
  scroll-margin-block-start: 0.65rem;
  border: 1px solid var(--border);
  border-radius: 7px;
  background: var(--canvas);
}
.file-header { display: flex; align-items: center; flex-wrap: wrap; gap: 0.45rem; min-height: 2.5rem; padding: 0.35rem 0.6rem; background: var(--surface); }
.file-heading { flex: 1 1 7rem; min-width: 0; }
.file-title { display: flex; align-items: center; gap: 0.45rem; }
h2, p { margin: 0; }
h2 { min-width: 0; font-size: 0.95rem; }
button { min-width: 0; box-sizing: border-box; font: inherit; color: inherit; border: 1px solid var(--border); border-radius: 5px; cursor: pointer; }
button:not(:disabled):hover { background: var(--toolbar); }
h2 button {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  max-width: 100%;
  padding: 0.2rem;
  border-color: transparent;
  background: transparent;
  font-weight: 640;
}
h2 span, .file-heading p { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
h2 svg { flex: none; width: 0.9rem; height: 0.9rem; stroke: var(--muted); stroke-width: 1.4; }
h2 button[aria-expanded="true"] svg { transform: rotate(90deg); }
input { flex: none; width: 1rem; height: 1rem; margin: 0; cursor: pointer; }
input[aria-disabled="true"] { cursor: default; opacity: 0.58; }
.file-heading p { padding-inline-start: 1.4rem; color: var(--muted); font-size: 0.8rem; }
.size-change { flex: none; font: 0.8rem ui-monospace, monospace; white-space: nowrap; }
.additions { color: var(--added-ink); }
.deletions { color: var(--removed-ink); }
.patch-side { border: 1px solid var(--border); border-radius: 5px; padding: 0.15rem 0.38rem; font-size: 0.75rem; }
.stage-file { padding: 0.3rem 0.52rem; background: var(--canvas); font-size: 0.85rem; }
.stage-file:disabled { cursor: default; opacity: 0.58; }
.stage-file.primary { color: var(--accent); border-color: var(--accent); background: var(--selection); }
.file-content { border-block-start: 1px solid var(--border); }
.patch-note { padding: 0.55rem 0.8rem; color: var(--muted); overflow-wrap: anywhere; }
</style>
