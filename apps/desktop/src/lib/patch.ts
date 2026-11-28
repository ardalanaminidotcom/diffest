/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */
import type { ChangedFile } from "./revisions.js";
import { type CodeViewItem, type FileDiffMetadata, parsePatchFiles } from "@pierre/diffs";

interface PatchSummary {
  added: number;
  removed: number;
}

interface ReviewPatch extends PatchSummary {
  item: Omit<Extract<CodeViewItem<undefined>, { type: "diff"; }>, "id">
    | Omit<Extract<CodeViewItem<undefined>, { type: "file"; }>, "id">;
  modes: string;
}

function parseFiles(patch: string): FileDiffMetadata[] {
  return parsePatchFiles(patch, { throwOnError: true }).flatMap(result => result.files);
}

function parseReviewPatch(file: ChangedFile): ReviewPatch {
  const { patch, path, previousPath } = file;

  // Older snapshots can contain only hunks. The original path is applied to the parsed result.
  const input = patch.startsWith("@@ ") ? `--- a/file\n+++ b/file\n${ patch }` : patch;
  let files: FileDiffMetadata[] = [];
  try {
    if (input) files = parseFiles(input);
  } catch {
    // Git can omit standard path prefixes or the context marker on empty lines.
    const compatible = input.replaceAll(/^diff --git .+$/gmu, "diff --git a/file b/file")
      .replaceAll(/\n(?=\n)/gu, "\n ");
    if (compatible !== input) {
      try {
        files = parseFiles(compatible);
      } catch {
        // Keep the original Git output readable when Pierre cannot parse it.
      }
    }
  }

  let added = 0;
  let removed = 0;
  for (const diff of files) {
    for (const hunk of diff.hunks) {
      added += hunk.additionLines;
      removed += hunk.deletionLines;
    }
  }
  const diff = files.at(0);
  if (files.length === 1 && diff && diff.hunks.length > 0) {
    const fileDiff = {
      ...diff,
      name: path,
    };
    if (previousPath) fileDiff.prevName = previousPath;
    else delete fileDiff.prevName;
    return {
      added,
      removed,
      item: {
        type: "diff",
        fileDiff,
      },
      modes: patch.match(/^(?:old|new) mode [0-7]{6}$/gmu)?.join("\n") ?? "",
    };
  }

  return {
    added,
    removed,
    item: {
      type: "file",
      file: {
        name    : path,
        contents: patch,
        lang    : "text",
      },
    },
    modes: "",
  };
}

export { parseReviewPatch };
export type { PatchSummary, ReviewPatch };
