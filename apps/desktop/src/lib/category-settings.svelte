<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { Categorization, CategoryRule, CategoryRuleDraft } from "./category-settings.js";
import type { DiffLayout, Project, ReviewRetention } from "./revisions.js";
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";
import CategoryRuleForm from "./category-rule-form.svelte";

interface Properties {
  projects: Project[];
  drafts: Record<string, CategoryRuleDraft | null>;
  onback(): void;
  onsaved(): Promise<void>;
}

let { projects, drafts, onback, onsaved }: Properties = $props();
const configurableProjects = $derived(projects.filter(project => project.mainCheckout !== null));
let scope = $state("");
let section = $state<"general" | "categories">("general");
const checkout = $derived(configurableProjects.find(project => project.id.toString() === scope)?.mainCheckout ?? null);

let loadedCheckout = $state<string | null>();
let settings = $state<Categorization>();
let error = $state("");
let retention = $state<ReviewRetention>();
let retentionDays = $state<number>();
let retentionError = $state("");
const maxRetentionDays = 4_294_967_295;
const validDays = $derived(typeof retentionDays === "number" && Number.isInteger(retentionDays)
  && retentionDays >= 1 && retentionDays <= maxRetentionDays);
let pendingRead = 0;
let saving = $state(false);
let backButton: HTMLButtonElement;
let scopePicker: HTMLSelectElement;
const currentSettings = $derived(loadedCheckout === checkout ? settings : null);
const tier = $derived(checkout === null ? currentSettings?.global : currentSettings?.project);
const showGeneral = $derived(section === "general");

onMount(() => backButton.focus({ preventScroll: true }));

async function load(mainCheckout: string | null): Promise<void> {
  const request = ++pendingRead;
  try {
    const [response, reviewRetention] = await Promise.all([
      invoke<Categorization>("read_category_settings", {
        mainCheckout,
        paths: [],
      }),
      invoke<ReviewRetention>("read_review_retention", { mainCheckout }),
    ]);
    if (request !== pendingRead) return;

    const pristine = retentionDays === retention?.days;
    if (loadedCheckout !== mainCheckout || pristine) retentionDays = reviewRetention.days;
    loadedCheckout = mainCheckout;
    settings = response;
    retention = reviewRetention;
    error = "";
  } catch (error_) {
    if (request === pendingRead) error = String(error_);
  }
}

function refresh(): void {
  if (!saving) void load(checkout);
}

$effect(() => {
  const mainCheckout = checkout;
  error = "";
  retentionError = "";
  void load(mainCheckout);
});

async function persist(action: Promise<unknown>): Promise<void> {
  saving = true;
  try {
    await action;
    await Promise.all([load(checkout), onsaved()]);
  } finally {
    saving = false;
  }
}

function saveRules(version: string, rules: CategoryRule[] | null): Promise<void> {
  return persist(invoke("save_category_settings", {
    mainCheckout: checkout,
    version,
    rules,
  }));
}

async function changeRetention(days: number | null): Promise<void> {
  if (saving) return;

  saving = true;
  pendingRead++;
  retentionError = "";
  try {
    retention = await invoke<ReviewRetention>("save_review_retention", {
      mainCheckout: checkout,
      days,
    });
    retentionDays = retention.days;
  } catch (error_) {
    retentionError = String(error_);
  } finally {
    saving = false;
  }
}

async function changeLayout(layout: DiffLayout): Promise<void> {
  const version = tier?.version;
  if (saving || !version || layout === currentSettings?.defaultDiffLayout) return;

  try {
    const parameters = {
      version,
      layout,
    };
    await persist(invoke("save_default_diff_layout", parameters));
  } catch (error_) {
    error = String(error_);
  }
}

const navButton = `control flex min-w-0 items-center gap-2 border-transparent bg-transparent px-2 py-[0.45rem]
  text-start text-muted enabled:cursor-pointer enabled:hover:bg-selection disabled:opacity-[0.58]
  aria-[current=page]:bg-selection aria-[current=page]:font-[620] aria-[current=page]:text-accent`;
const icon = "size-4 flex-none fill-none stroke-current stroke-[1.4] [stroke-linecap:round] [stroke-linejoin:round]";
const setting = `mbs-[1.3rem] flex flex-wrap items-center justify-between gap-[0.65rem]
  border-bs border-be border-divider py-3`;
const description = "m-0 mbs-1 text-muted";
const layoutButton = `control border-0 bg-transparent px-[0.55rem] py-[0.3rem] enabled:cursor-pointer
  enabled:hover:bg-selection enabled:hover:text-accent disabled:opacity-[0.58] aria-pressed:bg-selection
  aria-pressed:text-accent`;
const formButton = "[color:inherit] [font:inherit] enabled:cursor-pointer disabled:opacity-[0.58]";
const errorMessage = "m-0 mbs-2 wrap-anywhere text-removed-ink";
</script>

<svelte:window onfocus={refresh} onkeydown={(event) => {
  if (saving || event.defaultPrevented || event.key !== "Escape" || event.target === scopePicker) return;

  if (event.target instanceof HTMLInputElement) event.target.blur();
  onback();
}} />

<main class="fixed inset-0 z-5 grid grid-cols-[max(10rem,20vw)_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] bg-canvas compact:grid-cols-[max(7rem,24vw)_minmax(0,1fr)]" aria-label="Settings">
  <aside class="grid min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] border-e border-divider bg-surface" aria-label="Settings sidebar">
    <header class="box-border flex min-h-[3.9rem] items-center border-be border-divider px-[0.65rem] py-[0.45rem] compact:px-1">
      <button class={navButton} type="button" bind:this={backButton} aria-label="Back to review" disabled={saving} onclick={onback}>
        <svg class={icon} aria-hidden="true" viewBox="0 0 16 16"><path d="M13.5 8h-11m4-4-4 4 4 4" /></svg>
        Back to review
      </button>
    </header>
    <nav class="grid content-start gap-[0.2rem] overflow-auto px-[0.65rem] py-3 compact:px-1" aria-label="Settings sections">
      <button type="button" class={navButton} aria-current={showGeneral ? "page" : undefined}
        disabled={saving} onclick={() => section = "general"}>
        <svg class={icon} aria-hidden="true" viewBox="0 0 16 16"><path d="M2.5 4h11m-11 4h11m-11 4h11M6 2.6v2.8m4 1.2v2.8M6 10.6v2.8" /></svg>
        General
      </button>
      <button type="button" class={navButton} aria-current={!showGeneral ? "page" : undefined}
        disabled={saving} onclick={() => section = "categories"}>
        <svg class={icon} aria-hidden="true" viewBox="0 0 16 16"><path d="M1.8 4.6a1.2 1.2 0 0 1 1.2-1.2h3l1.4 1.5H13a1.2 1.2 0 0 1 1.2 1.2v5.6a1.2 1.2 0 0 1-1.2 1.2H3a1.2 1.2 0 0 1-1.2-1.2Z" /></svg>
        File categories
      </button>
    </nav>
    <footer class="flex items-center gap-2 border-bs border-divider px-[0.9rem] py-[0.65rem] font-[680]">
      <svg class="size-[1.125rem] flex-none fill-none stroke-accent stroke-[1.4] [stroke-linecap:round] [stroke-linejoin:round]" aria-hidden="true" viewBox="0 0 18 18"><rect x="1.5" y="1.5" width="15" height="15" rx="3" /><path d="M6 7h6M9 4v6m-3 3h6" /></svg>
      Diffest
    </footer>
  </aside>

  <div class="grid min-w-0 grid-rows-[auto_minmax(0,1fr)]">
    <header class="box-border flex min-h-[3.9rem] items-center justify-between gap-[0.65rem] border-be border-divider bg-toolbar px-5 py-[0.45rem] compact:px-[0.7rem]">
      <strong class="text-[0.95rem]">Settings</strong>
      <label class="control box-border flex max-w-[min(16rem,65%)] min-w-[min(10rem,65%)] items-center gap-1 ps-[0.55rem]
        has-[select:focus-visible]:outline-2 has-[select:focus-visible]:outline-offset-2 has-[select:focus-visible]:outline-accent">
        {#if checkout === null}
          <svg class={[icon, "text-muted"]} aria-hidden="true" viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.8" /><ellipse cx="8" cy="8" rx="2.5" ry="5.8" /><path d="M2.2 8h11.6" /></svg>
        {:else}
          <svg class={[icon, "text-muted"]} aria-hidden="true" viewBox="0 0 16 16"><path d="M1.8 4.6a1.2 1.2 0 0 1 1.2-1.2h3l1.4 1.5H13a1.2 1.2 0 0 1 1.2 1.2v5.6a1.2 1.2 0 0 1-1.2 1.2H3a1.2 1.2 0 0 1-1.2-1.2Z" /></svg>
        {/if}
        <select class={[formButton, "min-w-0 flex-1 rounded-[inherit] border-0 bg-transparent px-2 py-[0.35rem] text-ellipsis focus-visible:outline-none"]}
          aria-label="Settings scope" bind:this={scopePicker} value={scope} disabled={saving}
          onchange={(event) => {
            scope = event.currentTarget.value;
            section = "categories";
          }}>
          <option value="">Global</option>
          {#each configurableProjects as project (project.id)}
            {@const hasDuplicateName = configurableProjects.some(other => other.id !== project.id && other.name === project.name)}
            <option value={project.id.toString()}>
              Project · {project.name}{hasDuplicateName ? ` (${ project.mainCheckout })` : ""}
            </option>
          {/each}
        </select>
      </label>
    </header>
    <div class="min-w-0 overflow-auto [scrollbar-gutter:stable]">
      <section class="mx-auto box-border max-w-[54rem] px-6 py-[1.7rem] compact:px-[0.7rem]" aria-labelledby="settings-section-title">
        {#if showGeneral}
          <h1 id="settings-section-title" class="m-0 text-[1.45rem] tracking-tight">General</h1>
          {#if checkout === null && currentSettings}
            <div class={setting}>
              <div>
                <h2 class="m-0 text-[0.95rem]">Default diff layout</h2>
                <p class={description}>New reviews use this layout.</p>
              </div>
              <div class="flex rounded-[5px] border border-divider" role="group" aria-label="Default diff layout">
                <button class={layoutButton} type="button" aria-pressed={currentSettings.defaultDiffLayout !== "split"}
                  disabled={saving || !tier?.version} onclick={() => changeLayout("unified")}>Unified</button>
                <button class={[layoutButton, "border-s border-divider"]} type="button" aria-pressed={currentSettings.defaultDiffLayout === "split"}
                  disabled={saving || !tier?.version} onclick={() => changeLayout("split")}>Split</button>
              </div>
            </div>
          {/if}
          {#if currentSettings && retention}
            <form class={setting} onsubmit={(event) => {
              event.preventDefault();
              if (validDays) void changeRetention(retentionDays ?? null);
            }}>
              <div>
                <h2 class="m-0 text-[0.95rem]"><label for="review-retention-days">Review inactivity limit</label></h2>
                <p class={description}>Remove unfinished commit reviews after this many days away.</p>
                {#if checkout !== null && retention.overrideDays === null}
                  <p class={description}>Using the global setting.</p>
                {/if}
                {#if retentionError}<p class={description} role="alert">{retentionError}</p>{/if}
              </div>
              <div class="flex flex-wrap items-center gap-[0.35rem]">
                <input id="review-retention-days" class="control box-border inline-20 px-[0.45rem] py-[0.3rem]" type="number" min="1" max={maxRetentionDays} step="1"
                  bind:value={retentionDays} required disabled={saving} />
                <button class={formButton} type="submit" disabled={saving || !validDays
                  || (retentionDays === retention.days && (checkout === null || retention.overrideDays !== null))}>Save</button>
                {#if checkout !== null && retention.overrideDays !== null}
                  <button class={formButton} type="button" disabled={saving} onclick={() => changeRetention(null)}>Use global setting</button>
                {/if}
              </div>
            </form>
          {/if}
        {:else}
          <h1 id="settings-section-title" class="m-0 text-[1.45rem] tracking-tight">File categories</h1>
          <p class={description}>The first matching glob wins. Unmatched files go to Source.</p>
          {#if tier}
            {#key checkout}
              {@const draftKey = scope}
              <CategoryRuleForm scope={checkout === null ? "Global" : "Project"}
                parent={checkout === null ? "built-in" : "Global"} {tier} {refresh} save={saveRules}
                initialDraft={drafts[draftKey] ?? null} ondraft={(draft) => drafts[draftKey] = draft}
                onglobal={() => {
                  scope = "";
                  scopePicker.focus();
                }} />
            {/key}
          {/if}
        {/if}
        {#if error}
          <p class={errorMessage} role="alert">{error}</p>
        {:else if !currentSettings}
          <p class={description} role="status">Loading settings…</p>
        {/if}
        {#if tier?.error}
          <p class={errorMessage} role="alert">{tier.error}</p>
        {/if}
        {#if checkout !== null && currentSettings?.global.error}
          <p class={errorMessage} role="alert">{currentSettings.global.error}</p>
        {/if}
      </section>
    </div>
  </div>
</main>
