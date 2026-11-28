<script lang="ts">
import type { FileDetails } from "./file-details.js";
import FileVersion from "./file-version.svelte";

const { details }: { details: FileDetails; } = $props();
</script>

<section aria-label="Image comparison" class="grid grid-cols-[repeat(auto-fit,minmax(min(100%,12rem),1fr))] items-start gap-[0.65rem] p-3">
  <FileVersion label="Before" version={details.before} />
  <FileVersion label="After" version={details.after} />
</section>
