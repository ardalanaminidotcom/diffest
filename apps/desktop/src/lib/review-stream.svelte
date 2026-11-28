<script lang="ts">
/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { ReviewPatch } from "./patch.js";
import type { ChangedFile, Comparison, DiffLayout, WorktreePatch } from "./revisions.js";
import { type CodeViewItem, type CodeViewOptions, CodeView, DEFAULT_CODE_VIEW_FILE_METRICS } from "@pierre/diffs";
import { mount, onMount, tick, unmount } from "svelte";
import { SvelteMap } from "svelte/reactivity";
import { type FileCategory, generatedCategory } from "./category-settings.js";
import ReviewFile, { type ReviewEntry, type ReviewSlots } from "./review-file.svelte";

interface Properties {
  files: ChangedFile[];
  order: number[];
  categories: FileCategory[] | null;
  reviewedFiles: Set<number>;
  worktree: WorktreePatch[] | null;
  comparison: Comparison | null;
  checkout: string;
  layout: DiffLayout;
  patches: ReviewPatch[];
  busy: boolean;
  onreview(index: number): void;
  onstage(patch: WorktreePatch): void;
}

interface VersionedItem {
  item: CodeViewItem<undefined>;
  data: ReviewPatch;
}

interface MountedFile {
  slots: ReviewSlots;
  controls?: Record<string, unknown>;
}

const {
  files, order, categories, reviewedFiles, worktree, comparison, checkout, layout, patches, busy, onreview, onstage,
}: Properties = $props();
const overrides = new SvelteMap<string, boolean>();
const itemsById = new Map<string, VersionedItem>();
const mounted = new Map<string, MountedFile>();
let root: HTMLElement;
let viewer: CodeView | null = null;
let pendingFocus: string | null = null;
let retainedFocus: HTMLElement | null = null;

const entries = $derived.by(() => {
  const result = new Map<string, ReviewEntry>();
  for (const index of order) {
    const file = files[index];
    const patch = worktree?.[index];
    const category = categories === null ? null : categories[index]?.category;
    const reviewed = patch?.reviewed ?? reviewedFiles.has(index);
    const id = patch?.fingerprint ?? String(index);
    const collapseByDefault = reviewed || category === generatedCategory;
    result.set(id, {
      id,
      index,
      file,
      patch,
      category,
      reviewed,
      collapsed: category === null || (overrides.get(id) ?? collapseByDefault),
    });
  }
  return result;
});

function slotsFor(id: string): ReviewSlots {
  const existing = mounted.get(id);
  if (existing) return existing.slots;

  const ownerDocument = root.ownerDocument;
  const slots = {
    prefix  : ownerDocument.createElement("div"),
    metadata: ownerDocument.createElement("div"),
    body    : ownerDocument.createElement("div"),
  };
  mounted.set(id, { slots });
  return slots;
}

function headerSlot(id: string, part: "prefix" | "metadata"): HTMLElement {
  const slot = slotsFor(id)[part];
  const active = root.ownerDocument.activeElement;

  // Pierre reparents the slotted element while updating its header.
  if (active instanceof globalThis.HTMLElement && slot.contains(active)) retainedFocus = active;
  return slot;
}

function restoreFocus(): void {
  if (pendingFocus !== null) {
    const button = mounted.get(pendingFocus)?.slots.prefix.querySelector<HTMLButtonElement>("[data-review-disclosure]");
    if (!button?.isConnected) return;
    button.focus({ preventScroll: true });
    if (button.ownerDocument.activeElement === button) pendingFocus = null;
  } else if (retainedFocus?.isConnected) {
    retainedFocus.focus({ preventScroll: true });
  }
  retainedFocus = null;
}

function itemFor(entry: ReviewEntry): CodeViewItem<undefined> {
  const { id, index, collapsed } = entry;
  const data = patches[index];
  const previous = itemsById.get(id);
  if (previous?.data === data && previous.item.collapsed === collapsed) return previous.item;

  const content = data.item;
  const identity = {
    id,
    collapsed,
    version: (previous?.item.version ?? 0) + 1,
  };
  const item: CodeViewItem<undefined> = content.type === "diff"
    ? {
        ...content,
        ...identity,
        annotations: [
          {
            lineNumber: 0,
            side      : "additions",
          },
        ],
      }
    : {
        ...content,
        ...identity,
        annotations: [{ lineNumber: 0 }],
      };
  itemsById.set(id, {
    item,
    data,
  });
  return item;
}

function mountControls(host: HTMLElement, initial: ReviewEntry): void {
  const { id } = initial;
  const slots = slotsFor(id);
  host.classList.add("review-file", "block", "rounded-[7px]", "border", "border-divider");
  mounted.set(id, {
    slots,
    controls: mount(ReviewFile, {
      target: host,
      props : {
        get entry() {
          return entries.get(id) ?? initial;
        },
        host,
        slots,
        get checkout() {
          return checkout;
        },
        get comparison() {
          return comparison;
        },
        get modes() {
          return patches[(entries.get(id) ?? initial).index]?.modes ?? "";
        },
        get busy() {
          return busy;
        },
        ontoggle() {
          overrides.set(id, !(entries.get(id) ?? initial).collapsed);
        },
        onreview,
        onstage,
      },
    }),
  });
}

onMount(() => {
  const header = root.ownerDocument.createElement("p");
  header.className = "m-0 px-2 pb-2 text-[0.8rem] text-muted";
  const options: CodeViewOptions<undefined, undefined> = {
    // Include both 1px borders when CodeView estimates the file height.
    itemMetrics         : { diffHeaderHeight: DEFAULT_CODE_VIEW_FILE_METRICS.diffHeaderHeight + 2 },
    overflow            : "wrap",
    stickyHeaders       : true,
    renderCodeViewHeader: () => header,
    renderHeaderPrefix  : (file, { item }) => headerSlot(item.id, "prefix"),
    renderHeaderMetadata: (file, { item }) => headerSlot(item.id, "metadata"),
    renderAnnotation    : (annotation, file, { item }) => slotsFor(item.id).body,
    onItemCollapsedChange(item, next) {
      overrides.set(item.id, next.collapsed === true);
      const previous = itemsById.get(item.id);
      if (previous) previous.item = next;
    },
    // eslint-disable-next-line max-params -- CodeView passes the item context as the fourth argument.
    onPostRender(host, instance, phase, { item }) {
      const id = item.id;
      if (phase === "unmount") {
        const controls = mounted.get(id)?.controls;
        if (controls) void unmount(controls);
        mounted.delete(id);
        return;
      }
      if (phase === "mount") {
        const initial = entries.get(id);
        if (initial) mountControls(host, initial);
      }
      if (pendingFocus === id || retainedFocus) void tick().then(restoreFocus);
    },
  };
  const view = new CodeView(options);
  viewer = view;
  view.setup(root);
  const unsubscribe = view.subscribeToScroll(() => {
    if (pendingFocus !== null) void tick().then(restoreFocus);
  });

  let previousEntries = new Map<string, ReviewEntry>();
  const stop = $effect.root(() => {
    $effect(() => {
      view.setOptions({
        ...options,
        diffStyle: layout,
      });
    });
    $effect(() => {
      if (comparison) {
        const caption = comparison.mode === "divergence" ? "Changes since divergence" : "Changes between revisions";
        header.textContent = `${ caption } · ${ comparison.start.slice(0, 7) } ← ${ comparison.end.slice(0, 7) }`;
      } else {
        header.textContent = "Uncommitted changes · staged and unstaged";
      }
    });
    $effect(() => {
      const next = entries;

      // Reset a manual override when a review mark changes. Clearing a mark opens the file.
      for (const [id, entry] of next) {
        const previous = previousEntries.get(id);
        if (previous && previous.reviewed !== entry.reviewed) {
          if (entry.reviewed) overrides.delete(id);
          else overrides.set(id, false);
        }
      }
      previousEntries = next;
      view.setItems([...entries.values()].map(itemFor));
      for (const id of itemsById.keys()) {
        if (next.has(id)) continue;
        itemsById.delete(id);
        overrides.delete(id);
      }
    });
  });

  return () => {
    stop();
    unsubscribe();
    view.cleanUp();
    viewer = null;
  };
});

export function jumpTo(index: number): void {
  const id = worktree?.[index]?.fingerprint ?? String(index);
  pendingFocus = id;
  retainedFocus = null;
  viewer?.scrollTo({
    type : "item",
    id,
    align: "start",
  });
  restoreFocus();
}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Allow keyboard scrolling of the file stream.) -->
<section class="file-stream min-h-0 min-w-0 overflow-auto [scrollbar-gutter:stable] focus-visible:-outline-offset-2
  [--diffs-font-family:var(--font-diff)]" aria-label="File diffs" tabindex="0" bind:this={root}></section>
