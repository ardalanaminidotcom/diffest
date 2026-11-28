<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */

import type { FileVersion } from "./file-details.js";

interface Properties {
  label: string;
  version: FileVersion | null;
}

interface Dimensions {
  width: number;
  height: number;
}

const { label, version }: Properties = $props();
let dimensions = $state<Dimensions>();
let image = $state<HTMLImageElement>();
let previewFailed = $state(false);

function aspectRatio(width: number, height: number): string {
  let divisor = width;
  let remainder = height;
  while (remainder) [divisor, remainder] = [remainder, divisor % remainder];

  return `${ width / divisor }:${ height / divisor }`;
}

function imageLoaded(): void {
  if (!image) return;

  dimensions = {
    width : image.naturalWidth,
    height: image.naturalHeight,
  };
}
</script>

<div class="min-w-0 rounded-[0.35rem] border border-divider p-[0.65rem] text-[0.85rem]">
  <h3 class="m-0 mb-1 text-[length:inherit]">{label}</h3>
  {#if version?.preview && !previewFailed}
    <img class="mx-auto mt-2 block max-h-96 max-w-full" bind:this={image} src={version.preview} alt={`${ label } preview of ${ version.path }`}
      onload={imageLoaded} onerror={() => previewFailed = true} />
    {#if dimensions}
      <p class="m-0 mt-[0.4rem] text-muted">{dimensions.width} × {dimensions.height} px · {aspectRatio(dimensions.width, dimensions.height)}</p>
    {/if}
  {:else}
    <p class="m-0 mt-[0.4rem] text-muted">{version ? "Preview unavailable" : "File absent"}</p>
  {/if}
</div>
