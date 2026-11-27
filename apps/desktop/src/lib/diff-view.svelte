<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { DiffLayout } from "./revisions.js";
import { renderDiffRows, renderRawBlocks } from "./diff-rows.js";
import { mountDiffRows } from "./mount-diff-rows.js";
import { omitHunkHeaders, parsePatch } from "./patch.js";

interface Properties {
  patch: string;
  path: string;
  previousPath: string | null;
  layout: DiffLayout;
  highlight: boolean;
  windowRows: boolean;
}

function languageFor(filename: string): string {
  const basename = filename.slice(filename.lastIndexOf("/") + 1).toLowerCase();
  if (basename === "dockerfile") return "dockerfile";
  if (basename === "makefile") return "makefile";
  const extension = basename.includes(".") ? basename.split(".").at(-1) ?? "" : "";
  return extension === "svelte" || extension === "vue" ? "xml" : extension;
}

let { patch, path, previousPath, layout, highlight, windowRows }: Properties = $props();
const parsed = $derived(parsePatch(patch));
const raw = $derived(parsed.hunks.length === 0);
const blocks = $derived.by(() => {
  if (!raw) return renderDiffRows(parsed.hunks.map(hunk => hunk.lines), layout, windowRows);
  return windowRows && patch ? renderRawBlocks(omitHunkHeaders(patch)) : [];
});
const beforeLanguage = $derived(languageFor(previousPath ?? path));
const afterLanguage = $derived(languageFor(path));
const highlightPatchLimit = 100_000;
</script>

{#if parsed.tooManyLines}<p class="raw-notice">Large patch · Showing raw text</p>{/if}
{#if blocks.length}
  <div class="patch-rows" class:raw-rows={raw} use:mountDiffRows={{
    blocks, windowRows, beforeLanguage, afterLanguage, highlight: highlight && patch.length <= highlightPatchLimit,
  }}></div>
{:else if patch}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of long patch lines.) -->
  <pre aria-label="Unified patch" tabindex="0">{omitHunkHeaders(patch)}</pre>
{/if}

<style>
.patch-rows { position: relative; }
.patch-rows :global(table) { width: 100%; table-layout: fixed; border-collapse: separate; border-spacing: 0; }
.patch-rows :global(th), .patch-rows :global(td) { padding: 0; text-align: start; vertical-align: top; }
.patch-rows :global(thead th) {
  padding: 0.35rem 0.75rem;
  color: var(--muted);
  font-size: 0.72rem;
  font-weight: 600;
}
.patch-rows :global(.split th + th), .patch-rows :global(.split td:nth-child(3)) { border-inline-start: 1px solid var(--border); }
.patch-rows :global(.block-heading) { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
.patch-rows :global(.hunk-start td) { border-block-start: 1px solid var(--border); }
pre, .patch-rows :global(pre), .patch-rows :global(table) { font: 0.875rem/1.55 ui-monospace, "SF Mono", Consolas, monospace; }
.patch-rows :global(code) { font: inherit; white-space: pre-wrap; overflow-wrap: anywhere; tab-size: 4; }
.patch-rows :global(.number) { width: 3.75em; }
.patch-rows :global(.line-number) { padding-inline-end: 0.4rem; color: var(--muted); text-align: end; font-variant-numeric: tabular-nums; }
.patch-rows :global(.line) { position: relative; padding-inline: 1.35rem 0.5rem; }
.patch-rows :global(.line::before) { position: absolute; inset-inline-start: 0.3rem; }
.patch-rows :global(.line.added::before) { content: "+"; color: var(--added-ink); }
.patch-rows :global(.line.removed::before) { content: "−"; color: var(--removed-ink); }
.patch-rows :global(.added) { background: var(--added); }
.patch-rows :global(.removed) { background: var(--removed); }
.patch-rows :global(.added .line-number:nth-child(2)), .patch-rows :global(.line-number.added) { color: var(--added-ink); }
.patch-rows :global(.removed .line-number:first-child), .patch-rows :global(.line-number.removed) { color: var(--removed-ink); }
.patch-rows :global(.added > .line-number:first-child), .patch-rows :global(.line-number.added) { box-shadow: inset 1px 0 var(--added-ink); }
.patch-rows :global(.removed > .line-number:first-child), .patch-rows :global(.line-number.removed) { box-shadow: inset 1px 0 var(--removed-ink); }
.patch-rows :global(.blank) { background: var(--surface); }
pre { margin: 0; padding: 0.75rem; overflow: auto; }
.raw-rows { overflow-x: auto; padding-block: 0.75rem; }
.raw-rows :global(pre) { margin: 0; padding-inline: 0.75rem; }
.raw-notice { margin: 0; padding: 0.35rem 0.75rem; color: var(--muted); background: var(--surface); }
.patch-rows :global(.hljs-keyword), .patch-rows :global(.hljs-selector-tag), .patch-rows :global(.hljs-literal) { color: var(--syntax-keyword); }
.patch-rows :global(.hljs-title), .patch-rows :global(.hljs-tag), .patch-rows :global(.hljs-attr), .patch-rows :global(.hljs-built_in) { color: var(--syntax-name); }
.patch-rows :global(.hljs-string), .patch-rows :global(.hljs-regexp) { color: var(--syntax-string); }
.patch-rows :global(.hljs-number), .patch-rows :global(.hljs-symbol) { color: var(--syntax-number); }
.patch-rows :global(.hljs-comment) { color: var(--muted); }
</style>
