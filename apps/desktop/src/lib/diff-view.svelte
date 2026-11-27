<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { DiffLayout } from "./revisions.js";
import hljs from "highlight.js/lib/common";
import { omitHunkHeaders, parsePatch, splitHunk } from "./patch.js";

interface Properties {
  patch: string;
  path: string;
  previousPath: string | null;
  layout: DiffLayout;
}

const highlightPatchLimit = 100_000;
const highlightLineLimit = 2000;

function languageFor(filename: string): string {
  const basename = filename.slice(filename.lastIndexOf("/") + 1).toLowerCase();
  if (basename === "dockerfile") return "dockerfile";
  if (basename === "makefile") return "makefile";
  const extension = basename.includes(".") ? basename.split(".").at(-1) ?? "" : "";
  return extension === "svelte" || extension === "vue" ? "xml" : extension;
}

let { patch, path, previousPath, layout }: Properties = $props();
const parsed = $derived(parsePatch(patch));
const beforeLanguage = $derived(languageFor(previousPath ?? path));
const afterLanguage = $derived(languageFor(path));

function highlight(text: string, language: string): string | null {
  // Keep very large patches and long generated lines readable without tokenizing them on the UI thread.
  if (patch.length > highlightPatchLimit || text.length > highlightLineLimit) return null;
  if (!hljs.getLanguage(language)) return null;
  return hljs.highlight(text, { language }).value;
}
</script>

{#snippet source(text: string, language: string)}
  {@const formatted = highlight(text, language)}
  <code>{#if formatted !== null}{@html formatted}{:else}{text}{/if}</code>
{/snippet}

{#if parsed.hunks.length}
  <table aria-label={layout === "split" ? "Split patch" : "Unified patch"} class:split={layout === "split"}>
    {#if layout === "split"}
      <colgroup><col class="number" /><col /></colgroup>
      <colgroup><col class="number" /><col /></colgroup>
      <thead><tr><th scope="colgroup" colspan="2">Before</th><th scope="colgroup" colspan="2">After</th></tr></thead>
    {:else}
      <colgroup><col class="number" /><col class="number" /><col /></colgroup>
    {/if}
    {#each parsed.hunks as hunk}
      <tbody>
        {#if layout === "split"}
          {#each splitHunk(hunk.lines) as row}
            <tr>
              <td class="line-number" class:removed={row.before?.change}>{row.before?.before}</td>
              <td class="line" class:removed={row.before?.change} class:blank={!row.before}>
                {#if row.before}{@render source(row.before.text, beforeLanguage)}{/if}
              </td>
              <td class="line-number" class:added={row.after?.change}>{row.after?.after}</td>
              <td class="line" class:added={row.after?.change} class:blank={!row.after}>
                {#if row.after}{@render source(row.after.text, afterLanguage)}{/if}
              </td>
            </tr>
          {/each}
        {:else}
          {#each hunk.lines as line}
            <tr class:removed={line.change === "removed"} class:added={line.change === "added"}>
              <td class="line-number">{line.before}</td>
              <td class="line-number">{line.after}</td>
              <td class="line" class:removed={line.change === "removed"} class:added={line.change === "added"}>
                {@render source(line.text, line.change === "removed" ? beforeLanguage : afterLanguage)}
              </td>
            </tr>
          {/each}
        {/if}
      </tbody>
    {/each}
  </table>
{:else}
  {#if patch}
    {#if parsed.tooManyLines}<p class="raw-notice">Large patch · Showing raw text</p>{/if}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of long patch lines.) -->
    <pre aria-label="Unified patch" tabindex="0">{omitHunkHeaders(patch)}</pre>
  {/if}
{/if}

<style>
table { width: 100%; table-layout: fixed; border-collapse: collapse; }
th, td { padding: 0; text-align: start; vertical-align: top; }
thead th {
  padding: 0.35rem 0.75rem;
  color: var(--muted);
  font-size: 0.72rem;
  font-weight: 600;
}
.split th + th, .split td:nth-child(3) { border-inline-start: 1px solid var(--border); }
tbody + tbody tr:first-child td { border-block-start: 1px solid var(--border); }
pre, table { font: 0.875rem/1.55 ui-monospace, "SF Mono", Consolas, monospace; }
code { font: inherit; white-space: pre-wrap; overflow-wrap: anywhere; tab-size: 4; }
.number { width: 3.75em; }
.line-number { padding-inline-end: 0.4rem; color: var(--muted); text-align: end; font-variant-numeric: tabular-nums; }
.line { position: relative; padding-inline: 1.35rem 0.5rem; }
.line::before { position: absolute; inset-inline-start: 0.3rem; }
.line.added::before { content: "+"; color: var(--added-ink); }
.line.removed::before { content: "−"; color: var(--removed-ink); }
.added { background: var(--added); }
.removed { background: var(--removed); }
.added .line-number:nth-child(2), .line-number.added { color: var(--added-ink); }
.removed .line-number:first-child, .line-number.removed { color: var(--removed-ink); }
.added > .line-number:first-child, .line-number.added { box-shadow: inset 1px 0 var(--added-ink); }
.removed > .line-number:first-child, .line-number.removed { box-shadow: inset 1px 0 var(--removed-ink); }
.blank { background: var(--surface); }
pre { margin: 0; padding: 0.75rem; overflow: auto; }
.raw-notice { margin: 0; padding: 0.35rem 0.75rem; color: var(--muted); background: var(--surface); }
code :global(.hljs-keyword), code :global(.hljs-selector-tag), code :global(.hljs-literal) { color: var(--syntax-keyword); }
code :global(.hljs-title), code :global(.hljs-tag), code :global(.hljs-attr), code :global(.hljs-built_in) { color: var(--syntax-name); }
code :global(.hljs-string), code :global(.hljs-regexp) { color: var(--syntax-string); }
code :global(.hljs-number), code :global(.hljs-symbol) { color: var(--syntax-number); }
code :global(.hljs-comment) { color: var(--muted); }
</style>
