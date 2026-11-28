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

<section class="review-file min-w-0 overflow-clip rounded-[7px] border border-divider bg-canvas [scroll-margin-block-start:0.65rem]"
  id={`file-review-${ index }`} aria-labelledby={`file-name-${ index }`} bind:this={section}>
  <header class="file-header flex min-h-10 flex-wrap items-center gap-[0.45rem] bg-surface px-[0.6rem] py-[0.35rem]">
    {#if reviewed !== null}
      <input class="m-0 size-4 flex-none cursor-pointer aria-disabled:cursor-default aria-disabled:opacity-[0.58]"
        type="checkbox" checked={reviewed} aria-disabled={busy}
        aria-label={`Reviewed: ${ name }`}
        title={reviewed ? "Clear review mark" : "Mark as reviewed"} onclick={(event) => {
          event.preventDefault();
          if (!busy) onreview();
        }} />
    {/if}
    <div class="min-w-0 flex-[1_1_7rem]">
      <div class="file-title flex items-center gap-[0.45rem]">
        <h2 class="m-0 min-w-0 text-[0.95rem]" id={`file-name-${ index }`}>
          <button class="control group box-border flex max-w-full min-w-0 cursor-pointer items-center gap-1 border-transparent bg-transparent p-[0.2rem] font-[640] enabled:hover:bg-toolbar"
            type="button" aria-controls={`file-content-${ index }`} aria-expanded={expanded}
            title={file.path} onclick={() => expansionOverride = !expanded}>
            <svg class="size-[0.9rem] flex-none stroke-muted stroke-[1.4] group-aria-expanded:rotate-90" aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="m6 4 4 4-4 4" /></svg>
            <span class="truncate">{name}</span>
          </button>
        </h2>
        {#if sizeChange !== null}
          <span class={[
            "size-change flex-none font-mono text-[0.8rem]/[normal] whitespace-nowrap",
            sizeChange > 0 && "text-added-ink",
            sizeChange < 0 && "text-removed-ink",
          ]}
            title={`Net file size change: ${ formatSizeChange(sizeChange, true) }`}>
            {formatSizeChange(sizeChange)}
          </span>
        {/if}
      </div>
      {#if file.previousPath || file.path !== name}
        <p class="m-0 truncate ps-[1.4rem] text-[0.8rem] text-muted" title={file.previousPath ? `${ file.previousPath } → ${ file.path }` : file.path}>
          {#if file.previousPath}{file.previousPath} → {/if}{file.path}
        </p>
      {/if}
    </div>
    {#snippet badge(text: string, conflict = false)}
      <span class={["rounded-[5px] border border-divider px-[0.38rem] py-[0.15rem] text-[0.75rem]", conflict && "text-removed-ink"]}>{text}</span>
    {/snippet}
    {#if generated}{@render badge(generatedCategory)}{/if}
    {#if patch}{@render badge(patch.side === "staged" ? "Staged" : "Unstaged")}{/if}
    {#if file.status === "U"}{@render badge("Conflicted", true)}{/if}
    {#if patch?.side === "unstaged"}
      {@const unstaged = patch}
      <button class={[
        "control workspace-button text-[0.85rem] enabled:hover:bg-toolbar",
        unstaged.reviewed && unstaged.status !== "U" && "border-accent bg-selection text-accent",
      ]}
        type="button" onclick={() => onstage(unstaged)} disabled={busy || !unstaged.reviewed || unstaged.status === "U"}>
        Stage file
      </button>
    {/if}
  </header>
  <div class="border-bs border-divider" id={`file-content-${ index }`} hidden={!expanded}>
    {#if expanded}
      {#if file.patch}
        <DiffView patch={file.patch} path={file.path} previousPath={file.previousPath} {layout}
          highlight={nearViewport} {windowRows} />
      {/if}
      {#if details && showImagePreviews}
        {#if previewsDiscarded}
          <div class="block-(--preview-height)" aria-hidden="true" style:--preview-height={`${ previewHeight }px`}></div>
        {:else}
          <div bind:this={preview}><FilePreviews {details} /></div>
        {/if}
      {:else if !file.patch && !detailsError}
        <p class="m-0 px-[0.8rem] py-[0.55rem] wrap-anywhere text-muted" role="status">{details ? "No text diff available." : "Loading file…"}</p>
      {/if}
      {#if detailsError}<p class="m-0 px-[0.8rem] py-[0.55rem] wrap-anywhere text-muted" role="status">{detailsError}</p>{/if}
    {/if}
  </div>
</section>
