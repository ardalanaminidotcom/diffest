<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { CategoryRule, RuleSettings } from "./category-settings.js";

interface Properties {
  scope: string;
  parent: string;
  tier: RuleSettings;
  refresh(): void;
  save(version: string, rules: CategoryRule[] | null): Promise<void>;
}

let { scope, parent, tier, refresh, save }: Properties = $props();
let draft = $state<CategoryRule[] | null>(null);
let draftVersion = $state<string | null>(null);
let saving = $state(false);
let error = $state("");
let changed = $derived(draft !== null && draftVersion !== tier.version);

function edit(): void {
  draft = tier.rules.map(rule => ({
    ...rule,
    subcategory: rule.subcategory ?? "",
  }));
  draftVersion = tier.version;
  error = "";
}

function discard(): void {
  draft = null;
  error = "";
  refresh();
}

function move(index: number, destination: number): void {
  if (!draft) return;

  const [rule] = draft.splice(index, 1);
  draft.splice(destination, 0, rule);
}

async function persist(version: string, rules: CategoryRule[] | null): Promise<void> {
  saving = true;
  error = "";
  try {
    await save(version, rules);
    draft = null;
  } catch (error_) {
    error = String(error_);
  } finally {
    saving = false;
  }
}

function submit(event: SubmitEvent): void {
  event.preventDefault();
  if (draft && draftVersion && !changed && !saving) {
    const rules = draft.map(rule => ({
      glob       : rule.glob,
      category   : rule.category,
      subcategory: rule.subcategory?.trim() || null,
    }));
    void persist(draftVersion, rules);
  }
}
</script>

<form aria-label={scope + " category rules"} onsubmit={submit}>
  <h3>{scope} category rules</h3>
  <code>{tier.path}</code>
  <p>{tier.custom ? "Custom rules" : `Inherits ${parent} rules`}</p>
  {#if changed}
    <p role="alert">Settings changed on disk. Discard edits to load the file.</p>
  {:else if error}
    <p role="alert">{error}</p>
  {/if}
  {#if draft}
    <fieldset disabled={saving}>
      <legend>Rules in match order</legend>
      <ol>
        {#each draft as rule, index (index)}
          <li>
            <label>Glob <input name="glob" bind:value={rule.glob} required /></label>
            <label>Category <input bind:value={rule.category} required /></label>
            <label>Subcategory <input bind:value={rule.subcategory} /></label>
            <div class="order">
              <button
                type="button"
                aria-label={`Move rule ${index + 1} up`}
                disabled={index === 0}
                onclick={() => move(index, index - 1)}>↑</button>
              <button
                type="button"
                aria-label={`Move rule ${index + 1} down`}
                disabled={index === draft!.length - 1}
                onclick={() => move(index, index + 1)}>↓</button>
              <button type="button" onclick={() => draft?.splice(index, 1)}>Remove rule</button>
            </div>
          </li>
        {/each}
      </ol>
      <button type="button" onclick={() => draft?.push({ glob: "", category: "", subcategory: "" })}>Add rule</button>
      <button type="submit" disabled={changed}>Save rules</button>
      <button type="button" onclick={discard}>Discard edits</button>
    </fieldset>
  {:else}
    <button type="button" disabled={saving || !tier.version} onclick={edit}>
      {tier.custom ? "Edit rules" : "Customize rules"}
    </button>
    {#if tier.custom}
      <button
        type="button"
        disabled={saving || !tier.version}
        onclick={() => tier.version && persist(tier.version, null)}>Reset to {parent} rules</button>
    {/if}
  {/if}
</form>

<style>
form, fieldset { margin-block: 0.75rem; }
h3, p { margin-block: 0.4rem; }
h3 { font-size: 1rem; }
code { overflow-wrap: anywhere; }
fieldset { border: 1px solid var(--border); min-inline-size: 0; }
ol { padding-inline-start: 1.5rem; }
li { margin-block-end: 0.5rem; }
label, .order { display: inline-block; font-size: 0.8rem; }
input { display: block; max-width: 100%; }
button, input {
  box-sizing: border-box;
  font: inherit;
  color: inherit;
  background: Canvas;
  border: 1px solid var(--border);
  border-radius: 0.35rem;
  padding: 0.35rem 0.45rem;
}
button:not(:disabled) { cursor: pointer; }
</style>
