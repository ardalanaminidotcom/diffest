<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { FileCategory } from "./category-settings.js";
import type { Snippet } from "svelte";

interface Properties {
  paths: string[];
  categories: FileCategory[];
  entry: Snippet<[number]>;
}

let { paths, categories, entry }: Properties = $props();
const groups = $derived.by(() => {
  const grouped = new Map<string, Map<string | null, number[]>>();
  for (const [index] of paths.entries()) {
    const category = categories[index]?.category ?? "Other";
    const subcategory = categories[index]?.subcategory ?? null;
    const subgroups = grouped.get(category) ?? new Map<string | null, number[]>();
    const files = subgroups.get(subcategory) ?? [];
    files.push(index);
    subgroups.set(subcategory, files);
    grouped.set(category, subgroups);
  }
  return grouped;
});
</script>

<nav aria-label="Changed files">
  {#each groups as [category, subgroups] (category)}
    <details open>
      <summary>{category}</summary>
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
nav, .entries { display: grid; gap: 0.35rem; }
.entries { margin-inline-start: 0.5rem; margin-block: 0.35rem; }
summary { cursor: pointer; font-size: 0.85rem; overflow-wrap: anywhere; }
</style>
