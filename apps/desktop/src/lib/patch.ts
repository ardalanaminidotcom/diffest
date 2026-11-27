/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */

interface PatchLine {
  before?: number;
  after?: number;
  text: string;
  change?: "removed" | "added";
}

interface Hunk {
  lines: PatchLine[];
}

interface SplitRow {
  before: PatchLine | undefined;
  after: PatchLine | undefined;
}

interface PatchSummary {
  added: number;
  removed: number;
  lineCount: number;
}

interface ParsedPatch {
  hunks: Hunk[];
  summary: PatchSummary;
  tooManyLines: boolean;
}

const hunkHeader = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/u;
const maxStructuredLines = 5000;

function scanPatch(patch: string, render: boolean): ParsedPatch {
  const hunks: Hunk[] = [];
  const lines = patch.replace(/\n$/u, "").split("\n");
  const summary: PatchSummary = {
    added    : 0,
    removed  : 0,
    lineCount: lines.length,
  };
  const tooManyLines = lines.length > maxStructuredLines;
  const collectHunks = render && !tooManyLines;
  let hunk: Hunk | undefined;
  let before = 0;
  let after = 0;
  let inHunk = false;

  for (const line of lines) {
    const range = hunkHeader.exec(line);
    if (range) {
      inHunk = true;
      if (collectHunks) {
        hunk = { lines: [] };
        hunks.push(hunk);
        before = Number(range[1]);
        after = Number(range[2]);
      }
      continue;
    }
    if (line.startsWith("diff --")) inHunk = false;

    if (!inHunk) continue;

    const marker = line.charAt(0);
    if (marker === "-") summary.removed++;
    if (marker === "+") summary.added++;
    if (!hunk) continue;

    const text = line.slice(1);
    switch (marker) {
      case "-": {
        hunk.lines.push({
          before: before++,
          text,
          change: "removed",
        });
        break;
      }
      case "+": {
        hunk.lines.push({
          after : after++,
          text,
          change: "added",
        });
        break;
      }
      case " ":
      case "": {
        hunk.lines.push({
          before: before++,
          after : after++,
          text,
        });
        break;
      }
      case "\\": {
        const previous = hunk.lines.at(-1);
        if (previous) previous.text += `\n${ line }`;
        break;
      }
      default: {
        // Preserve unusual patch formats in the raw view.
        hunks.length = 0;
        return {
          hunks,
          summary,
          tooManyLines,
        };
      }
    }
  }
  return {
    hunks,
    summary,
    tooManyLines,
  };
}

function parsePatch(patch: string): ParsedPatch {
  return scanPatch(patch, true);
}

function summarizePatch(patch: string): PatchSummary {
  return scanPatch(patch, false).summary;
}

function omitHunkHeaders(patch: string): string {
  return patch.split("\n").filter(line => !hunkHeader.test(line))
    .join("\n");
}

function splitHunk(lines: readonly PatchLine[]): SplitRow[] {
  const rows: SplitRow[] = [];
  let removals: PatchLine[] = [];
  let additions: PatchLine[] = [];
  function flush(): void {
    for (let index = 0; index < Math.max(removals.length, additions.length); index++) {
      rows.push({
        before: removals[index],
        after : additions[index],
      });
    }
    removals = [];
    additions = [];
  }

  for (const line of lines) {
    if (line.change === "removed") {
      removals.push(line);
    } else if (line.change === "added") {
      additions.push(line);
    } else {
      flush();
      rows.push({
        before: line,
        after : line,
      });
    }
  }
  flush();
  return rows;
}

export type { PatchLine };
export { omitHunkHeaders, parsePatch, splitHunk, summarizePatch };
