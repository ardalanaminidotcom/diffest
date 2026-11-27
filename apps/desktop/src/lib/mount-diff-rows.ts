/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { Action } from "svelte/action";
import hljs from "highlight.js/lib/common";

interface Options {
  blocks: string[];
  windowRows: boolean;
  beforeLanguage: string;
  afterLanguage: string;
  highlight: boolean;
}

const highlightLineLimit = 2000;
const highlightBatchSize = 128;
const measurementBatchSize = 8;
const resizeDelay = 80;

function isSelected(node: Element): boolean {
  const selection = node.ownerDocument.getSelection();
  return Boolean(selection && !selection.isCollapsed && selection.containsNode(node, true));
}

function windowDiffBlocks(node: HTMLElement, blocks: string[], onMount: () => void): () => void {
  const slots = blocks.map((html) => {
    const element = node.ownerDocument.createElement("div");
    node.append(element);
    return {
      element,
      html,
      near: false,
    };
  });
  const slotsByElement = new Map<Element, typeof slots[number]>(slots.map(slot => [slot.element, slot]));

  let width = -1;
  let resizeTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
  function measure(): void {
    const nextWidth = node.getBoundingClientRect().width;
    if (nextWidth === width) return;

    width = nextWidth;

    // Measure a bounded batch at the real table width before releasing its offscreen rows.
    for (let index = 0; index < slots.length; index += measurementBatchSize) {
      const batch = slots.slice(index, index + measurementBatchSize);
      const temporary = batch.filter(slot => !slot.element.firstElementChild);
      for (const { element, html } of temporary) element.innerHTML = html;
      const heights = batch.map(slot => slot.element.firstElementChild?.getBoundingClientRect().height ?? 0);
      for (const [offset, slot] of batch.entries()) slot.element.style.blockSize = `${ heights[offset] }px`;
      for (const { element } of temporary) element.replaceChildren();
    }
  }

  measure();
  const resize = new globalThis.ResizeObserver(() => {
    if (node.getBoundingClientRect().width === width) return;

    if (resizeTimer !== null) globalThis.clearTimeout(resizeTimer);
    resizeTimer = globalThis.setTimeout(measure, resizeDelay);
  });
  resize.observe(node);

  function evictOffscreen(): void {
    for (const { element, near } of slots) {
      if (near || !element.firstElementChild || isSelected(element)) continue;

      element.replaceChildren();
    }
  }

  const observer = new globalThis.IntersectionObserver((entries) => {
    for (const { target, isIntersecting } of entries) {
      const slot = slotsByElement.get(target);
      if (!slot) continue;

      slot.near = isIntersecting;
      if (isIntersecting && !target.firstElementChild) target.innerHTML = slot.html;
    }
    evictOffscreen();
    onMount();
  }, {
    root      : node.closest(".file-stream"),
    rootMargin: "240px",
  });
  for (const { element } of slots) observer.observe(element);
  node.ownerDocument.addEventListener("selectionchange", evictOffscreen);

  return () => {
    observer.disconnect();
    resize.disconnect();
    if (resizeTimer !== null) globalThis.clearTimeout(resizeTimer);
    node.ownerDocument.removeEventListener("selectionchange", evictOffscreen);
  };
}

const mountDiffRows: Action<HTMLElement, Options> = (node, initial) => {
  let options = initial;
  let cancelWindow: (() => void) | null = null;
  let timer: ReturnType<typeof globalThis.setTimeout> | null = null;
  const pending = new WeakMap<HTMLTableElement, HTMLElement[]>();

  function scheduleHighlight(): void {
    if (!options.highlight || timer !== null) return;

    timer = globalThis.setTimeout(() => {
      timer = null;
      if (!options.highlight) return;

      for (const table of node.querySelectorAll("table")) {
        let lines = pending.get(table);
        if (!lines) {
          lines = [...table.querySelectorAll("code")].toReversed();
          pending.set(table, lines);
        }
        if (lines.length === 0) continue;

        const heights = new Map<HTMLTableRowElement, number>();
        const changedLines = [];
        for (const line of lines.splice(-highlightBatchSize)) {
          const language = line.dataset.side === "before" ? options.beforeLanguage : options.afterLanguage;
          const text = line.textContent;
          const row = line.closest("tr");
          if (!row || !text || text.length > highlightLineLimit || isSelected(line)) continue;
          if (!hljs.getLanguage(language)) continue;

          if (!heights.has(row)) heights.set(row, row.getBoundingClientRect().height);
          changedLines.push({
            line,
            row,
            text,
            markup: hljs.highlight(text, { language }).value,
          });
        }
        for (const { line, markup } of changedLines) line.innerHTML = markup;

        // Some token boundaries can change where the browser wraps a line. Keep those rows plain.
        const changedRows = new Set<HTMLTableRowElement>();
        for (const [row, height] of heights) {
          const currentHeight = row.getBoundingClientRect().height;
          if (currentHeight !== height) changedRows.add(row);
        }
        for (const { line, row, text } of changedLines) {
          if (!changedRows.has(row)) continue;

          line.textContent = text;
        }

        scheduleHighlight();
        break;
      }
    }, 0);
  }

  function render(): void {
    cancelWindow?.();
    node.replaceChildren();
    if (options.windowRows) cancelWindow = windowDiffBlocks(node, options.blocks, scheduleHighlight);
    else node.innerHTML = options.blocks.join("");
    scheduleHighlight();
  }

  render();
  return {
    update(next) {
      const reflow = next.blocks !== options.blocks;
      options = next;
      if (reflow) render();
      else scheduleHighlight();
    },
    destroy() {
      cancelWindow?.();
      if (timer !== null) globalThis.clearTimeout(timer);
    },
  };
};

export { mountDiffRows };
