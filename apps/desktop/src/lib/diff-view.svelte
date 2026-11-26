<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { DiffLayout } from "./revisions.js";

interface Line {
  number: number;
  text: string;
  changed: boolean;
}

interface Row {
  heading?: string;
  before?: Line;
  after?: Line;
}

interface Properties {
  patch: string;
  layout: DiffLayout;
}

let { patch, layout }: Properties = $props();

function splitPatch(text: string): Row[] {
  const result: Row[] = [];
  let before: Line[] = [];
  let after: Line[] = [];
  let previous: Line[] = [];
  let oldNumber = 0;
  let newNumber = 0;
  let inHunk = false;

  function flush(): void {
    for (let index = 0; index < Math.max(before.length, after.length); index++) {
      result.push({
        before: before[index],
        after : after[index],
      });
    }
    before = [];
    after = [];
  }

  for (const line of text.replace(/\n$/u, "").split("\n")) {
    const range = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/u.exec(line);
    if (range) {
      flush();
      result.push({ heading: line });
      oldNumber = Number(range[1]);
      newNumber = Number(range[2]);
      inHunk = true;
    } else if (inHunk) {
      const content = line.slice(1);
      if (line.startsWith("-")) {
        const removed = {
          number : oldNumber++,
          text   : content,
          changed: true,
        };
        before.push(removed);
        previous = [removed];
      } else if (line.startsWith("+")) {
        const added = {
          number : newNumber++,
          text   : content,
          changed: true,
        };
        after.push(added);
        previous = [added];
      } else if (line === "" || line.startsWith(" ")) {
        flush();
        const oldLine = {
          number : oldNumber++,
          text   : content,
          changed: false,
        };
        const newLine = {
          number : newNumber++,
          text   : content,
          changed: false,
        };
        result.push({
          before: oldLine,
          after : newLine,
        });
        previous = [oldLine, newLine];
      } else if (line.startsWith("\\")) {
        for (const side of previous) side.text += `\n${ line }`;
      } else {
        flush();
        inHunk = false;
      }
    }
  }
  flush();
  return result;
}

let rows = $derived(layout === "split" ? splitPatch(patch) : []);
</script>

{#if rows.length}
  <table aria-label="Split patch">
    <colgroup><col class="number" /><col /></colgroup>
    <colgroup><col class="number" /><col /></colgroup>
    <thead><tr><th scope="colgroup" colspan="2">Before</th><th scope="colgroup" colspan="2">After</th></tr></thead>
    <tbody>
      {#each rows as row}
        {#if row.heading}
          <tr><th class="hunk" colspan="4"><code>{row.heading}</code></th></tr>
        {:else}
          <tr>
            <td class="line-number" class:removed={row.before?.changed}>{row.before?.number}</td>
            <td class:removed={row.before?.changed}><code>{row.before?.text}</code></td>
            <td class="line-number" class:added={row.after?.changed}>{row.after?.number}</td>
            <td class:added={row.after?.changed}><code>{row.after?.text}</code></td>
          </tr>
        {/if}
      {/each}
    </tbody>
  </table>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of long patch lines.) -->
  <pre aria-label="Unified patch" tabindex="0">{patch}</pre>
{/if}

<style>
pre {
  margin: 0;
  padding: 0.75rem;
  overflow: auto;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 0.35rem;
  line-height: 1.45;
}

table {
  width: 100%;
  table-layout: fixed;
  border-collapse: collapse;
  border: 1px solid var(--border);
  background: var(--surface);
  font-size: 0.85rem;
}

th, td { padding: 0.2rem 0.35rem; text-align: start; vertical-align: top; }
th, .line-number:nth-child(3) { border-inline-start: 1px solid var(--border); }
thead th, .hunk { border-block-end: 1px solid var(--border); }
code { white-space: pre-wrap; overflow-wrap: anywhere; }
.number { width: 4ch; }
.line-number { color: color-mix(in srgb, CanvasText 75%, Canvas); font-variant-numeric: tabular-nums; text-align: end; }
.removed { background: color-mix(in srgb, #d33846 22%, Canvas); }
.added { background: color-mix(in srgb, #289b55 22%, Canvas); }
</style>
