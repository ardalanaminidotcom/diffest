import type { DiffLayout } from "./revisions.js";
import { type PatchLine, splitHunk } from "./patch.js";

const rowsPerBlock = 128;

function escapeText(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("\r", "&#13;");
}

function code(line: PatchLine, before: boolean): string {
  return `<code data-side="${ before ? "before" : "after" }">${ escapeText(line.text) }</code>`;
}

function unifiedRow(line: PatchLine, boundary: string): string {
  const change = line.change ?? "";
  return `<tr class="${ change } ${ boundary }"><td class="line-number">${ line.before ?? "" }</td>
    <td class="line-number">${ line.after ?? "" }</td>
    <td class="line ${ change }">${ code(line, change === "removed") }</td></tr>`;
}

function splitCell(line: PatchLine | undefined, before: boolean): string {
  const change = line?.change ?? "";
  return `<td class="line-number ${ change }">${ (before ? line?.before : line?.after) ?? "" }</td>
    <td class="line ${ line ? change : "blank" }">${ line ? code(line, before) : "" }</td>`;
}

function renderDiffRows(hunks: ReadonlyArray<readonly PatchLine[]>, layout: DiffLayout, windowRows: boolean): string[] {
  const split = layout === "split";
  const rows = hunks.flatMap((lines, index) => {
    function boundary(rowIndex: number): string {
      return index > 0 && rowIndex === 0 ? "hunk-start" : "";
    }

    return split
      ? splitHunk(lines).map((row, rowIndex) => {
          const cells = splitCell(row.before, true) + splitCell(row.after, false);
          return `<tr class="${ boundary(rowIndex) }">${ cells }</tr>`;
        })
      : lines.map((line, rowIndex) => unifiedRow(line, boundary(rowIndex)));
  });
  const label = split ? "Split patch" : "Unified patch";
  const columns = split
    ? '<colgroup><col class="number"><col></colgroup><colgroup><col class="number"><col></colgroup>'
    : '<colgroup><col class="number"><col class="number"><col></colgroup>';
  const heading = '<thead><tr><th scope="colgroup" colspan="2">Before</th>'
    + '<th scope="colgroup" colspan="2">After</th></tr></thead>';
  const blockSize = windowRows ? rowsPerBlock : rows.length || 1;
  const blocks: string[] = [];
  for (let index = 0; index < rows.length || index === 0; index += blockSize) {
    const continuation = index > 0 ? heading.replace("<thead>", '<thead class="block-heading">') : heading;
    const header = split ? continuation : "";
    const body = rows.slice(index, index + blockSize).join("");
    const opening = `<table aria-label="${ label }" class="${ split ? "split" : "" }">`;
    blocks.push(`${ opening }${ columns }${ header }<tbody>${ body }</tbody></table>`);
  }
  return blocks;
}

function renderRawBlocks(text: string): string[] {
  const lines = text.split("\n");
  const blocks: string[] = [];
  for (let index = 0; index < lines.length; index += rowsPerBlock) {
    const block = escapeText(lines.slice(index, index + rowsPerBlock).join("\n"));
    blocks.push(`<pre aria-label="Unified patch" tabindex="0"><span>${ block }</span></pre>`);
  }
  return blocks;
}

export { renderDiffRows, renderRawBlocks };
