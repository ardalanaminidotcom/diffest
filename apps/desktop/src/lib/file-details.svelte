<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */

import type { FileDetails, FileDetailsSource } from "./file-details.js";
import { invoke } from "@tauri-apps/api/core";
import FileVersion from "./file-version.svelte";

interface Properties {
  checkout: string;
  source: FileDetailsSource;
}

const { checkout, source }: Properties = $props();
let details = $state<FileDetails | null>(null);
let failure = $state("");
const beforeSize = $derived(details?.before === null ? 0 : details?.before?.byteSize);
const afterSize = $derived(details?.after === null ? 0 : details?.after?.byteSize);
const delta = $derived(beforeSize != null && afterSize != null ? afterSize - beforeSize : null);

$effect(() => {
  details = null;
  failure = "";
  let current = true;
  void invoke<FileDetails>("read_file_details", {
    checkout,
    source,
  }).then(
    (result) => {
      if (current) details = result;
    },
    (error: unknown) => {
      if (current) failure = String(error);
    },
  );
  return () => {
    current = false;
  };
});
</script>

<section aria-label="File details">
  {#if details}
    <div class="versions">
      <FileVersion label="Before" version={details.before} />
      <FileVersion label="After" version={details.after} />
    </div>
    {#if delta !== null}<p>Size change <strong>{delta > 0 ? "+" : ""}{delta} bytes</strong></p>{/if}
  {:else if failure}
    <p role="status">{failure}</p>
  {:else}
    <p role="status">Loading file details…</p>
  {/if}
</section>

<style>
section { margin-block-end: 0.65rem; }

.versions {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
  align-items: start;
  gap: 0.65rem;
}

p { margin: 0.5rem 0 0; font-size: 0.85rem; }
</style>
