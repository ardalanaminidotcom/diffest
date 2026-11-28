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

<nav aria-label="Changed files" class="grid gap-[0.55rem] px-[0.4rem] pt-[0.2rem] pb-3">
  {#each groups as [category, subgroups] (category)}
    {@const { added, removed } = categoryLineCounts(subgroups.values())}
    <details class="group" open>
      <summary class="flex cursor-pointer items-center gap-[0.3rem] rounded-sm bg-toolbar px-[0.35rem] py-1 text-[0.82rem] font-[650] wrap-anywhere text-muted [&::-webkit-details-marker]:hidden"
        aria-label={`${ category }, lines added ${ added }, lines removed ${ removed }`}>
        <svg class="size-3 flex-none fill-none stroke-current stroke-[1.4] group-open:rotate-90" aria-hidden="true" viewBox="0 0 16 16"><path d="m6 4 4 4-4 4" /></svg>
        <span class="min-w-0">{category}</span>
        {#if added || removed}
          <span class="ms-auto flex flex-none gap-[0.3rem] font-mono text-[0.75rem]/[normal]" aria-hidden="true">
            {#if added}<span class="text-added-ink">+{added}</span>{/if}
            {#if removed}<span class="text-removed-ink">−{removed}</span>{/if}
          </span>
        {/if}
      </summary>
      <div class="grid gap-[0.12rem]">
        {#each subgroups as [subcategory, files] (subcategory)}
          {#if subcategory}
            <details class="ps-2" open>
              <summary class="cursor-pointer px-[0.35rem] py-1 text-[0.82rem] font-[650] wrap-anywhere text-muted">{subcategory}</summary>
              <div class="grid gap-[0.12rem]">
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
