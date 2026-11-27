<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import { tick, untrack } from "svelte";
import {
  type CategoryRule,
  type CategoryRuleDraft,
  type RuleSettings,
  builtInCategories,
} from "./category-settings.js";

interface Properties {
  scope: string;
  parent: string;
  tier: RuleSettings;
  initialDraft: CategoryRuleDraft | null;
  ondraft(draft: CategoryRuleDraft | null): void;
  refresh(): void;
  save(version: string, rules: CategoryRule[] | null): Promise<void>;
  onglobal(): void;
}

let { scope, parent, tier, initialDraft, ondraft, refresh, save, onglobal }: Properties = $props();
let draft = $state(untrack(() => initialDraft));
let saving = $state(false);
let error = $state("");
let form: HTMLFormElement | null;
const groups = $derived.by(() => {
  const grouped: CategoryRule[][] = [];
  for (const rule of tier.rules) {
    const previous = grouped.at(-1);
    if (previous?.[0].category === rule.category) previous.push(rule);
    else grouped.push([rule]);
  }
  return grouped;
});
const changed = $derived(draft !== null && draft.version !== tier.version);

async function edit(): Promise<void> {
  draft = {
    rules: tier.rules.map(rule => ({
      ...rule,
      subcategory: rule.subcategory ?? "",
    })),
    version: tier.version,
  };
  ondraft(draft);
  error = "";
  await tick();
  form?.querySelector<HTMLInputElement>("input[name=glob]")?.focus();
}

async function addRule(): Promise<void> {
  draft?.rules.push({
    glob       : "",
    category   : "",
    subcategory: "",
  });
  await tick();
  form?.querySelector<HTMLInputElement>("li:last-child input[name=glob]")?.focus();
}

function discard(): void {
  draft = null;
  ondraft(null);
  error = "";
  refresh();
}

function move(index: number, destination: number): void {
  if (!draft) return;

  const [rule] = draft.rules.splice(index, 1);
  draft.rules.splice(destination, 0, rule);
}

function renameCategory(index: number, name: string): void {
  if (!draft || builtInCategories.has(draft.rules[index].category)) return;

  const first = draft.rules[index];
  const category = first.category;
  first.category = name;
  if (!category) return;

  for (const rule of draft.rules.slice(index + 1)) {
    if (rule.category !== category) break;

    rule.category = name;
  }
}

async function changeRuleCategory(index: number): Promise<void> {
  if (!draft || builtInCategories.has(draft.rules[index].category)) return;

  draft.rules[index].category = "";
  await tick();
  form?.querySelectorAll("ol > li")[index]?.querySelector<HTMLInputElement>(".category-name input")?.focus();
}

async function persist(version: string, rules: CategoryRule[] | null): Promise<void> {
  saving = true;
  error = "";
  try {
    await save(version, rules);
    draft = null;
    ondraft(null);
  } catch (error_) {
    error = String(error_);
  } finally {
    saving = false;
  }
}

function submit(event: SubmitEvent): void {
  event.preventDefault();
  if (draft?.version && !changed && !saving) {
    const rules = draft.rules.map(rule => ({
      glob       : rule.glob,
      category   : rule.category,
      subcategory: rule.subcategory?.trim() || null,
    }));
    void persist(draft.version, rules);
  }
}
</script>

<form aria-label={scope + " category rules"} bind:this={form} onsubmit={submit}>
  <div class="rule-heading">
    <div class="rule-title">
      <h2>Category rules</h2>
      <span class="rule-status">{tier.custom ? "Custom rules" : scope === "Project" ? "Inherited from Global" : "Built-in rules"}</span>
    </div>
    {#if !draft}
      <button class="primary" type="button" disabled={saving || !tier.version} onclick={edit}>
        {tier.custom ? "Edit rules" : "Customize rules"}
      </button>
    {/if}
  </div>
  {#if changed}
    <p role="alert">Settings changed on disk. Discard edits to load the file.</p>
  {:else if error}
    <p role="alert">{error}</p>
  {/if}
  {#if draft}
    <fieldset disabled={saving}>
      <legend>Rules in match order</legend>
      <datalist id="built-in-category-names">
        {#each builtInCategories as name (name)}<option value={name}></option>{/each}
      </datalist>
      <ol>
        {#each draft.rules as rule, index (index)}
          {@const startsCategory = !rule.category || draft.rules[index - 1]?.category !== rule.category}
          {@const builtIn = builtInCategories.has(rule.category)}
          <li class:category-start={startsCategory}>
            {#if startsCategory && builtIn}
              <p class="category-name"><strong>{rule.category}</strong> <small>Built-in</small></p>
            {:else if startsCategory}
              <label class="category-name">Category
                <input value={rule.category} list="built-in-category-names" required
                  onchange={(event) => renameCategory(index, event.currentTarget.value)}
                  onkeydown={(event) => {
                    if (event.key === "Enter") event.currentTarget.blur();
                  }} />
              </label>
            {/if}
            <label>Glob <input name="glob" bind:value={rule.glob} required /></label>
            <label>Subcategory <input bind:value={rule.subcategory} /></label>
            <div class="order">
              {#if !builtIn && rule.category && (draft.rules[index - 1]?.category === rule.category || draft.rules[index + 1]?.category === rule.category)}
                <button type="button" onclick={() => changeRuleCategory(index)}>Change category</button>
              {/if}
              <button
                type="button"
                aria-label={`Move rule ${index + 1} up`}
                disabled={index === 0}
                onclick={() => move(index, index - 1)}>↑</button>
              <button
                type="button"
                aria-label={`Move rule ${index + 1} down`}
                disabled={index === draft!.rules.length - 1}
                onclick={() => move(index, index + 1)}>↓</button>
              <button type="button" onclick={() => draft?.rules.splice(index, 1)}>Remove rule</button>
            </div>
          </li>
        {/each}
      </ol>
      <button type="button" onclick={addRule}>Add rule</button>
      <button class="primary" type="submit" disabled={changed}>Save rules</button>
      <button type="button" onclick={discard}>Discard edits</button>
    </fieldset>
  {:else}
    {#if tier.rules.length}
      <div class="category-groups" aria-label="Rules in match order" role="group">
        {#each groups as group}
          <section class="rule-group">
            <h3>{group[0].category}</h3>
            <ul>
              {#each group as rule}
                <li><code>{rule.glob}</code>{#if rule.subcategory}<span>{rule.subcategory}</span>{/if}</li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {:else}
      <p class="hint">No category rules.</p>
    {/if}
    {#if tier.custom}
      <button class="text-action"
        type="button"
        disabled={saving || !tier.version}
        onclick={() => tier.version && persist(tier.version, null)}>Reset to {parent} rules</button>
    {:else if scope === "Project"}
      <p class="hint">
        Uses Global rules across this project's worktrees.
        <button type="button" class="text-action" onclick={onglobal}>View Global rules</button>
      </p>
    {/if}
  {/if}
  <p class="file-path">
    <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M4 2h5l3 3v9H4zM9 2v3h3M6 8h4m-4 3h4" /></svg>
    <code>{tier.path}</code>
  </p>
</form>

<style>
form { margin-block-start: 1.35rem; }
fieldset { margin: 0; padding: 0.6rem 0.8rem; }
.rule-heading, .rule-title { display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; }
.rule-heading { justify-content: space-between; margin-block-end: 0.6rem; }
h2 { margin: 0; font-size: 0.95rem; }
p { margin-block: 0.5rem; }
.hint { color: var(--muted); }
.rule-status { padding: 0.08rem 0.38rem; border: 1px solid var(--border); border-radius: 4px; color: var(--muted); font-size: 0.8rem; }
.category-groups { border: 1px solid var(--border); border-radius: 6px; overflow: clip; }
.rule-group { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 3fr); gap: 0.65rem; padding: 0.55rem 0.65rem; }
.rule-group + .rule-group { border-block-start: 1px solid var(--border); }
.rule-group h3 { margin: 0; font-size: inherit; overflow-wrap: anywhere; }
.rule-group ul { display: grid; gap: 0.2rem; list-style: none; margin: 0; padding: 0; }
.rule-group li { display: flex; align-items: baseline; flex-wrap: wrap; gap: 0.15rem 0.55rem; margin: 0; }
.rule-group code { color: var(--accent); }
.rule-group span { color: var(--muted); overflow-wrap: anywhere; }
code { overflow-wrap: anywhere; }
.file-path { display: flex; align-items: flex-start; gap: 0.4rem; margin-block-start: 1.25rem; color: var(--muted); font-size: 0.82rem; }
.file-path svg { flex: none; width: 0.95rem; height: 0.95rem; stroke: currentColor; stroke-width: 1.4; }
.category-groups + .text-action { margin-block-start: 0.45rem; }
fieldset { border: 1px solid var(--border); min-inline-size: 0; }
ol { padding-inline-start: 1.5rem; }
li { margin-block-end: 0.5rem; }
li > label { width: 32%; box-sizing: border-box; padding-inline-end: 0.35rem; overflow-wrap: anywhere; }
.category-start:not(:first-child) { padding-block-start: 0.55rem; border-block-start: 1px solid var(--border); }
li > .category-name { display: block; margin-block: 0 0.35rem; }
.category-name small { margin-inline-start: 0.25rem; color: var(--muted); }
.order { margin-block-start: 0.35rem; }
label, .order { display: inline-block; font-size: 0.8rem; }
input { display: block; width: 100%; }
button, input {
  box-sizing: border-box;
  font: inherit;
  color: inherit;
  background: var(--canvas);
  border: 1px solid var(--border);
  border-radius: 0.35rem;
  padding: 0.35rem 0.45rem;
}
button.primary, button:not(:disabled):hover { background: var(--selection); }
button.primary, button.text-action { color: var(--accent); }
button.text-action { border: 0; background: transparent; padding: 0; text-decoration: underline; text-underline-offset: 3px; }
button:not(:disabled) { cursor: pointer; }
</style>
