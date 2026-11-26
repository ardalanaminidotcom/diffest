<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { Categorization, CategoryRule, FileCategory } from "./category-settings.js";
import { invoke } from "@tauri-apps/api/core";
import CategoryRuleForm from "./category-rule-form.svelte";

interface Properties {
  mainCheckout: string | undefined;
  paths: string[];
  categories?: FileCategory[];
}

let { mainCheckout, paths, categories = $bindable([]) }: Properties = $props();
let settings = $state<Categorization>();
let loadedCheckout = $state<string>();
let error = $state("");
let pendingRead = 0;

async function load(checkout: string | undefined, filePaths: string[]): Promise<void> {
  const request = ++pendingRead;
  try {
    const response = await invoke<Categorization>("read_category_settings", {
      mainCheckout: checkout ?? null,
      paths       : filePaths,
    });
    if (request !== pendingRead) return;

    settings = response;
    loadedCheckout = checkout;
    categories = response.categories;
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
</script>

<svelte:window onfocus={refresh} />

<details class="category-settings">
  <summary>Category settings</summary>
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
.category-settings { margin-block-end: 1rem; }
summary { cursor: pointer; }
p { font-size: 0.85rem; overflow-wrap: anywhere; }
</style>
