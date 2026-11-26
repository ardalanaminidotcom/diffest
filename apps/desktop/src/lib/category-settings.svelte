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
</script>

<svelte:window onfocus={refresh} />

<details class="category-settings">
  <summary>Settings</summary>
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
  </div>
</details>

{#if error}
  <p role="alert">{error}</p>
{/if}
{#if settings?.global.error}
  <p role="alert">{settings.global.error}</p>
{/if}
{#if loadedCheckout === mainCheckout && settings?.project?.error}
  <p role="alert">{settings.project.error}</p>
{/if}

<style>
summary {
  cursor: pointer;
  background: var(--canvas);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding: 0.3rem 0.52rem;
  font-size: 0.85rem;
  list-style: none;
}
summary::-webkit-details-marker { display: none; }
summary:hover, details[open] > summary { background: var(--selection); }
.settings-panel {
  position: absolute;
  z-index: 2;
  box-sizing: border-box;
  inset: 100% 0 auto auto;
  width: min(100%, 34rem);
  max-height: calc(100dvh - 5rem);
  overflow: auto;
  padding: 0.85rem 1rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 0 0 0 8px;
  box-shadow: 0 6px 18px #0002;
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
