<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { Categorization, CategoryRule, FileCategory } from "./category-settings.js";
import type { DiffLayout } from "./revisions.js";
import { invoke } from "@tauri-apps/api/core";
import CategoryRuleForm from "./category-rule-form.svelte";

interface Properties {
  mainCheckout: string | null;
  paths: string[];
  categories?: FileCategory[];
  defaultLayout?: DiffLayout;
}

let { mainCheckout, paths, categories = $bindable([]), defaultLayout = $bindable("unified") }: Properties = $props();
let settings = $state<Categorization>();
let loadedCheckout = $state<string | null>(null);
let error = $state("");
let pendingRead = 0;
let savingLayout = $state(false);

async function load(checkout: string | null, filePaths: string[]): Promise<void> {
  const request = ++pendingRead;
  try {
    const response = await invoke<Categorization>("read_category_settings", {
      mainCheckout: checkout,
      paths       : filePaths,
    });
    if (request !== pendingRead) return;

    settings = response;
    loadedCheckout = checkout;
    categories = response.categories;
    defaultLayout = response.defaultDiffLayout ?? "unified";
    error = "";
  } catch (error_) {
    if (request === pendingRead) error = String(error_);
  }
}

function refresh(): void {
  void load(mainCheckout, paths);
}

$effect(() => {
  categories = [];
  refresh();
});

async function save(checkout: string | null, version: string, rules: CategoryRule[] | null): Promise<void> {
  await invoke("save_category_settings", {
    mainCheckout: checkout,
    version,
    rules,
  });
  await load(mainCheckout, paths);
}

async function changeLayout(event: Event & { currentTarget: HTMLSelectElement; }): Promise<void> {
  const select = event.currentTarget;
  const version = settings?.global.version;
  if (!version) return;

  savingLayout = true;
  try {
    await invoke("save_default_diff_layout", {
      version,
      layout: select.value,
    });
    await load(mainCheckout, paths);
  } catch (error_) {
    error = String(error_);
  } finally {
    select.value = defaultLayout;
    savingLayout = false;
  }
}
const errorAnnouncement = $derived([
  error,
  settings?.global.error,
  loadedCheckout === mainCheckout ? settings?.project?.error : null,
].filter(Boolean).join(" "));
</script>

<svelte:window onfocus={refresh} />

<details name="workspace-menu" class="category-settings">
  <summary title="Settings" aria-label={errorAnnouncement ? "Settings, error" : "Settings"}
    class:has-error={Boolean(errorAnnouncement)}>
    <svg aria-hidden="true" viewBox="0 0 16 16" fill="none">
      <path d="m6.8 1.8-.4 1.4-1.1.6-1.4-.4-1.2 2 .9 1.2v1.3l-.9 1.2 1.2 2 1.4-.4 1.1.6.4 1.4h2.4l.4-1.4 1.1-.6 1.4.4 1.2-2-.9-1.2V6.6l.9-1.2-1.2-2-1.4.4-1.1-.6-.4-1.4Z" transform="translate(0 .75)" />
      <circle cx="8" cy="8" r="2" />
    </svg>
  </summary>
  <div class="settings-panel">
    {#if settings}
      <label for="default-diff-layout">Default diff layout</label>
      <select
        id="default-diff-layout"
        value={defaultLayout}
        onchange={changeLayout}
        disabled={savingLayout || !settings.global.version}>
        <option value="unified">Unified</option>
        <option value="split">Split</option>
      </select>
    {/if}
    <p>Rules use repository-relative globs. The first match wins. Use <code>**</code> to match across folders.</p>
    <p>In JSON, set <code>categoryRules</code> to an array of rules with <code>glob</code>, <code>category</code>, and an optional <code>subcategory</code>.</p>
    {#if settings}
      <CategoryRuleForm
        scope="Global"
        parent="built-in"
        tier={settings.global}
        {refresh}
        save={(version, rules) => save(null, version, rules)} />
      {#if mainCheckout && loadedCheckout === mainCheckout && settings.project}
        {#key mainCheckout}
          <CategoryRuleForm
            scope="Project"
            parent="global"
            tier={settings.project}
            {refresh}
            save={(version, rules) => save(mainCheckout ?? null, version, rules)} />
        {/key}
      {/if}
    {/if}
    {#if error}
      <p role="alert">{error}</p>
    {/if}
    {#if settings?.global.error}
      <p role="alert">{settings.global.error}</p>
    {/if}
    {#if loadedCheckout === mainCheckout && settings?.project?.error}
      <p role="alert">{settings.project.error}</p>
    {/if}
  </div>
</details>
<span class="settings-announcement" aria-live="polite">{errorAnnouncement}</span>

<style>
summary {
  cursor: pointer;
  background: var(--canvas);
  border: 1px solid var(--border);
  border-radius: 5px;
  position: relative;
  display: grid;
  place-items: center;
  width: max-content;
  padding: 0.23rem;
  list-style: none;
}
summary::-webkit-details-marker { display: none; }
summary svg { width: 1rem; height: 1rem; stroke: currentColor; stroke-width: 1.4; }
summary:hover, details[open] > summary { background: var(--selection); }
details[open] > summary { z-index: 3; }
summary.has-error::after {
  content: "";
  position: absolute;
  inset: 0.12rem 0.12rem auto auto;
  width: 0.35rem;
  height: 0.35rem;
  border-radius: 50%;
  background: var(--removed-ink);
}
.settings-announcement {
  position: absolute;
  clip-path: inset(50%);
  width: 1px;
  height: 1px;
  overflow: hidden;
  white-space: nowrap;
}
.settings-panel {
  position: absolute;
  z-index: 2;
  box-sizing: border-box;
  inset: auto auto 100% 0.85rem;
  width: min(calc(100vw - 1.7rem), 34rem);
  max-height: min(80dvh, calc(100dvh - 9rem));
  overflow: auto;
  padding: 0.85rem 1rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: 0 6px 18px #0002;
}
@media (max-width: 520px) {
  .settings-panel {
    position: fixed;
    inset: auto 0.85rem 0.85rem;
    max-height: 45dvh;
  }
}
label, select { margin-block-start: 0.65rem; }
select {
  font: inherit;
  color: inherit;
  background: var(--canvas);
  border: 1px solid var(--border);
  border-radius: 0.35rem;
  padding: 0.35rem 0.45rem;
}
p { font-size: 0.85rem; overflow-wrap: anywhere; }
</style>
