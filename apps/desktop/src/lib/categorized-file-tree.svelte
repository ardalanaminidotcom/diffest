<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { FileGroups } from "./category-settings.js";
import type { Snippet } from "svelte";

interface Properties {
  groups: FileGroups;
  entry: Snippet<[number]>;
}

let { groups, entry }: Properties = $props();
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
</style>
