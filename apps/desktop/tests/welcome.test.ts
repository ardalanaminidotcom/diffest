import { invoke } from "@tauri-apps/api/core";
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";
import { ssr } from "../src/routes/+layout.js";
import Page from "../src/routes/+page.svelte";

it("renders on the client for Tauri's static frontend", () => {
  expect(ssr).toBe(false);
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const revisionCommands = vi.fn();
const layoutCommands = vi.fn();
let defaultDiffLayout = "unified";
let loadWorktreePatches = false;
beforeEach(() => {
  revisionCommands.mockReset();
  layoutCommands.mockReset().mockResolvedValue(null);
  defaultDiffLayout = "unified";
  loadWorktreePatches = false;
  vi.mocked(invoke).mockImplementation(async (command, parameters) => {
    if (command === "read_review_layout" || command === "set_review_layout") return layoutCommands(command, parameters);
    if (command === "save_default_diff_layout") {
      if (parameters && "layout" in parameters) defaultDiffLayout = String(parameters.layout);
      return null;
    }
    if (command === "read_file_details") {
      const source = parameters && "source" in parameters ? parameters.source : null;
      if (source && typeof source === "object" && "path" in source) {
        if (source.path === "mode.png") {
          const image = {
            path    : "mode.png",
            format  : "PNG",
            byteSize: 48,
            preview : "data:image/png;base64,iVBORw0KGgo=",
          };
          return {
            before: image,
            after : image,
          };
        }
        if (source.path === "new.txt") {
          return {
            before: null,
            after : { byteSize: 1432 },
          };
        }
        if (source.path === "after.txt") {
          return {
            before: { byteSize: 440 },
            after : { byteSize: 120 },
          };
        }
      }
      return {
        before: null,
        after : null,
      };
    }

    if (command === "open_worktree_review" && !loadWorktreePatches) return [];
    if (command !== "read_category_settings") return revisionCommands(command, parameters);

    const filePaths = parameters && "paths" in parameters ? parameters.paths : null;
    const paths: unknown[] = Array.isArray(filePaths) ? filePaths : [];
    return {
      global: {
        path   : "/home/demo/.diffest/settings.json",
        version: "missing",
        error  : null,
        custom : false,
        rules  : [],
      },
      defaultDiffLayout,
      project   : null,
      categories: paths.map(path => ({
        category   : path === "after.txt" ? "Tests" : "Other",
        subcategory: path === "after.txt" ? "Unit" : null,
      })),
    };
  });
});

function render(): HTMLDivElement {
  const target = document.createElement("div");
  document.body.append(target);
  const page = mount(Page, { target });
  onTestFinished(async () => {
    await unmount(page);
    target.remove();
  });
  flushSync();
  return target;
}

function element(target: HTMLElement, selector: string): HTMLElement {
  const item = target.querySelector<HTMLElement>(selector);
  if (!item) throw new Error(`Missing ${ selector }`);

  return item;
}

function input(target: HTMLElement, selector: string, value: string): void {
  const field = element(target, selector);
  if (!(field instanceof HTMLInputElement)) throw new TypeError(`Expected an input for ${ selector }`);

  field.value = value;
  field.dispatchEvent(new Event("input"));
  flushSync();
}

const main = {
  path  : "/tmp/demo",
  branch: "main",
};
const linked = {
  path  : "/tmp/demo-worktree",
  branch: "topic",
};
const project = {
  id           : 1,
  name         : "demo",
  mainCheckout : main.path,
  worktrees    : [main, linked],
  worktreeError: null,
  savedReviews : [],
};
const revisions = {
  branches: ["main", "topic"],
  commits : [
    {
      id     : "abc123",
      subject: "Initial commit",
    },
  ],
};

it("opens the added checkout, switches worktrees, and renders the selected unified patch", async () => {
  const previousPathBytes = Array.from("beforeÿ.txt", character => character.codePointAt(0));
  const commands = revisionCommands;
  commands
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce({
      project,
      checkout: linked.path,
    })
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce({
      start: "abc123",
      end  : "def456",
      files: [
        {
          status      : "A",
          path        : "new.txt",
          previousPath: null,
          patch       : "+new content\n",
        },
        {
          status      : "R",
          path        : "after.txt",
          previousPath: String.raw`before\xff.txt`,
          previousPathBytes,
          patch       : "-old\n+new\n",
        },
      ],
    })
    .mockResolvedValueOnce({
      start: "first",
      end  : "def456",
      files: [],
    })
    .mockResolvedValueOnce({
      start: "abc123",
      end  : "def456",
      files: [],
    });

  const target = render();
  const addCheckout = "#projects button[aria-label='Add checkout']";
  expect(target.querySelector("#projects footer h1")?.textContent).toBe("Diffest");
  await vi.waitFor(() => {
    expect(target.querySelector<HTMLButtonElement>(addCheckout)?.disabled).toBe(false);
  });

  element(target, addCheckout).click();
  flushSync();
  await vi.waitFor(() => {
    expect(commands).toHaveBeenCalledTimes(2);
    expect(target.querySelector<HTMLButtonElement>(addCheckout)?.disabled).toBe(false);
  });
  expect(invoke).toHaveBeenLastCalledWith("register_project");
  expect(target.querySelector("#projects ul")).toBeNull();
  expect(target.querySelector("[role='alert']")).toBeNull();
  element(target, addCheckout).click();
  await vi.waitFor(() => {
    expect(target.querySelector("[aria-pressed=true]")?.textContent).toContain("topic");
  });
  expect(commands).toHaveBeenCalledWith("open_worktree", { checkout: linked.path });
  expect(invoke).toHaveBeenCalledWith("read_category_settings", {
    mainCheckout: main.path,
    paths       : [],
  });

  element(target, "aside ul button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("[aria-pressed=true]")?.textContent).toContain("main");
  });
  expect(commands).toHaveBeenCalledWith("open_worktree", { checkout: main.path });

  element(target, "[aria-controls='compare-setup']").click();
  flushSync();
  input(target, "#base", "abc123");
  input(target, "#target", "refs/heads/topic");
  element(target, "#compare button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe("+new content\n");
    expect(target.querySelector(".file-title .size-change.additions")?.textContent.trim()).toBe("+1.4 KB");
    expect(target.querySelector("#changed-files .size-change")).toBeNull();
  });
  expect(commands).toHaveBeenCalledWith("compare_revisions", {
    checkout: main.path,
    base    : "abc123",
    target  : "refs/heads/topic",
    mode    : "between",
  });
  expect(target.textContent).toContain("def456");
  await vi.waitFor(() => {
    const groups = target.querySelectorAll("[aria-label='Changed files'] summary");
    expect([...groups].map(group => group.textContent)).toEqual(["Other", "Tests", "Unit"]);
  });
  const rename = element(target, "[aria-label='Changed files'] details details button");
  expect(rename.textContent).toContain(String.raw`before\xff.txt`);
  expect(rename.textContent).toContain("Renamed");
  rename.click();
  flushSync();
  expect(target.querySelector("pre")?.textContent).toBe("-old\n+new\n");
  expect(target.querySelector(".size-change")).toBeNull();
  await vi.waitFor(() => {
    expect(target.querySelector(".file-title .size-change.deletions")?.textContent.trim()).toBe("−320 B");
  });
  await vi.waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("read_file_details", {
      checkout: main.path,
      source  : expect.objectContaining({
        kind: "comparison",
        previousPathBytes,
      }),
    });
  });

  element(target, "[aria-controls='compare-setup']").click();
  flushSync();
  const mode = element(target, "#mode");
  if (!(mode instanceof HTMLSelectElement)) throw new TypeError("Expected the compare mode select");

  mode.value = "divergence";
  mode.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  element(target, "#compare button").click();
  await vi.waitFor(() => {
    expect(target.textContent).toContain("No changes between these commits.");
  });
  expect(commands).toHaveBeenLastCalledWith("compare_revisions", expect.objectContaining({ mode: "divergence" }));
  expect(target.textContent).toContain("first");

  element(target, "[aria-controls='compare-setup']").click();
  flushSync();
  mode.value = "between";
  mode.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  element(target, "#compare button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("dd")?.textContent).toBe("abc123");
  });
  expect(commands).toHaveBeenLastCalledWith("compare_revisions", expect.objectContaining({ mode: "between" }));
});

it("reopens a registered project and reports a comparison error", async () => {
  const offline = {
    id           : 2,
    name         : "offline",
    mainCheckout : "/tmp/offline",
    worktrees    : [],
    worktreeError: "Checkout is missing",
    savedReviews : [],
  };
  const detached = {
    path  : linked.path,
    branch: null,
  };
  const registered = {
    ...project,
    worktrees: [main, detached],
  };
  revisionCommands
    .mockResolvedValueOnce([registered, offline])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce({
      project : registered,
      checkout: detached.path,
    })
    .mockResolvedValueOnce(revisions)
    .mockRejectedValueOnce("Unknown revision");
  const target = render();
  await vi.waitFor(() => {
    expect(target.querySelector("aside ul button")?.textContent).toContain("main");
  });
  element(target, "aside ul button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("#base")).toBeTruthy();
  });
  expect(target.textContent).toContain("Checkout is missing");
  element(target, "#projects .project-list summary").click();
  expect(target.querySelector<HTMLDetailsElement>("#projects .project-list details")?.open).toBe(false);
  element(target, "#projects button[aria-label='Add checkout']").click();
  await vi.waitFor(() => {
    expect(target.querySelector("[aria-pressed=true]")?.textContent).toContain("Detached HEAD");
    expect(target.querySelector<HTMLDetailsElement>("#projects .project-list details")?.open).toBe(true);
  });
  expect(target.querySelectorAll("#projects .project-list summary")).toHaveLength(2);
  element(target, "[aria-controls='compare-setup']").click();
  flushSync();
  input(target, "#base", "missing");
  element(target, "#compare button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("[role=alert]")?.textContent).toBe("Unknown revision");
  });
});

it("saves the displayed comparison and resumes shared review progress from another worktree", async () => {
  const comparison = {
    baseLabel  : "refs/heads/main",
    targetLabel: "refs/heads/topic",
    mode       : "divergence",
    base       : "basetip",
    start      : "ancestor",
    end        : "topictip",
    files      : [
      {
        status      : "A",
        path        : "one.txt",
        previousPath: null,
        patch       : "+first\n",
      },
      {
        status      : "A",
        path        : "two.txt",
        previousPath: null,
        patch       : "+second\n",
      },
    ],
  };
  const summary = {
    id           : 7,
    baseLabel    : comparison.baseLabel,
    targetLabel  : comparison.targetLabel,
    start        : comparison.start,
    end          : comparison.end,
    fileCount    : 2,
    reviewedCount: 0,
  };
  const partialSummary = {
    ...summary,
    reviewedCount: 1,
  };
  const completeSummary = {
    ...summary,
    reviewedCount: 2,
  };
  const partial = {
    summary      : partialSummary,
    reviewedFiles: [0],
  };
  const commands = revisionCommands;
  commands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(comparison)
    .mockResolvedValueOnce(summary)
    .mockResolvedValueOnce(partial)
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce({
      comparison,
      ...partial,
    })
    .mockResolvedValueOnce({
      summary      : completeSummary,
      reviewedFiles: [0, 1],
    })
    .mockResolvedValueOnce(partial);

  const target = render();
  await vi.waitFor(() => {
    expect(target.querySelector("aside ul button")?.textContent).toContain("main");
  });
  element(target, "aside ul button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("#base")).toBeTruthy();
  });

  element(target, "[aria-controls='compare-setup']").click();
  flushSync();
  input(target, "#base", "refs/heads/main");
  input(target, "#target", "refs/heads/topic");
  element(target, "#compare button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe("+first\n");
  });

  element(target, "[aria-controls='compare-setup']").click();
  flushSync();
  input(target, "#target", "HEAD");
  element(target, "[aria-controls='compare-setup']").click();
  element(target, "#save-review").click();
  await vi.waitFor(() => {
    const saved = target.querySelector("[aria-label='Saved commit reviews'] button");
    expect(saved?.textContent).toContain("refs/heads/topic");
  });
  expect(commands).toHaveBeenLastCalledWith("save_commit_review", {
    checkout: main.path,
    comparison,
    layout  : null,
  });

  element(target, "#review-file").click();
  await vi.waitFor(() => {
    expect(target.querySelector("#review-progress")?.textContent).toBe("1 of 2 reviewed");
  });
  const file = "[aria-label='Changed files'] details button";
  expect(element(target, file).textContent).toContain("Reviewed");

  expect(element(target, "#projects").hidden).toBe(false);
  element(target, "aside ul li:nth-child(2) button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")).toBeNull();
  });
  element(target, ".saved-reviews > summary").click();
  element(target, "[aria-label='Saved commit reviews'] button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe("+first\n");
  });
  expect(commands).toHaveBeenLastCalledWith("open_commit_review", {
    checkout: linked.path,
    id      : 7,
  });
  expect(target.querySelector<HTMLInputElement>("#review-file")?.checked).toBe(true);
  expect(element(target, "#review-progress").textContent).toBe("1 of 2 reviewed");

  element(target, `${ file }:nth-of-type(2)`).click();
  flushSync();
  element(target, "#review-file").click();
  await vi.waitFor(() => {
    expect(target.querySelector("#review-progress")?.textContent).toBe("Reviewed");
  });
  expect(element(target, "[aria-label='Saved commit reviews'] button").textContent).toContain("Reviewed");
  expect(commands).toHaveBeenLastCalledWith("set_commit_file_reviewed", {
    checkout : linked.path,
    id       : 7,
    fileIndex: 1,
    reviewed : true,
  });

  element(target, "#review-file").click();
  await vi.waitFor(() => {
    expect(target.querySelector("#review-progress")?.textContent).toBe("1 of 2 reviewed");
  });
  expect(element(target, `${ file }:nth-of-type(2)`).textContent).toContain("Unreviewed");
});

it("opens a project's saved review directly after startup", async () => {
  const summary = {
    id           : 7,
    baseLabel    : "main",
    targetLabel  : "topic",
    start        : "ancestor",
    end          : "topictip",
    fileCount    : 0,
    reviewedCount: 0,
  };
  const comparison = {
    baseLabel  : summary.baseLabel,
    targetLabel: summary.targetLabel,
    base       : "basetip",
    mode       : "divergence",
    start      : summary.start,
    end        : summary.end,
    files      : [],
  };
  const commands = revisionCommands;
  commands
    .mockResolvedValueOnce([
      {
        ...project,
        savedReviews: [summary],
      },
    ])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce({
      comparison,
      summary,
      reviewedFiles: [],
    });

  const target = render();
  const reviewButton = "[aria-label='Saved commit reviews'] button";
  await vi.waitFor(() => {
    expect(target.querySelector(reviewButton)?.textContent).toContain("Reviewed");
  });
  element(target, ".saved-reviews > summary").click();
  element(target, reviewButton).click();
  await vi.waitFor(() => {
    expect(target.querySelector("#review-progress")?.textContent).toBe("Reviewed");
  });
  expect(target.querySelector<HTMLInputElement>("#target")?.value).toBe("topic");
  expect(target.querySelector<HTMLSelectElement>("#mode")?.value).toBe("divergence");
  expect(target.textContent).toContain("No changes between these commits.");
  expect(commands).toHaveBeenLastCalledWith("open_commit_review", {
    checkout: main.path,
    id      : 7,
  });
});

it("reviews separate staged and unstaged patches and refreshes the active worktree", async () => {
  const staged = {
    status      : "M",
    path        : "shared.txt",
    previousPath: null,
    patch       : "-committed\n+index\n",
    side        : "staged",
    fingerprint : "staged",
    reviewed    : false,
  };
  const unstaged = {
    ...staged,
    patch      : "-index\n+worktree\n",
    side       : "unstaged",
    fingerprint: "unstaged",
  };
  const conflict = {
    ...unstaged,
    status     : "U",
    fingerprint: "changed",
    patch      : "+updated\n",
  };
  const reviewedConflict = {
    ...conflict,
    reviewed: true,
  };
  const refreshedConflict = {
    ...reviewedConflict,
    fingerprint: "refreshed",
    patch      : "+refreshed\n",
    reviewed   : false,
  };
  const commands = revisionCommands;
  commands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce([staged, unstaged])
    .mockRejectedValueOnce("Unable to save the review mark.")
    .mockResolvedValueOnce([
      staged,
      {
        ...unstaged,
        reviewed: true,
      },
    ])
    .mockResolvedValueOnce({
      patches: [staged, conflict],
      error  : "Review the current patch before staging.",
    })
    .mockResolvedValueOnce([staged, reviewedConflict])
    .mockResolvedValueOnce([staged, refreshedConflict])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce([staged]);

  const target = render();
  loadWorktreePatches = true;
  const worktree = "aside ul button";
  const files = "[aria-label='Changed files'] button";
  await vi.waitFor(() => {
    expect(target.querySelector(worktree)?.textContent).toContain("main");
  });
  expect(target.querySelector("#changed-files")).toBeNull();
  element(target, worktree).click();
  await vi.waitFor(() => {
    expect(target.querySelector("#worktree-review")).toBeTruthy();
  });
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe(staged.patch);
    expect(element(target, "#changed-files").hidden).toBe(false);
  });
  expect(element(target, files).textContent).toContain("Staged");
  const unstagedEntry = element(target, `${ files }:nth-of-type(2)`);
  expect(unstagedEntry.textContent).toContain("Unstaged");
  unstagedEntry.click();
  flushSync();
  expect(target.querySelector("pre")?.textContent).toBe(unstaged.patch);
  expect(invoke).toHaveBeenCalledWith("read_file_details", {
    checkout: main.path,
    source  : {
      kind             : "worktree",
      fingerprint      : "unstaged",
      path             : "shared.txt",
      previousPath     : null,
      pathBytes        : null,
      previousPathBytes: null,
    },
  });
  expect(element(target, "#stage-file").hasAttribute("disabled")).toBe(true);
  expect(target.querySelector<HTMLInputElement>(".file-title > #review-file")?.checked).toBe(false);
  element(target, "#review-file").click();
  await vi.waitFor(() => {
    expect(target.querySelector("[role='alert']")?.textContent).toBe("Unable to save the review mark.");
  });
  expect(target.querySelector<HTMLInputElement>("#review-file")?.checked).toBe(false);
  expect(element(target, "#stage-file").hasAttribute("disabled")).toBe(true);
  element(target, "#review-file").click();
  await vi.waitFor(() => {
    expect(element(target, "#review-progress").textContent).toBe("1 of 2 reviewed");
  });
  expect(commands).toHaveBeenLastCalledWith("set_worktree_patch_reviewed", {
    checkout   : main.path,
    fingerprint: "unstaged",
    reviewed   : true,
  });

  expect(target.querySelector<HTMLInputElement>("#review-file")?.checked).toBe(true);
  expect(element(target, "#stage-file").hasAttribute("disabled")).toBe(false);
  element(target, "#stage-file").click();
  await vi.waitFor(() => {
    expect(element(target, "#review-progress").textContent).toBe("0 of 2 reviewed");
  });
  expect(commands).toHaveBeenLastCalledWith("stage_worktree_patch", {
    checkout   : main.path,
    fingerprint: "unstaged",
  });
  expect(element(target, "[role='alert']").textContent).toBe("Review the current patch before staging.");
  expect(element(target, `${ files }:nth-of-type(2)`).textContent).toContain("Conflicted");
  expect(target.querySelector("pre")?.textContent).toBe("+updated\n");
  element(target, "#review-file").click();
  await vi.waitFor(() => {
    expect(target.querySelector<HTMLInputElement>("#review-file")?.checked).toBe(true);
  });
  expect(element(target, "#stage-file").hasAttribute("disabled")).toBe(true);

  element(target, "#worktree-review").click();
  await vi.waitFor(() => {
    expect(commands).toHaveBeenLastCalledWith("open_worktree_review", { checkout: main.path });
    expect(target.querySelector("pre")?.textContent).toBe(refreshedConflict.patch);
  });
  expect(element(target, "#projects").hidden).toBe(false);
  element(target, "[aria-label='Collapse file list']").click();
  flushSync();
  expect(element(target, "#changed-files").hidden).toBe(true);
  element(target, "aside ul li:nth-child(2) button").click();
  await vi.waitFor(() => {
    expect(target.textContent).toContain("No uncommitted changes.");
    expect(target.querySelector("#changed-files")).toBeNull();
  });
  expect(commands).toHaveBeenLastCalledWith("open_worktree_review", { checkout: linked.path });
  element(target, worktree).click();
  await vi.waitFor(() => {
    expect(element(target, "#changed-files").hidden).toBe(false);
    expect(target.querySelector("pre")?.textContent).toBe(staged.patch);
  });
});

it("shows numbered changes and file counts, and restores the chosen split layout on refresh", async () => {
  const file = {
    status      : "M",
    path        : "shared.ts",
    previousPath: null,
    patch       : [
      "diff --git a/shared.ts b/shared.ts",
      "--- a/shared.ts",
      "+++ b/shared.ts",
      "@@ -3,4 +3,5 @@",
      " same",
      "-old",
      '+const value = "<img>";',
      "+extra",
      "",
      " end",
      "@@ -15 +16,0 @@",
      "-last",
      String.raw`\ No newline at end of file`,
      "",
    ].join("\n"),
    side       : "unstaged",
    fingerprint: "shared",
    reviewed   : true,
  };
  const typeChange = {
    ...file,
    status     : "T",
    path       : "link",
    fingerprint: "type-change",
    patch      : [
      "diff --git a/link b/link",
      "deleted file mode 100644",
      "--- a/link",
      "+++ /dev/null",
      "@@ -1 +0,0 @@",
      "-ordinary file",
      "diff --git a/link b/link",
      "new file mode 120000",
      "--- /dev/null",
      "+++ b/link",
      "@@ -0,0 +1 @@",
      "+symlink target",
      "",
    ].join("\n"),
  };
  const generatedLineCount = 5000;
  const largePatch = {
    ...file,
    path       : "generated.txt",
    fingerprint: "large-patch",
    patch      : `@@ -0,0 +1,${ generatedLineCount } @@\n`.concat("+generated\n".repeat(generatedLineCount)),
  };
  revisionCommands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce([file, typeChange, largePatch])
    .mockResolvedValueOnce([file, typeChange, largePatch]);
  layoutCommands.mockResolvedValueOnce(null).mockResolvedValueOnce(null)
    .mockResolvedValueOnce("split");

  loadWorktreePatches = true;
  const target = render();
  await vi.waitFor(() => expect(target.querySelector("aside ul button")).toBeTruthy());
  element(target, "aside ul button").click();
  await vi.waitFor(() => expect(target.querySelector("#worktree-review")).toBeTruthy());
  expect(element(target, "#compare-setup").hidden).toBe(true);
  await vi.waitFor(() => {
    const unified = element(target, "[aria-label='Unified patch']");
    expect(unified.querySelector(".hljs-keyword")?.textContent).toBe("const");
    expect(unified.querySelector("img")).toBeNull();
    expect(unified.textContent).toContain('const value = "<img>";');
    const firstLine = unified.querySelector("tbody tr:has(td)");
    expect([...firstLine?.querySelectorAll("td") ?? []].map(cell => cell.textContent.trim()))
      .toEqual(["3", "3", "same"]);
    expect(element(target, "[aria-label='Changed files'] button").textContent).toContain("+2");
    expect(element(target, "[aria-label='Changed files'] button").textContent).toContain("−2");
  });
  expect(target.querySelector(".app-bar [aria-label='Diff layout']")).toBeTruthy();
  expect(target.querySelector("#review-file[type=checkbox]")).toBeTruthy();
  expect(target.textContent).not.toContain("Patch headers");
  expect(target.textContent).not.toContain("File details");
  const split = "[aria-label='Split patch']";
  element(target, "[aria-label='Diff layout'] button:last-child").click();
  await vi.waitFor(() => expect(target.querySelector(split)).toBeTruthy());
  const projectsToggle = ".app-bar button[aria-controls=projects]";
  element(target, "#projects footer summary[title=Settings]").click();
  element(target, projectsToggle).click();
  flushSync();
  expect(element(target, projectsToggle).getAttribute("aria-expanded")).toBe("false");
  expect(element(target, "#projects").inert).toBe(true);
  expect(target.querySelector("#projects footer details[open]")).toBeNull();
  expect(target.querySelector(split)).toBeTruthy();
  element(target, projectsToggle).click();
  flushSync();
  expect(element(target, projectsToggle).getAttribute("aria-expanded")).toBe("true");
  expect(element(target, "#projects").inert).toBe(false);
  const rows = [...target.querySelectorAll(`${ split } tbody tr:has(td)`)]
    .map(row => [...row.querySelectorAll("td")].map(cell => cell.textContent.trim()));
  expect(rows).toEqual([
    ["3", "same", "3", "same"],
    ["4", "old", "4", 'const value = "<img>";'],
    ["", "", "5", "extra"],
    ["5", "", "6", ""],
    ["6", "end", "7", "end"],
    ["15", "last\n\\ No newline at end of file", "", ""],
  ]);
  expect(target.querySelector<HTMLInputElement>("#review-file")?.checked).toBe(true);
  expect(element(target, "[aria-label='Changed files'] [aria-pressed=true]").textContent).toContain(file.path);
  element(target, "[aria-controls='changed-files']").click();
  flushSync();
  expect(element(target, "#changed-files").hidden).toBe(true);
  expect(target.querySelector(split)).toBeTruthy();
  element(target, "[aria-controls='changed-files']").click();
  flushSync();
  expect(element(target, "#changed-files").hidden).toBe(false);
  expect(layoutCommands).toHaveBeenLastCalledWith("set_review_layout", {
    checkout: main.path,
    id      : null,
    layout  : "split",
  });

  element(target, "#worktree-review").click();
  await vi.waitFor(() => {
    expect(layoutCommands).toHaveBeenLastCalledWith("read_review_layout", {
      checkout: main.path,
      id      : null,
    });
    expect(target.querySelector<HTMLButtonElement>("#worktree-review")?.disabled).toBe(false);
  });
  expect(target.querySelector(split)).toBeTruthy();

  const typeEntry = element(target, "button[title=link]");
  expect(typeEntry.textContent).toContain("+1");
  expect(typeEntry.textContent).toContain("−1");
  typeEntry.click();
  flushSync();
  expect(element(target, split).textContent).toContain("ordinary file");
  expect(element(target, split).textContent).toContain("symlink target");

  element(target, 'button[title="generated.txt"]').click();
  flushSync();
  expect(element(target, "[aria-label='Unified patch']").textContent).toBe(largePatch.patch);
  expect(target.querySelector(split)).toBeNull();
});

it("keeps a mode-only image patch visible alongside its previews", async () => {
  const file = {
    status      : "M",
    path        : "mode.png",
    previousPath: null,
    patch       : "diff --git a/mode.png b/mode.png\nold mode 100644\nnew mode 100755\n",
    side        : "unstaged",
    fingerprint : "changed-mode",
    reviewed    : false,
  };
  loadWorktreePatches = true;
  revisionCommands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce([file]);
  const target = render();
  await vi.waitFor(() => expect(target.querySelector("aside ul button")).toBeTruthy());
  element(target, "aside ul button").click();
  await vi.waitFor(() => {
    expect(target.querySelector(".size-change")?.textContent.trim()).toBe("0 B");
    expect(target.querySelector("[aria-label='Image comparison'] img")).toBeTruthy();
  });
  expect(target.querySelector("pre")?.textContent).toContain("old mode 100644\nnew mode 100755");
});

it("applies global layout changes to an inherited review and reopens an explicit saved choice", async () => {
  const comparison = {
    baseLabel  : "main",
    targetLabel: "HEAD",
    mode       : "between",
    base       : "abc123",
    start      : "abc123",
    end        : "def456",
    files      : [
      {
        status      : "A",
        path        : "added.txt",
        previousPath: null,
        patch       : "@@ -0,0 +1 @@\n+new\n",
      },
    ],
  };
  const summary = {
    id           : 7,
    baseLabel    : comparison.baseLabel,
    targetLabel  : comparison.targetLabel,
    start        : comparison.start,
    end          : comparison.end,
    fileCount    : 1,
    reviewedCount: 0,
  };
  revisionCommands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(comparison)
    .mockResolvedValueOnce(summary)
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce({
      comparison,
      summary,
      reviewedFiles: [],
    });
  layoutCommands.mockResolvedValueOnce(null).mockResolvedValueOnce("unified");

  const target = render();
  await vi.waitFor(() => expect(target.querySelector("aside ul button")).toBeTruthy());
  element(target, "aside ul button").click();
  await vi.waitFor(() => expect(target.querySelector("#base")).toBeTruthy());
  element(target, "[aria-controls='compare-setup']").click();
  flushSync();
  input(target, "#base", "main");
  element(target, "#compare button").click();
  const unified = "[aria-label='Unified patch']";
  const split = "[aria-label='Split patch']";
  await vi.waitFor(() => expect(target.querySelector(unified)).toBeTruthy());

  element(target, "#save-review").click();
  const saved = "[aria-label='Saved commit reviews'] button";
  await vi.waitFor(() => expect(target.querySelector(saved)).toBeTruthy());
  element(target, "#projects footer summary[title=Settings]").click();
  const select = target.querySelector<HTMLSelectElement>("#default-diff-layout");
  if (!select) throw new Error("Missing default diff layout");
  expect(select.value).toBe("unified");
  select.value = "split";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await vi.waitFor(() => expect(target.querySelector(split)?.textContent).toContain("new"));

  const unifiedButton = "[aria-label='Diff layout'] button:first-child";
  element(target, unifiedButton).click();
  await vi.waitFor(() => expect(target.querySelector(unified)).toBeTruthy());
  expect(layoutCommands).toHaveBeenLastCalledWith("set_review_layout", {
    checkout: main.path,
    id      : 7,
    layout  : "unified",
  });

  element(target, "aside ul button").click();
  await vi.waitFor(() => expect(target.querySelector(unified)).toBeNull());
  element(target, ".saved-reviews > summary").click();
  element(target, saved).click();
  await vi.waitFor(() => expect(target.querySelector(unified)).toBeTruthy());
  expect(select.value).toBe("split");
});
