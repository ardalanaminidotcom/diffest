<script lang="ts">
import type { FileDetails } from "./file-details.js";
import FileVersion from "./file-version.svelte";

const { details }: { details: FileDetails; } = $props();
</script>

<section aria-label="Image comparison" class="versions">
  <FileVersion label="Before" version={details.before} />
  <FileVersion label="After" version={details.after} />
</section>

<style>
.versions {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
  align-items: start;
  gap: 0.65rem;
  padding: 0.75rem;
}
</style>
