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
const imageFormats = ["JPEG", "PNG", "GIF", "WebP"];
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

<div class="version">
  <h3>{label}</h3>
  {#if version}
    <code>{version.path}</code>
    <dl>
      {#if version.format}<dt>Format</dt><dd>{version.format}</dd>{/if}
      <dt>Size</dt><dd>{version.byteSize === null ? "Unavailable" : `${ version.byteSize } bytes`}</dd>
      {#if dimensions}
        <dt>Dimensions</dt><dd>{dimensions.width} × {dimensions.height} px</dd>
        <dt>Aspect ratio</dt><dd>{aspectRatio(dimensions.width, dimensions.height)}</dd>
      {/if}
    </dl>
    {#if version.preview && !previewFailed}
      <img
        bind:this={image}
        src={version.preview}
        alt={`${ label } preview of ${ version.path }`}
        onload={imageLoaded}
        onerror={() => previewFailed = true} />
    {:else if version.format && imageFormats.includes(version.format)}
      <p>Preview unavailable</p>
    {/if}
  {:else}
    <p>File absent</p>
  {/if}
</div>

<style>
.version {
  min-width: 0;
  padding: 0.65rem;
  border: 1px solid var(--border);
  border-radius: 0.35rem;
  font-size: 0.85rem;
}

h3 { margin: 0 0 0.25rem; font-size: inherit; }
code { overflow-wrap: anywhere; }
p { margin: 0.4rem 0 0; }
dl { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0.2rem 0.65rem; margin: 0.4rem 0 0; }
dd { margin: 0; }
img { display: block; max-width: 100%; max-height: 24rem; margin: 0.5rem auto 0; }
</style>
