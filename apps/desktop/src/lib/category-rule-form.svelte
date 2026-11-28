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
const inputStyle = "control box-border block w-full rounded-[0.35rem] px-[0.45rem] py-[0.35rem]";
const buttonBase = "control box-border rounded-[0.35rem] enabled:cursor-pointer enabled:hover:bg-selection";
const buttonStyle = `${ buttonBase } px-[0.45rem] py-[0.35rem]`;
const primaryButton = `${ buttonStyle } bg-selection text-accent`;
const textAction = `${ buttonBase } border-0 bg-transparent p-0 text-accent underline underline-offset-[3px]`;
const fieldLabel = "box-border w-[32%] pe-[0.35rem] text-[0.8rem] wrap-anywhere";
</script>

<form class="mbs-[1.35rem]" aria-label={scope + " category rules"} bind:this={form} onsubmit={submit}>
  <div class="mbe-[0.6rem] flex flex-wrap items-center justify-between gap-2">
    <div class="flex flex-wrap items-center gap-2">
      <h2 class="m-0 text-[0.95rem]">Category rules</h2>
      <span class="rounded-sm border border-divider px-[0.38rem] py-[0.08rem] text-[0.8rem] text-muted">{tier.custom ? "Custom rules" : scope === "Project" ? "Inherited from Global" : "Built-in rules"}</span>
    </div>
    {#if !draft}
      <button class={primaryButton} type="button" disabled={saving || !tier.version} onclick={edit}>
        {tier.custom ? "Edit rules" : "Customize rules"}
      </button>
    {/if}
  </div>
  {#if changed}
    <p class="my-2" role="alert">Settings changed on disk. Discard edits to load the file.</p>
  {:else if error}
    <p class="my-2" role="alert">{error}</p>
  {/if}
  {#if draft}
    <fieldset class="m-0 min-inline-0 border border-divider px-[0.8rem] py-[0.6rem]" disabled={saving}>
      <legend>Rules in match order</legend>
      <datalist id="built-in-category-names">
        {#each builtInCategories as name (name)}<option value={name}></option>{/each}
      </datalist>
      <ol class="ps-6">
        {#each draft.rules as rule, index (index)}
          {@const startsCategory = !rule.category || draft.rules[index - 1]?.category !== rule.category}
          {@const builtIn = builtInCategories.has(rule.category)}
          <li class={["mbe-2", startsCategory && "not-first:border-bs not-first:border-divider not-first:pbs-[0.55rem]"]}>
            {#if startsCategory && builtIn}
              <p class="m-0 mbe-[0.35rem] block"><strong>{rule.category}</strong> <small class="ms-1 text-muted">Built-in</small></p>
            {:else if startsCategory}
              <label class={[fieldLabel, "category-name mbe-[0.35rem] block"]}>Category
                <input class={inputStyle} value={rule.category} list="built-in-category-names" required
                  onchange={(event) => renameCategory(index, event.currentTarget.value)}
                  onkeydown={(event) => {
                    if (event.key === "Enter") event.currentTarget.blur();
                  }} />
              </label>
            {/if}
            <label class={[fieldLabel, "inline-block"]}>Glob <input class={inputStyle} name="glob" bind:value={rule.glob} required /></label>
            <label class={[fieldLabel, "inline-block"]}>Subcategory <input class={inputStyle} bind:value={rule.subcategory} /></label>
            <div class="mbs-[0.35rem] inline-block text-[0.8rem]">
              {#if !builtIn && rule.category && (draft.rules[index - 1]?.category === rule.category || draft.rules[index + 1]?.category === rule.category)}
                <button class={buttonStyle} type="button" onclick={() => changeRuleCategory(index)}>Change category</button>
              {/if}
              <button
                class={buttonStyle}
                type="button"
                aria-label={`Move rule ${index + 1} up`}
                disabled={index === 0}
                onclick={() => move(index, index - 1)}>↑</button>
              <button
                class={buttonStyle}
                type="button"
                aria-label={`Move rule ${index + 1} down`}
                disabled={index === draft!.rules.length - 1}
                onclick={() => move(index, index + 1)}>↓</button>
              <button class={buttonStyle} type="button" onclick={() => draft?.rules.splice(index, 1)}>Remove rule</button>
            </div>
          </li>
        {/each}
      </ol>
      <button class={buttonStyle} type="button" onclick={addRule}>Add rule</button>
      <button class={primaryButton} type="submit" disabled={changed}>Save rules</button>
      <button class={buttonStyle} type="button" onclick={discard}>Discard edits</button>
    </fieldset>
  {:else}
    {#if tier.rules.length}
      <div class="overflow-clip rounded-md border border-divider" aria-label="Rules in match order" role="group">
        {#each groups as group}
          <section class="grid grid-cols-[minmax(0,1fr)_minmax(0,3fr)] gap-[0.65rem] px-[0.65rem] py-[0.55rem] not-first:border-bs not-first:border-divider">
            <h3 class="m-0 text-[length:inherit] wrap-anywhere">{group[0].category}</h3>
            <ul class="m-0 grid list-none gap-[0.2rem] p-0">
              {#each group as rule}
                <li class="m-0 flex flex-wrap items-baseline gap-x-[0.55rem] gap-y-[0.15rem]"><code class="wrap-anywhere text-accent">{rule.glob}</code>{#if rule.subcategory}<span class="wrap-anywhere text-muted">{rule.subcategory}</span>{/if}</li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {:else}
      <p class="my-2 text-muted">No category rules.</p>
    {/if}
    {#if tier.custom}
      <button class={[textAction, tier.rules.length > 0 && "mbs-[0.45rem]"]}
        type="button"
        disabled={saving || !tier.version}
        onclick={() => tier.version && persist(tier.version, null)}>Reset to {parent} rules</button>
    {:else if scope === "Project"}
      <p class="my-2 text-muted">
        Uses Global rules across this project's worktrees.
        <button type="button" class={textAction} onclick={onglobal}>View Global rules</button>
      </p>
    {/if}
  {/if}
  <p class="mbs-5 mbe-2 flex items-start gap-[0.4rem] text-[0.82rem] text-muted">
    <svg class="size-[0.95rem] flex-none stroke-current stroke-[1.4]" aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M4 2h5l3 3v9H4zM9 2v3h3M6 8h4m-4 3h4" /></svg>
    <code class="wrap-anywhere">{tier.path}</code>
  </p>
</form>
