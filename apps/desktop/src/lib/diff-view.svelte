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

{#if parsed.tooManyLines}<p class="m-0 bg-surface px-3 py-[0.35rem] text-muted">Large patch · Showing raw text</p>{/if}
{#if !raw && parsed.modeLines.length}
  <pre class="m-0 overflow-auto border-be border-divider px-3 py-[0.35rem] font-diff text-[0.875rem]/[1.55] text-muted" aria-label="File mode change">{parsed.modeLines.join("\n")}</pre>
{/if}
{#if blocks.length}
  <div class={["patch-rows relative", raw && "overflow-x-auto py-3"]} use:mountDiffRows={{
    blocks, windowRows, beforeLanguage, afterLanguage, highlight: highlight && patch.length <= highlightPatchLimit,
  }}></div>
{:else if patch}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of long patch lines.) -->
  <pre class="m-0 overflow-auto p-3 font-diff text-[0.875rem]/[1.55]" aria-label="Unified patch" tabindex="0">{omitHunkHeaders(patch)}</pre>
{/if}
