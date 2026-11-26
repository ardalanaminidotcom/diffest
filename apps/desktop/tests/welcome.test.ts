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
beforeEach(() => {
  revisionCommands.mockReset();
  vi.mocked(invoke).mockImplementation(async (command, parameters) => {
    if (command === "read_file_details") {
      return {
        before: null,
        after : null,
      };
    }

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
  const commands = revisionCommands;
  commands
    .mockResolvedValueOnce([])
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
          previousPath: "before.txt",
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
  expect(target.querySelector("main h1")?.textContent).toBe("Diffest");
  await vi.waitFor(() => {
    expect(target.querySelector<HTMLButtonElement>("form button")?.disabled).toBe(false);
  });

  input(target, "#checkout", linked.path);
  element(target, "form button").click();
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

  input(target, "#base", "abc123");
  input(target, "#target", "refs/heads/topic");
  element(target, "#compare button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe("+new content\n");
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
  expect(rename.textContent).toContain("before.txt");
  expect(rename.textContent).toContain("Renamed");
  rename.click();
  flushSync();
  expect(target.querySelector("pre")?.textContent).toBe("-old\n+new\n");

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
  input(target, "#checkout", detached.path);
  element(target, "form button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("[aria-pressed=true]")?.textContent).toContain("Detached HEAD");
  });
  expect(target.querySelectorAll("aside h2")).toHaveLength(2);
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
  const commands = revisionCommands;
  commands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(comparison)
    .mockResolvedValueOnce(7)
    .mockResolvedValueOnce([0])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce({
      comparison,
      reviewedFiles: [0],
    })
    .mockResolvedValueOnce([0, 1])
    .mockResolvedValueOnce([0]);

  const target = render();
  await vi.waitFor(() => {
    expect(target.querySelector("aside ul button")?.textContent).toContain("main");
  });
  element(target, "aside ul button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("#base")).toBeTruthy();
  });

  input(target, "#base", "refs/heads/main");
  input(target, "#target", "refs/heads/topic");
  element(target, "#compare button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe("+first\n");
  });

  input(target, "#target", "HEAD");
  element(target, "#save-review").click();
  await vi.waitFor(() => {
    const saved = target.querySelector("[aria-label='Saved commit reviews'] button");
    expect(saved?.textContent).toContain("refs/heads/topic");
  });
  expect(commands).toHaveBeenLastCalledWith("save_commit_review", {
    checkout: main.path,
    comparison,
  });

  element(target, "#review-file").click();
  await vi.waitFor(() => {
    expect(target.querySelector("#review-progress")?.textContent).toBe("1 of 2 reviewed");
  });
  const file = "[aria-label='Changed files'] details button";
  expect(element(target, file).textContent).toContain("Reviewed");

  element(target, "aside ul li:nth-child(2) button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")).toBeNull();
  });
  element(target, "[aria-label='Saved commit reviews'] button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe("+first\n");
  });
  expect(commands).toHaveBeenLastCalledWith("open_commit_review", {
    checkout: linked.path,
    id      : 7,
  });
  expect(element(target, "#review-file").textContent).toBe("Clear review mark");
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
      reviewedFiles: [],
    });

  const target = render();
  const reviewButton = "[aria-label='Saved commit reviews'] button";
  await vi.waitFor(() => {
    expect(target.querySelector(reviewButton)?.textContent).toContain("Reviewed");
  });
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
  const commands = revisionCommands;
  commands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce([staged, unstaged])
    .mockResolvedValueOnce([
      staged,
      {
        ...unstaged,
        reviewed: true,
      },
    ])
    .mockResolvedValueOnce([
      staged,
      {
        ...unstaged,
        status     : "U",
        fingerprint: "changed",
        patch      : "+updated\n",
      },
    ])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce([]);

  const target = render();
  const worktree = "aside ul button";
  const files = "[aria-label='Changed files'] button";
  await vi.waitFor(() => {
    expect(target.querySelector(worktree)?.textContent).toContain("main");
  });
  element(target, worktree).click();
  await vi.waitFor(() => {
    expect(target.querySelector("#worktree-review")).toBeTruthy();
  });
  element(target, "#worktree-review").click();
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe(staged.patch);
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
      kind       : "worktree",
      fingerprint: "unstaged",
    },
  });
  element(target, "#review-file").click();
  await vi.waitFor(() => {
    expect(element(target, "#review-progress").textContent).toBe("1 of 2 reviewed");
  });
  expect(commands).toHaveBeenLastCalledWith("set_worktree_patch_reviewed", {
    checkout   : main.path,
    fingerprint: "unstaged",
    reviewed   : true,
  });

  element(target, "#worktree-review").click();
  await vi.waitFor(() => {
    expect(element(target, "#review-progress").textContent).toBe("0 of 2 reviewed");
  });
  expect(element(target, `${ files }:nth-of-type(2)`).textContent).toContain("Conflicted");
  expect(target.querySelector("pre")?.textContent).toBe("+updated\n");

  element(target, "aside ul li:nth-child(2) button").click();
  await vi.waitFor(() => {
    expect(target.textContent).toContain("No uncommitted changes.");
  });
  expect(commands).toHaveBeenLastCalledWith("open_worktree_review", { checkout: linked.path });
});
