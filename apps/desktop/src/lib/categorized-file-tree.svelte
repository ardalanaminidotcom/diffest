<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { FileGroups } from "./category-settings.js";
import type { PatchSummary } from "./patch.js";
import type { Snippet } from "svelte";

interface Properties {
  groups: FileGroups;
  patchSummaries: PatchSummary[];
  entry: Snippet<[number]>;
}

let { groups, patchSummaries, entry }: Properties = $props();

function categoryLineCounts(subgroups: Iterable<number[]>): Pick<PatchSummary, "added" | "removed"> {
  const totals = {
    added  : 0,
    removed: 0,
  };
  for (const files of subgroups) {
    for (const index of files) {
      totals.added += patchSummaries[index].added;
      totals.removed += patchSummaries[index].removed;
    }
  }
  return totals;
}
</script>

<nav aria-label="Changed files">
  {#each groups as [category, subgroups] (category)}
    {@const { added, removed } = categoryLineCounts(subgroups.values())}
    <details open>
      <summary class="category-summary" aria-label={`${ category }, lines added ${ added }, lines removed ${ removed }`}>
        <svg aria-hidden="true" viewBox="0 0 16 16"><path d="m6 4 4 4-4 4" /></svg>
        <span class="category-name">{category}</span>
        {#if added || removed}
          <span class="line-totals" aria-hidden="true">
            {#if added}<span class="additions">+{added}</span>{/if}
            {#if removed}<span class="deletions">−{removed}</span>{/if}
          </span>
        {/if}
      </summary>
      <div class="entries">
        {#each subgroups as [subcategory, files] (subcategory)}
          {#if subcategory}
            <details open>
              <summary>{subcategory}</summary>
              <div class="entries">
                {#each files as index (index)}
                  {@render entry(index)}
                {/each}
              </div>
            </details>
          {:else}
            {#each files as index (index)}
              {@render entry(index)}
            {/each}
          {/if}
        {/each}
      </div>
    </details>
  {/each}
</nav>

<style>
nav { display: grid; gap: 0.55rem; padding: 0.2rem 0.4rem 0.75rem; }
.entries { display: grid; gap: 0.12rem; }
.entries details { padding-inline-start: 0.5rem; }
summary {
  cursor: pointer;
  color: var(--muted);
  font-size: 0.82rem;
  font-weight: 650;
  overflow-wrap: anywhere;
  padding: 0.25rem 0.35rem;
}
.category-summary {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  border-radius: 4px;
  background: var(--toolbar);
}
.category-summary::-webkit-details-marker { display: none; }
.category-summary svg { flex: none; width: 0.75rem; height: 0.75rem; fill: none; stroke: currentColor; stroke-width: 1.4; }
details[open] > .category-summary svg { transform: rotate(90deg); }
.category-name { min-width: 0; }
.line-totals { display: flex; flex: none; gap: 0.3rem; margin-inline-start: auto; font: 0.75rem ui-monospace, monospace; }
.additions { color: var(--added-ink); }
.deletions { color: var(--removed-ink); }
</style>
