import { invoke } from "@tauri-apps/api/core";
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";
import DiffView from "../src/lib/diff-view.svelte";
import { ssr } from "../src/routes/+layout.js";
import Page from "../src/routes/+page.svelte";

it("renders on the client for Tauri's static frontend", () => {
  expect(ssr).toBe(false);
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const revisionCommands = vi.fn();
const fixtureCategories = new Map<unknown, string>([["after.txt", "Tests"], ["pnpm-lock.yaml", "Generated"]]);
const layoutCommands = vi.fn();
const NativeIntersectionObserver = globalThis.IntersectionObserver;
const fileVisibility = new WeakMap<Element, (nearViewport: boolean) => void>();
let defaultDiffLayout = "unified";
let pendingCategorization = Promise.resolve(null);
const comparisonStatus = vi.fn();
let loadWorktreePatches = false;
beforeEach(() => {
  vi.stubGlobal("IntersectionObserver", class extends NativeIntersectionObserver {

    public constructor(private readonly onIntersection: IntersectionObserverCallback) {
      super(onIntersection);
    }

    public override observe(target: Element): void {
      const bounds = target.getBoundingClientRect();
      const updateVisibility = (isIntersecting: boolean): void => {
        const entry: IntersectionObserverEntry = {
          target,
          isIntersecting,
          intersectionRatio : isIntersecting ? 1 : 0,
          boundingClientRect: bounds,
          intersectionRect  : bounds,
          rootBounds        : bounds,
          time              : 0,
        };
        this.onIntersection([entry], this);
      };
      fileVisibility.set(target, updateVisibility);
      queueMicrotask(() => updateVisibility(true));
    }

  });
  onTestFinished(() => {
    vi.unstubAllGlobals();
  });

  vi.mocked(invoke).mockClear();
  revisionCommands.mockReset();
  layoutCommands.mockReset().mockResolvedValue(null);
  defaultDiffLayout = "unified";
  pendingCategorization = Promise.resolve(null);
  comparisonStatus.mockReset().mockResolvedValue("current");
  const retention = new Map<unknown, number>([[null, 7]]);
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
    if (command === "leave_comparison_review") return null;
    if (command === "comparison_update") return comparisonStatus(parameters);
    if (command === "read_review_retention" || command === "save_review_retention") {
      const mainCheckout = parameters && "mainCheckout" in parameters ? parameters.mainCheckout : null;
      const days = parameters && "days" in parameters ? parameters.days : null;
      if (command === "save_review_retention") {
        if (typeof days === "number") retention.set(mainCheckout, days);
        else retention.delete(mainCheckout);
      }
      const overrideDays = retention.get(mainCheckout) ?? null;
      return {
        days: overrideDays ?? retention.get(null),
        overrideDays,
      };
    }
    if (command !== "read_category_settings") return revisionCommands(command, parameters);

    const filePaths = parameters && "paths" in parameters ? parameters.paths : null;
    const paths: unknown[] = Array.isArray(filePaths) ? filePaths : [];
    if (paths.includes("pnpm-lock.yaml")) await pendingCategorization;
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
        category   : fixtureCategories.get(path) ?? "Source",
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

function chooseSource(target: HTMLElement, revision: string): void {
  element(target, ".revision-picker .source > summary").click();
  flushSync();
  input(target, "#base", revision);
  element(target, "#compare button").click();
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
};
const revisions = {
  defaultBranch : null,
  currentBranch : null,
  remoteBranches: [],
  branches      : ["main", "topic"],
  commits       : [
    {
      id     : "abc123",
      subject: "Initial commit",
    },
  ],
};

it("starts generated diffs collapsed and keeps their review checkbox independent", async () => {
  const patch = {
    path        : "pnpm-lock.yaml",
    status      : "M",
    previousPath: null,
    patch       : "@@ -1 +1 @@\n-old\n+new\n",
    side        : "unstaged",
    fingerprint : "lockfile",
    reviewed    : false,
  };
  revisionCommands.mockResolvedValueOnce([project]).mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce([patch]);
  loadWorktreePatches = true;
  const categorization = Promise.withResolvers<null>();
  pendingCategorization = categorization.promise;
  const target = render();
  const worktree = await vi.waitFor(() => element(target, "aside ul button"));
  worktree.click();

  const file = await vi.waitFor(() => element(target, "#file-review-0"));
  const diff = "[aria-label='Unified patch']";
  expect(file.querySelector(diff)).toBeNull();
  categorization.resolve(null);
  await vi.waitFor(() => expect(file.textContent).toContain("Generated"));
  const toggle = element(file, "h2 button");
  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  expect(file.querySelector(diff)).toBeNull();
  toggle.click();
  await vi.waitFor(() => expect(file.querySelector(diff)?.textContent).toContain("new"));
  expect(file.querySelector<HTMLInputElement>("input[type=checkbox]")?.checked).toBe(false);
});

it("reviews a continuous file stream in sidebar order and jumps to a collapsed file", async () => {
  const patches = ["one.txt", "after.txt", "two.txt"].map(path => ({
    path,
    status      : "M",
    previousPath: null,
    patch       : `@@ -1 +1 @@\n-old content\n+${ path } content\n`,
    side        : "unstaged",
    fingerprint : path,
    reviewed    : false,
  }));
  const reviewed = patches.map(patch => ({
    ...patch,
    reviewed: patch.path === "after.txt",
  }));
  revisionCommands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(patches)
    .mockResolvedValueOnce(reviewed)
    .mockResolvedValueOnce(patches);
  loadWorktreePatches = true;
  const target = render();
  const worktree = "aside ul button";
  await vi.waitFor(() => expect(target.querySelector(worktree)).toBeTruthy());
  element(target, worktree).click();

  const headings = "[aria-label='File diffs'] h2";
  const sidebarNames = "[aria-label='Changed files'] button strong";
  function displayedNames(selector: string): string[] {
    return Array.from(target.querySelectorAll(selector), name => name.textContent.trim());
  }
  await vi.waitFor(() => {
    expect(displayedNames(headings)).toEqual(["one.txt", "two.txt", "after.txt"]);
    expect(displayedNames(headings)).toEqual(displayedNames(sidebarNames));
    const source = element(target, "nav[aria-label='Changed files'] > details > summary");
    expect(source.getAttribute("aria-label")).toBe("Source, lines added 2, lines removed 2");
    expect(source.textContent).toContain("+2");
    expect(source.textContent).toContain("−2");
  });
  const totalsSelector = "#changed-files header [aria-label='Total lines added 3, lines removed 3']";
  const totals = element(target, totalsSelector);
  expect(totals.textContent).toContain("+3");
  expect(totals.textContent).toContain("−3");
  expect(target.querySelectorAll("[aria-label='Unified patch']")).toHaveLength(3);
  expect(target.textContent).not.toContain("@@");

  const file = element(target, "#file-review-1");
  const disclosure = element(file, "h2 button");
  const viewed = element(file, "input[type=checkbox]");
  viewed.click();
  await vi.waitFor(() => expect(disclosure.getAttribute("aria-expanded")).toBe("false"));
  const testCategory = element(target, "nav[aria-label='Changed files'] > details:nth-child(2) > summary");
  testCategory.click();
  expect(testCategory.parentElement?.hasAttribute("open")).toBe(false);
  expect(testCategory.textContent).toContain("+1");
  expect(testCategory.textContent).toContain("−1");
  expect(element(target, totalsSelector)).toBe(totals);
  testCategory.click();
  expect(file.querySelector("[aria-label='Unified patch']")).toBeNull();
  expect(target.querySelectorAll("[aria-label='Unified patch']")).toHaveLength(2);

  const scroll = vi.spyOn(file, "scrollIntoView");
  element(target, "[aria-label='Changed files'] button[title='after.txt']").click();
  expect(document.activeElement).toBe(disclosure);
  expect(scroll).toHaveBeenCalledWith({ block: "start" });
  expect(disclosure.getAttribute("aria-expanded")).toBe("false");

  disclosure.click();
  flushSync();
  expect(disclosure.getAttribute("aria-expanded")).toBe("true");
  expect(file.querySelector<HTMLInputElement>("input[type=checkbox]")?.checked).toBe(true);
  disclosure.click();
  flushSync();
  expect(disclosure.getAttribute("aria-expanded")).toBe("false");
  viewed.click();
  await vi.waitFor(() => expect(disclosure.getAttribute("aria-expanded")).toBe("true"));
  expect(target.querySelectorAll("[aria-label='Unified patch']")).toHaveLength(3);
});

it("keeps text rows and scroll position stable when a patch leaves and reenters the viewport", async () => {
  const patches = ["first.txt", "second.txt"].map(path => ({
    path,
    status      : "A",
    previousPath: null,
    patch       : "@@ -0,0 +1 @@\n+new line\n",
    side        : "unstaged",
    fingerprint : path,
    reviewed    : false,
  }));
  revisionCommands.mockResolvedValueOnce([project]).mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(patches);
  loadWorktreePatches = true;
  const target = render();
  await vi.waitFor(() => expect(target.querySelector("aside ul button")).toBeTruthy());
  element(target, "aside ul button").click();
  await vi.waitFor(() => expect(target.querySelector("#file-review-0 table")).toBeTruthy());

  const stream = element(target, "[aria-label='File diffs']");
  const firstFile = element(stream, "#file-review-0");
  const updateVisibility = fileVisibility.get(firstFile);
  if (!updateVisibility) throw new Error("Expected the file to be observed");

  const row = element(firstFile, "tr");
  const initialScroll = 900;
  stream.scrollTop = initialScroll;
  updateVisibility(false);
  flushSync();
  expect(firstFile.querySelector("tr")).toBe(row);
  expect(firstFile.querySelector(".patch-placeholder")).toBeNull();

  updateVisibility(true);
  flushSync();
  expect(firstFile.querySelector("tr")).toBe(row);
  expect(stream.scrollTop).toBe(initialScroll);
});

it("preserves measured space for offscreen row blocks in a large diff", async () => {
  const target = document.createElement("div");
  target.classList.add("file-stream");
  document.body.append(target);
  const lineCount = 260;
  const rowHeight = 18;
  const width = 500;
  const tableBounds = vi.spyOn(globalThis.HTMLTableElement.prototype, "getBoundingClientRect")
    .mockImplementation(function tableBounds(this: HTMLTableElement) {
      return new globalThis.DOMRect(0, 0, width, this.rows.length * rowHeight);
    });
  const view = mount(DiffView, {
    target,
    props: {
      patch       : `@@ -1,0 +1,${ lineCount } @@\n${ "+new line\n".repeat(lineCount) }`,
      path        : "large.txt",
      previousPath: null,
      layout      : "unified",
      highlight   : false,
      windowRows  : true,
    },
  });
  onTestFinished(async () => {
    await unmount(view);
    target.remove();
    tableBounds.mockRestore();
  });
  await vi.waitFor(() => expect(target.querySelectorAll("table").length).toBeGreaterThan(1));
  const table = element(target, "table");
  const slot = table.parentElement;
  const updateVisibility = slot && fileVisibility.get(slot);
  if (!slot || !updateVisibility) throw new Error("Expected the row block to be observed");

  const measuredSize = slot.style.getPropertyValue("--diff-block-size");
  expect(Number.parseFloat(measuredSize)).toBeGreaterThan(0);
  updateVisibility(false);
  expect(slot.querySelector("table")).toBeNull();
  expect(slot.style.getPropertyValue("--diff-block-size")).toBe(measuredSize);

  updateVisibility(true);
  expect(slot.querySelector("table")?.textContent).toContain("new line");
  expect(slot.style.getPropertyValue("--diff-block-size")).toBe(measuredSize);
});

it("opens the added checkout, switches worktrees, and shows unified patches with size changes", async () => {
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

  chooseSource(target, "abc123");
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe("+new content\n");
    expect(target.querySelector(".file-title .size-change")?.textContent.trim()).toBe("+1.4 KB");
    expect(target.querySelector("#changed-files .size-change")).toBeNull();
  });
  expect(commands).toHaveBeenCalledWith("compare_revisions", {
    checkout: main.path,
    base    : "abc123",
    target  : "HEAD",
    mode    : "between",
  });
  await vi.waitFor(() => {
    const groups = target.querySelectorAll("[aria-label='Changed files'] summary");
    expect([...groups].map(group => group.textContent.trim())).toEqual(["Source", "Tests", "Unit"]);
  });
  const rename = element(target, "[aria-label='Changed files'] details details button");
  expect(rename.textContent).toContain(String.raw`before\xff.txt`);
  expect(rename.textContent).toContain("Renamed");
  rename.click();
  flushSync();
  const renamedFile = element(target, "#file-review-1");
  expect(renamedFile.querySelector("pre")?.textContent).toBe("-old\n+new\n");
  await vi.waitFor(() => {
    expect(renamedFile.querySelector(".file-title .size-change")?.textContent.trim()).toBe("−320 B");
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

  chooseSource(target, "main");
  await vi.waitFor(() => {
    expect(target.textContent).toContain("No differences.");
  });
  expect(commands).toHaveBeenLastCalledWith("compare_revisions", expect.objectContaining({ mode: "between" }));

  element(target, ".revision-picker .source > summary").click();
  element(target, ".revision-picker .mode").click();
  await vi.waitFor(() => {
    expect(commands).toHaveBeenLastCalledWith("compare_revisions", expect.objectContaining({ mode: "divergence" }));
  });
});

it("reopens a registered project and reports a comparison error", async () => {
  const offline = {
    id           : 2,
    name         : "offline",
    mainCheckout : "/tmp/offline",
    worktrees    : [],
    worktreeError: "Checkout is missing",
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
  chooseSource(target, "missing");
  await vi.waitFor(() => {
    expect(target.querySelector("[role=alert]")?.textContent).toBe("Unknown revision");
  });
});

it("opens the default branch comparison and saves its first review mark", async () => {
  const comparison = {
    baseLabel  : "refs/heads/main",
    targetLabel: "HEAD",
    mode       : "divergence",
    base       : "main-tip",
    start      : "ancestor",
    end        : "topic-tip",
    files      : [
      {
        status      : "A",
        path        : "new.txt",
        previousPath: null,
        patch       : "+new\n",
      },
    ],
    progress: null,
  };
  const checkoutRevisions = {
    ...revisions,
    defaultBranch: "refs/heads/main",
    currentBranch: "refs/heads/topic",
  };
  const progress = {
    summary: {
      id           : 7,
      baseLabel    : comparison.baseLabel,
      targetLabel  : comparison.targetLabel,
      start        : comparison.start,
      end          : comparison.end,
      fileCount    : 1,
      reviewedCount: 1,
    },
    reviewedFiles: [0],
  };
  const resumedComparison = {
    ...comparison,
    progress,
  };
  revisionCommands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(checkoutRevisions)
    .mockResolvedValueOnce(comparison)
    .mockResolvedValueOnce(progress)
    .mockResolvedValueOnce(checkoutRevisions)
    .mockResolvedValueOnce(resumedComparison);
  const target = render();
  const topic = "#projects li:nth-child(2) button";
  await vi.waitFor(() => expect(target.querySelector(topic)).toBeTruthy());
  element(target, topic).click();
  const mark = ".file-stream input[type=checkbox]";
  await vi.waitFor(() => expect(target.querySelector(mark)).toBeTruthy());
  expect(revisionCommands).toHaveBeenLastCalledWith("compare_revisions", {
    checkout: linked.path,
    base    : "refs/heads/main",
    target  : "HEAD",
    mode    : "divergence",
  });
  element(target, mark).click();
  await vi.waitFor(() => expect(target.querySelector("#review-progress")?.textContent).toBe("Reviewed"));
  expect(revisionCommands).toHaveBeenLastCalledWith("start_commit_review", expect.objectContaining({ fileIndex: 0 }));
  element(target, topic).click();
  await vi.waitFor(() => {
    expect(target.querySelector<HTMLInputElement>(mark)?.checked).toBe(true);
    expect(revisionCommands).toHaveBeenLastCalledWith("compare_revisions", expect.objectContaining({ target: "HEAD" }));
  });
});

it("offers distinct remote branch tips and switches from a clean checkout to a working-tree comparison", async () => {
  const comparison = {
    baseLabel  : "refs/remotes/origin/main",
    targetLabel: "Working tree",
    mode       : "between",
    base       : "upstream-tip",
    start      : "upstream-tip",
    end        : "working-tree",
    files      : [],
    progress   : null,
  };
  revisionCommands.mockResolvedValueOnce([project]).mockResolvedValueOnce({
    ...revisions,
    defaultBranch : "refs/heads/main",
    currentBranch : "refs/heads/main",
    remoteBranches: ["origin/main"],
  })
    .mockResolvedValueOnce(comparison);
  const target = render();
  await vi.waitFor(() => expect(target.querySelector("#projects li button")).toBeTruthy());
  element(target, "#projects li button").click();
  await vi.waitFor(() => expect(target.textContent).toContain("Choose a source branch or commit"));
  element(target, ".revision-picker .target > summary").click();
  element(target, ".revision-picker .target button[title='Working tree']").click();
  await vi.waitFor(() => {
    expect(element(target, ".revision-picker .source > summary").textContent).toContain("HEAD");
    expect(element(target, ".revision-picker .target > summary").textContent).toContain("Working tree");
  });
  chooseSource(target, "origin/main");
  await vi.waitFor(() => {
    expect(revisionCommands).toHaveBeenLastCalledWith("compare_revisions", {
      checkout: main.path,
      base    : "refs/remotes/origin/main",
      target  : "Working tree",
      mode    : "between",
    });
  });
});

it("fetches remote branches in the target picker without rerunning the current comparison", async () => {
  const comparison = {
    baseLabel  : "refs/heads/main",
    targetLabel: "HEAD",
    mode       : "divergence",
    base       : "base",
    start      : "base",
    end        : "head",
    files      : [],
    progress   : null,
  };
  const references = {
    ...revisions,
    defaultBranch: "refs/heads/main",
    currentBranch: "refs/heads/topic",
  };
  const fetched = {
    ...references,
    remoteBranches: ["origin/release"],
  };
  const releaseComparison = {
    ...comparison,
    targetLabel: "refs/remotes/origin/release",
  };
  revisionCommands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(references)
    .mockResolvedValueOnce(comparison)
    .mockRejectedValueOnce("One remote could not be reached")
    .mockResolvedValueOnce(fetched)
    .mockResolvedValueOnce(fetched)
    .mockResolvedValueOnce(releaseComparison);
  const target = render();
  await vi.waitFor(() => expect(target.querySelector("#projects li button")).toBeTruthy());
  element(target, "#projects li button").click();
  const picker = ".revision-picker .target";
  const trigger = `${ picker } > summary`;
  await vi.waitFor(() => expect(element(target, trigger).textContent).toContain("HEAD"));
  element(target, trigger).click();
  flushSync();
  expect(element(target, picker).textContent).toContain("Recent commits");
  const fetch = [...target.querySelectorAll<HTMLButtonElement>(`${ picker } button`)]
    .find(button => button.textContent === "Fetch branches");
  if (!fetch) throw new Error("Missing Fetch branches button");
  fetch.click();
  const release = `${ picker } button[title='origin/release']`;
  await vi.waitFor(() => expect(target.querySelector(release)).toBeTruthy());
  expect(revisionCommands).toHaveBeenLastCalledWith("open_worktree", { checkout: main.path });
  expect(element(target, picker).textContent).toContain("One remote could not be reached");
  fetch.click();
  await vi.waitFor(() => expect(element(target, picker).textContent).toContain("Branches updated."));
  expect(revisionCommands).toHaveBeenLastCalledWith("fetch_revisions", { checkout: main.path });
  expect(element(target, trigger).textContent).toContain("HEAD");
  input(target, `${ picker } input`, "release");
  expect(element(target, picker).textContent).not.toContain("Recent commits");
  element(target, release).click();
  await vi.waitFor(() => expect(revisionCommands).toHaveBeenLastCalledWith("compare_revisions", {
    checkout: main.path,
    base    : "refs/heads/main",
    target  : "refs/remotes/origin/release",
    mode    : "between",
  }));
});

it("saves the displayed comparison and resumes shared review progress from another worktree", async () => {
  const comparison = {
    baseLabel  : "refs/heads/main",
    targetLabel: "HEAD",
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
  const selection = {
    base  : "refs/heads/main",
    target: "refs/heads/main",
    mode  : "between",
  };
  const session = {
    ...comparison,
    selection,
    update: "current",
  };
  const commands = revisionCommands;
  commands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(session)
    .mockResolvedValueOnce(partial)
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce({
      ...session,
      progress: partial,
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

  chooseSource(target, "refs/heads/main");
  await vi.waitFor(() => {
    expect(target.querySelector("pre")?.textContent).toBe("+first\n");
  });

  const secondFileToggle = element(target, "#file-review-1 h2 button");
  secondFileToggle.click();
  flushSync();
  expect(secondFileToggle.getAttribute("aria-expanded")).toBe("false");
  expect(target.querySelector("[aria-label='Saved commit reviews']")).toBeNull();

  const firstMark = "#file-review-0 input[type=checkbox]";
  const secondMark = "#file-review-1 input[type=checkbox]";
  element(target, firstMark).click();
  await vi.waitFor(() => {
    expect(target.querySelector("#review-progress")?.textContent).toBe("1 of 2 reviewed");
  });
  expect(commands).toHaveBeenLastCalledWith("start_commit_review", {
    checkout : main.path,
    comparison,
    selection,
    layout   : null,
    fileIndex: 0,
  });
  expect(secondFileToggle.getAttribute("aria-expanded")).toBe("false");
  const file = "[aria-label='Changed files'] details button";
  expect(element(target, file).textContent).toContain("Reviewed");

  expect(element(target, "#projects").hidden).toBe(false);
  element(target, "aside ul li:nth-child(2) button").click();
  await vi.waitFor(() => {
    expect(target.querySelector("[aria-label='File diffs']")).toBeNull();
  });
  chooseSource(target, "refs/heads/main");
  await vi.waitFor(() => {
    expect(target.querySelector("#file-review-0 pre")).toBeNull();
    expect(target.querySelector("#file-review-1 pre")?.textContent).toBe("+second\n");
  });
  expect(commands).toHaveBeenLastCalledWith("compare_revisions", {
    checkout: linked.path,
    base    : "refs/heads/main",
    target  : "HEAD",
    mode    : "between",
  });
  expect(target.querySelector<HTMLInputElement>(firstMark)?.checked).toBe(true);
  expect(element(target, "#review-progress").textContent).toBe("1 of 2 reviewed");

  element(target, `${ file }:nth-of-type(2)`).click();
  flushSync();
  element(target, secondMark).click();
  await vi.waitFor(() => {
    expect(target.querySelector("#review-progress")?.textContent).toBe("Reviewed");
  });
  expect(target.textContent).not.toContain("Saved reviews");
  expect(commands).toHaveBeenLastCalledWith("set_commit_file_reviewed", {
    checkout : linked.path,
    id       : 7,
    fileIndex: 1,
    reviewed : true,
  });

  element(target, secondMark).click();
  await vi.waitFor(() => {
    expect(target.querySelector("#review-progress")?.textContent).toBe("1 of 2 reviewed");
  });
  expect(element(target, `${ file }:nth-of-type(2)`).textContent).toContain("Unreviewed");
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
  const unstagedFile = "#file-review-1";
  const mark = `${ unstagedFile } input[type=checkbox]`;
  const stage = `${ unstagedFile } header > button`;
  const unstagedDiff = `${ unstagedFile } pre`;
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
    expect(element(target, "#changed-files").inert).toBe(false);
  });
  expect(element(target, files).textContent).toContain("Staged");
  const unstagedEntry = element(target, `${ files }:nth-of-type(2)`);
  expect(unstagedEntry.textContent).toContain("Unstaged");
  unstagedEntry.click();
  flushSync();
  expect(target.querySelector(unstagedDiff)?.textContent).toBe(unstaged.patch);
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
  expect(element(target, stage).hasAttribute("disabled")).toBe(true);
  expect(target.querySelector<HTMLInputElement>(mark)?.checked).toBe(false);
  element(target, mark).click();
  await vi.waitFor(() => {
    expect(target.querySelector("[role='alert']")?.textContent).toBe("Unable to save the review mark.");
  });
  expect(target.querySelector<HTMLInputElement>(mark)?.checked).toBe(false);
  expect(element(target, stage).hasAttribute("disabled")).toBe(true);
  element(target, mark).click();
  await vi.waitFor(() => {
    expect(element(target, "#review-progress").textContent).toBe("1 of 2 reviewed");
  });
  expect(commands).toHaveBeenLastCalledWith("set_worktree_patch_reviewed", {
    checkout   : main.path,
    fingerprint: "unstaged",
    reviewed   : true,
  });

  expect(target.querySelector<HTMLInputElement>(mark)?.checked).toBe(true);
  expect(target.querySelector(unstagedDiff)).toBeNull();
  expect(element(target, stage).hasAttribute("disabled")).toBe(false);
  element(target, stage).click();
  await vi.waitFor(() => {
    expect(element(target, "#review-progress").textContent).toBe("0 of 2 reviewed");
  });
  expect(commands).toHaveBeenLastCalledWith("stage_worktree_patch", {
    checkout   : main.path,
    fingerprint: "unstaged",
  });
  expect(element(target, "[role='alert']").textContent).toBe("Review the current patch before staging.");
  expect(element(target, `${ files }:nth-of-type(2)`).textContent).toContain("Conflicted");
  expect(target.querySelector(unstagedDiff)?.textContent).toBe("+updated\n");
  element(target, mark).click();
  await vi.waitFor(() => {
    expect(target.querySelector<HTMLInputElement>(mark)?.checked).toBe(true);
  });
  expect(element(target, stage).hasAttribute("disabled")).toBe(true);

  element(target, "#worktree-review").click();
  await vi.waitFor(() => {
    expect(commands).toHaveBeenLastCalledWith("open_worktree_review", { checkout: main.path });
    expect(target.querySelector(unstagedDiff)?.textContent).toBe(refreshedConflict.patch);
  });
  expect(element(target, "#projects").hidden).toBe(false);
  element(target, ".app-bar [aria-controls=changed-files]").click();
  flushSync();
  expect(element(target, "#changed-files").inert).toBe(true);
  element(target, "aside ul li:nth-child(2) button").click();
  await vi.waitFor(() => {
    expect(target.textContent).toContain("No uncommitted changes.");
    expect(target.querySelector("#changed-files")).toBeNull();
  });
  expect(commands).toHaveBeenLastCalledWith("open_worktree_review", { checkout: linked.path });
  element(target, worktree).click();
  await vi.waitFor(() => {
    expect(element(target, "#changed-files").inert).toBe(false);
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
      "old mode 100644",
      "new mode 100755",
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
  expect(element(target, ".revision-picker .target > summary").getAttribute("aria-label")).toBe("Target Working tree");
  const sharedFile = "#file-review-0";
  await vi.waitFor(() => expect(element(target, `${ sharedFile } h2 button`).getAttribute("aria-expanded")).toBe("false"));
  element(target, `${ sharedFile } h2 button`).click();
  await vi.waitFor(() => {
    const unified = element(target, `${ sharedFile } [aria-label='Unified patch']`);
    expect(element(target, `${ sharedFile } [aria-label='File mode change']`).textContent)
      .toBe("old mode 100644\nnew mode 100755");
    expect(unified.textContent).not.toContain("@@");
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
  expect(target.querySelector(`${ sharedFile } input[type=checkbox]`)).toBeTruthy();
  expect(target.textContent).not.toContain("Patch headers");
  expect(target.textContent).not.toContain("File details");
  const split = `${ sharedFile } [aria-label='Split patch']`;
  element(target, "[aria-label='Diff layout'] button:last-child").click();
  await vi.waitFor(() => expect(target.querySelector(split)).toBeTruthy());
  expect(element(target, `${ sharedFile } [aria-label='File mode change']`).textContent)
    .toBe("old mode 100644\nnew mode 100755");
  const projectsToggle = ".app-bar button[aria-controls=projects]";
  element(target, projectsToggle).click();
  flushSync();
  expect(element(target, projectsToggle).getAttribute("aria-expanded")).toBe("false");
  expect(element(target, "#projects").inert).toBe(true);
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
  expect(target.querySelector<HTMLInputElement>(`${ sharedFile } input[type=checkbox]`)?.checked).toBe(true);
  expect(element(target, "[aria-label='Changed files'] [aria-pressed=true]").textContent).toContain(file.path);
  const filesToggle = ".app-bar [aria-controls=changed-files]";
  const filesButton = element(target, filesToggle);
  filesButton.click();
  flushSync();
  expect(element(target, "#changed-files").inert).toBe(true);
  expect(filesButton.getAttribute("aria-expanded")).toBe("false");
  expect(target.querySelector(".file-header [aria-controls=changed-files]")).toBeNull();
  expect(target.querySelector(split)).toBeTruthy();
  expect(element(target, filesToggle)).toBe(filesButton);
  filesButton.click();
  flushSync();
  expect(element(target, "#changed-files").inert).toBe(false);
  expect(filesButton.getAttribute("aria-expanded")).toBe("true");
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

  const typeEntry = element(target, "#changed-files button[title=link]");
  expect(typeEntry.textContent).toContain("+1");
  expect(typeEntry.textContent).toContain("−1");
  typeEntry.click();
  element(target, "#file-review-1 h2 button").click();
  flushSync();
  const typePatch = element(target, "#file-review-1 [aria-label='Split patch']");
  expect(typePatch.textContent).toContain("ordinary file");
  expect(typePatch.textContent).toContain("symlink target");

  element(target, '#changed-files button[title="generated.txt"]').click();
  element(target, "#file-review-2 h2 button").click();
  flushSync();
  expect(element(target, "#file-review-2 [aria-label='Unified patch']").textContent)
    .toBe("+generated\n".repeat(generatedLineCount));
  expect(target.querySelector(split)).toBeTruthy();
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

  const imageFile = element(target, "#file-review-0");
  const updateVisibility = fileVisibility.get(imageFile);
  if (!updateVisibility) throw new Error("Expected the file to be observed");

  updateVisibility(false);
  flushSync();
  expect(imageFile.querySelector("pre")?.textContent).toContain("old mode 100644");
  expect(imageFile.querySelector("[aria-label='Image comparison']")).toBeNull();
  expect(imageFile.querySelector(".size-change")?.textContent.trim()).toBe("0 B");

  updateVisibility(true);
  await vi.waitFor(() => expect(imageFile.querySelector("[aria-label='Image comparison'] img")).toBeTruthy());
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "read_file_details")).toHaveLength(2);
});

it("scopes the dedicated settings screen and returns to the same review position", async () => {
  const otherProject = {
    ...project,
    id          : 2,
    name        : "another",
    mainCheckout: "/tmp/another",
    worktrees   : [],
  };
  const comparison = {
    baseLabel  : "main",
    targetLabel: "HEAD",
    mode       : "between",
    base       : "abc123",
    start      : "abc123",
    end        : "def456",
    files      : ["one.txt", "two.txt"].map(path => ({
      status      : "A",
      path,
      previousPath: null,
      patch       : "@@ -0,0 +1 @@\n+new\n",
    })),
  };
  revisionCommands.mockResolvedValueOnce([project, otherProject]).mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(comparison);
  const target = render();
  const worktree = "#projects ul button";
  await vi.waitFor(() => expect(target.querySelector(worktree)).toBeTruthy());
  element(target, worktree).click();
  await vi.waitFor(() => expect(target.querySelector("#base")).toBeTruthy());
  chooseSource(target, "abc123");
  const secondFile = "[aria-label='Changed files'] button[title='two.txt']";
  await vi.waitFor(() => expect(target.querySelector(secondFile)).toBeTruthy());
  element(target, secondFile).click();
  const stream = element(target, "[aria-label='File diffs']");
  const scrollPosition = 230;
  stream.scrollTop = scrollPosition;

  const gear = element(target, "#projects footer button[title=Settings]");
  gear.click();
  const screen = await vi.waitFor(() => element(target, "main[aria-label=Settings]"));
  expect(screen.querySelector("h1")?.textContent).toBe("General");
  expect(element(target, "main:has(#projects)").inert).toBe(true);

  const scope = screen.querySelector<HTMLSelectElement>("select[aria-label='Settings scope']");
  if (!scope) throw new Error("Missing settings scope");
  expect(scope.selectedOptions[0]?.textContent).toBe("Global");
  const selectScope = (label: string): void => {
    const option = [...scope.options].find(item => item.textContent.includes(label));
    if (!option) throw new Error(`Missing scope ${ label }`);
    scope.value = option.value;
    scope.dispatchEvent(new Event("change", { bubbles: true }));
    flushSync();
  };
  selectScope("Global");
  element(screen, "nav button:first-child").click();
  await vi.waitFor(() => expect(screen.textContent).toContain("Default diff layout"));
  selectScope("another");
  expect(screen.querySelector("h1")?.textContent).toBe("File categories");
  expect(screen.textContent).toContain("File categories");
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("read_category_settings", {
    mainCheckout: otherProject.mainCheckout,
    paths       : [],
  }));

  element(screen, "nav button:first-child").click();
  const days = "#review-retention-days";
  await vi.waitFor(() => expect(screen.querySelector<HTMLInputElement>(days)?.value).toBe("7"));
  expect(screen.textContent).not.toContain("Default diff layout");
  const limit = screen.querySelector<HTMLInputElement>(days);
  if (!limit) throw new Error("Missing review inactivity limit");
  limit.value = "14";
  limit.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  element(screen, `form:has(${ days })`).dispatchEvent(new SubmitEvent("submit", {
    bubbles   : true,
    cancelable: true,
  }));
  await vi.waitFor(() => expect(screen.textContent).toContain("Use global setting"));
  element(screen, "form button[type=button]").click();
  await vi.waitFor(() => expect(limit.value).toBe("7"));

  element(screen, "button[aria-label='Back to review']").click();
  await vi.waitFor(() => expect(target.contains(screen)).toBe(false));
  expect(element(target, "[aria-label='File diffs']")).toBe(stream);
  expect(stream.scrollTop).toBe(scrollPosition);
  expect(element(target, secondFile).getAttribute("aria-pressed")).toBe("true");
  expect(document.activeElement).toBe(gear);
});

it("shows an update beside Refresh without moving the pinned comparison", async () => {
  const selection = {
    base  : "refs/heads/main",
    target: "refs/heads/topic",
    mode  : "divergence",
  };
  const pinned = {
    baseLabel  : "main",
    targetLabel: "HEAD",
    mode       : "divergence",
    base       : "mainsha",
    start      : "mainsha",
    end        : "oldsha",
    layout     : null,
    update     : "current",
    progress   : null,
    selection,
    files      : [
      {
        status      : "A",
        path        : "one.txt",
        previousPath: null,
        patch       : "+pinned\n",
      },
    ],
  };
  revisionCommands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce({
      ...revisions,
      defaultBranch: selection.base,
      currentBranch: selection.target,
    })
    .mockResolvedValueOnce(pinned)
    .mockResolvedValueOnce({
      ...pinned,
      end  : "newsha",
      files: [
        {
          ...pinned.files[0],
          patch: "+latest\n",
        },
      ],
    });
  comparisonStatus.mockResolvedValueOnce("available");

  const target = render();
  const worktree = await vi.waitFor(() => element(target, "aside ul button"));
  worktree.click();
  await vi.waitFor(() => expect(target.querySelector("pre")?.textContent).toContain("pinned"));
  globalThis.dispatchEvent(new Event("focus"));
  await vi.waitFor(() => expect(target.textContent).toContain("Update available"));
  expect(target.querySelector("pre")?.textContent).toContain("pinned");
  element(target, "#worktree-review").click();
  await vi.waitFor(() => expect(target.querySelector("pre")?.textContent).toContain("latest"));
  expect(target.textContent).not.toContain("Update available");
  expect(revisionCommands).toHaveBeenLastCalledWith("compare_revisions", {
    checkout: main.path,
    base    : selection.base,
    target  : selection.target,
    mode    : selection.mode,
    refresh : true,
  });
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
    reviewedCount: 1,
  };
  revisionCommands
    .mockResolvedValueOnce([project])
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce(comparison)
    .mockResolvedValueOnce({
      summary,
      reviewedFiles: [0],
    })
    .mockResolvedValueOnce(revisions)
    .mockResolvedValueOnce({
      ...comparison,
      layout  : "unified",
      progress: {
        summary,
        reviewedFiles: [0],
      },
    });

  const target = render();
  await vi.waitFor(() => expect(target.querySelector("aside ul button")).toBeTruthy());
  element(target, "aside ul button").click();
  await vi.waitFor(() => expect(target.querySelector("#base")).toBeTruthy());
  chooseSource(target, "abc123");
  const unified = "[aria-label='Unified patch']";
  const split = "[aria-label='Split patch']";
  await vi.waitFor(() => expect(target.querySelector(unified)).toBeTruthy());

  element(target, ".file-stream input[type=checkbox]").click();
  await vi.waitFor(() => expect(target.querySelector("#review-progress")?.textContent).toBe("Reviewed"));
  element(target, "#file-review-0 h2 button").click();
  element(target, "#projects footer button[title=Settings]").click();
  const screen = await vi.waitFor(() => element(target, "main[aria-label=Settings]"));
  const scope = screen.querySelector<HTMLSelectElement>("select[aria-label='Settings scope']");
  if (!scope) throw new Error("Missing settings scope");
  scope.selectedIndex = 0;
  scope.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  element(screen, "nav button:first-child").click();
  const splitDefault = "[aria-label='Default diff layout'] button:last-child";
  await vi.waitFor(() => expect(element(screen, splitDefault).getAttribute("aria-pressed")).toBe("false"));
  element(screen, splitDefault).click();
  await vi.waitFor(() => expect(target.querySelector(split)?.textContent).toContain("new"));
  expect(element(screen, splitDefault).getAttribute("aria-pressed")).toBe("true");
  element(screen, "button[aria-label='Back to review']").click();
  flushSync();

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
  chooseSource(target, "abc123");
  await vi.waitFor(() => expect(target.querySelector("#file-review-0 h2 button")).toBeTruthy());
  element(target, "#file-review-0 h2 button").click();
  await vi.waitFor(() => expect(target.querySelector(unified)).toBeTruthy());
});
