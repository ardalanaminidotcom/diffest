<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { Categorization, CategoryRule, CategoryRuleDraft } from "./category-settings.js";
import type { DiffLayout, Project } from "./revisions.js";
import { invoke } from "@tauri-apps/api/core";
import { onMount, untrack } from "svelte";
import CategoryRuleForm from "./category-rule-form.svelte";

interface Properties {
  projects: Project[];
  activeProjectId: number | undefined;
  drafts: Record<string, CategoryRuleDraft | null>;
  onback(): void;
  onsaved(): Promise<void>;
}

let { projects, activeProjectId, drafts, onback, onsaved }: Properties = $props();
const configurableProjects = $derived(projects.filter(project => project.mainCheckout !== null));
let scope = $state(untrack(() => {
  const project = configurableProjects.find(item => item.id === activeProjectId);
  return project?.id.toString() ?? "";
}));
let section = $state<"general" | "categories">(untrack(() => (scope ? "categories" : "general")));
const checkout = $derived(configurableProjects.find(project => project.id.toString() === scope)?.mainCheckout ?? null);

let loadedCheckout = $state<string | null>();
let settings = $state<Categorization>();
let error = $state("");
let pendingRead = 0;
let saving = $state(false);
let backButton: HTMLButtonElement;
let scopePicker: HTMLSelectElement;
const currentSettings = $derived(loadedCheckout === checkout ? settings : null);
const tier = $derived(checkout === null ? currentSettings?.global : currentSettings?.project);
const showGeneral = $derived(checkout === null && section === "general");

onMount(() => backButton.focus({ preventScroll: true }));

async function load(mainCheckout: string | null): Promise<void> {
  const request = ++pendingRead;
  try {
    const response = await invoke<Categorization>("read_category_settings", {
      mainCheckout,
      paths: [],
    });
    if (request !== pendingRead) return;

    loadedCheckout = mainCheckout;
    settings = response;
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
</script>

<svelte:window onfocus={refresh} onkeydown={(event) => {
  if (!saving && !event.defaultPrevented && event.key === "Escape" && event.target !== scopePicker) onback();
}} />

<main class="settings-screen" aria-label="Settings">
  <aside class="settings-sidebar" aria-label="Settings sidebar">
    <header class="back-bar">
      <button class="nav-button" type="button" bind:this={backButton} aria-label="Back to review" disabled={saving} onclick={onback}>
        <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M13.5 8h-11m4-4-4 4 4 4" /></svg>
        Back to review
      </button>
    </header>
    <nav aria-label="Settings sections">
      {#if checkout === null}
        <button type="button" class="nav-button" aria-current={showGeneral ? "page" : undefined}
          disabled={saving} onclick={() => section = "general"}>
          <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M2.5 4h11m-11 4h11m-11 4h11M6 2.6v2.8m4 1.2v2.8M6 10.6v2.8" /></svg>
          General
        </button>
      {/if}
      <button type="button" class="nav-button" aria-current={!showGeneral ? "page" : undefined}
        disabled={saving} onclick={() => section = "categories"}>
        <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M1.8 4.6a1.2 1.2 0 0 1 1.2-1.2h3l1.4 1.5H13a1.2 1.2 0 0 1 1.2 1.2v5.6a1.2 1.2 0 0 1-1.2 1.2H3a1.2 1.2 0 0 1-1.2-1.2Z" /></svg>
        File categories
      </button>
    </nav>
    <footer>
      <svg aria-hidden="true" viewBox="0 0 18 18"><rect x="1.5" y="1.5" width="15" height="15" rx="3" /><path d="M6 7h6M9 4v6m-3 3h6" /></svg>
      Diffest
    </footer>
  </aside>

  <div class="settings-workspace">
    <header class="settings-bar">
      <strong>Settings</strong>
      <label class="scope-picker">
        {#if checkout === null}
          <svg aria-hidden="true" viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.8" /><ellipse cx="8" cy="8" rx="2.5" ry="5.8" /><path d="M2.2 8h11.6" /></svg>
        {:else}
          <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M1.8 4.6a1.2 1.2 0 0 1 1.2-1.2h3l1.4 1.5H13a1.2 1.2 0 0 1 1.2 1.2v5.6a1.2 1.2 0 0 1-1.2 1.2H3a1.2 1.2 0 0 1-1.2-1.2Z" /></svg>
        {/if}
        <select aria-label="Settings scope" bind:this={scopePicker} value={scope} disabled={saving}
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
    <div class="settings-scroll">
      <section class="settings-content" aria-labelledby="settings-section-title">
        {#if showGeneral}
          <h1 id="settings-section-title">General</h1>
          {#if currentSettings}
            <div class="setting">
              <div>
                <h2>Default diff layout</h2>
                <p>New reviews use this layout.</p>
              </div>
              <div class="layout-buttons" role="group" aria-label="Default diff layout">
                <button type="button" aria-pressed={currentSettings.defaultDiffLayout !== "split"}
                  disabled={saving || !tier?.version} onclick={() => changeLayout("unified")}>Unified</button>
                <button type="button" aria-pressed={currentSettings.defaultDiffLayout === "split"}
                  disabled={saving || !tier?.version} onclick={() => changeLayout("split")}>Split</button>
              </div>
            </div>
          {/if}
        {:else}
          <h1 id="settings-section-title">File categories</h1>
          <p class="intro">Group changed files in the Files sidebar. The first matching glob wins.</p>
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
          <p class="error" role="alert">{error}</p>
        {:else if !currentSettings}
          <p class="intro" role="status">Loading settings…</p>
        {/if}
        {#if tier?.error}
          <p class="error" role="alert">{tier.error}</p>
        {/if}
        {#if checkout !== null && currentSettings?.global.error}
          <p class="error" role="alert">{currentSettings.global.error}</p>
        {/if}
      </section>
    </div>
  </div>
</main>

<style>
.settings-screen {
  position: fixed;
  z-index: 5;
  inset: 0;
  display: grid;
  grid-template-columns: max(10rem, 20vw) minmax(0, 1fr);
  grid-template-rows: minmax(0, 1fr);
  background: var(--canvas);
}
.settings-sidebar {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  min-width: 0;
  border-inline-end: 1px solid var(--border);
  background: var(--surface);
}
.back-bar, .settings-bar {
  box-sizing: border-box;
  min-height: 3.9rem;
  display: flex;
  align-items: center;
  border-block-end: 1px solid var(--border);
}
.back-bar { padding: 0.45rem 0.65rem; }
.settings-bar { justify-content: space-between; gap: 0.65rem; padding: 0.45rem 1.25rem; background: var(--toolbar); }
.settings-bar strong, h2 { font-size: 0.95rem; }
.settings-workspace { display: grid; grid-template-rows: auto minmax(0, 1fr); min-width: 0; }
.settings-scroll { min-width: 0; overflow: auto; scrollbar-gutter: stable; }
.settings-content { box-sizing: border-box; max-width: 54rem; padding: 1.7rem 1.5rem; margin-inline: auto; }
h1, h2, p { margin: 0; }
h1 { font-size: 1.45rem; letter-spacing: -0.025em; }
svg { flex: none; width: 1rem; height: 1rem; fill: none; stroke: currentColor; stroke-width: 1.4; stroke-linecap: round; stroke-linejoin: round; }
nav { display: grid; align-content: start; gap: 0.2rem; padding: 0.75rem 0.65rem; overflow: auto; }
button, select { font: inherit; color: inherit; }
button:disabled, select:disabled { opacity: 0.58; }
button:not(:disabled), select:not(:disabled) { cursor: pointer; }
.nav-button {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
  padding: 0.45rem 0.5rem;
  text-align: start;
  background: transparent;
  color: var(--muted);
  border: 1px solid transparent;
  border-radius: 5px;
}
.nav-button[aria-current="page"] { color: var(--accent); background: var(--selection); font-weight: 620; }
.nav-button:not(:disabled):hover { background: var(--selection); }
footer { display: flex; align-items: center; gap: 0.5rem; padding: 0.65rem 0.9rem; border-block-start: 1px solid var(--border); font-weight: 680; }
footer svg { color: var(--accent); width: 1.125rem; height: 1.125rem; }
.scope-picker {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  box-sizing: border-box;
  min-width: min(10rem, 65%);
  max-width: min(16rem, 65%);
  padding-inline-start: 0.55rem;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: var(--canvas);
}
.scope-picker svg { color: var(--muted); }
.scope-picker select {
  flex: 1;
  min-width: 0;
  padding: 0.35rem 0.5rem;
  border: 0;
  border-radius: inherit;
  background: transparent;
  text-overflow: ellipsis;
}
.scope-picker:has(select:focus-visible) { outline: 2px solid var(--accent); outline-offset: 2px; }
.scope-picker select:focus-visible { outline: none; }
.intro, .setting p { margin-block-start: 0.25rem; color: var(--muted); }
.setting { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 0.65rem; margin-block-start: 1.3rem; padding: 0.75rem 0; border-block: 1px solid var(--border); }
.layout-buttons { display: flex; border: 1px solid var(--border); border-radius: 5px; }
.layout-buttons button { padding: 0.3rem 0.55rem; border: 0; background: transparent; }
.layout-buttons button + button { border-inline-start: 1px solid var(--border); }
.layout-buttons button[aria-pressed="true"], .layout-buttons button:not(:disabled):hover { background: var(--selection); color: var(--accent); }
.error { margin-block-start: 0.5rem; color: var(--removed-ink); overflow-wrap: anywhere; }
@media (max-width: 520px) {
  .settings-screen { grid-template-columns: max(7rem, 24vw) minmax(0, 1fr); }
  .settings-bar, .settings-content { padding-inline: 0.7rem; }
  .back-bar, nav { padding-inline: 0.25rem; }
}
</style>
