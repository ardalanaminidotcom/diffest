import { invoke } from "@tauri-apps/api/core";
import { flushSync, mount, unmount } from "svelte";
import { expect, it, onTestFinished, vi } from "vitest";
import CategorySettings from "../src/lib/category-settings.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const globalPath = "/home/demo/.diffest/settings.json";
const projectPath = "/repos/main/.diffest/settings.json";
const rules = [
  {
    glob       : "**/*.rs",
    category   : "Rust",
    subcategory: null,
  },
  {
    glob       : "**/tests/**",
    category   : "Tests",
    subcategory: "Unit",
  },
];

function render(activeProjectId?: number): HTMLDivElement {
  const target = document.createElement("div");
  document.body.append(target);
  const settings = mount(CategorySettings, {
    target,
    props: {
      projects: [
        {
          id           : 1,
          name         : "demo",
          mainCheckout : "/repos/main",
          worktrees    : [],
          worktreeError: null,
          savedReviews : [],
        },
      ],
      activeProjectId,
      drafts : {},
      onback : vi.fn(),
      onsaved: vi.fn().mockResolvedValue(null),
    },
  });
  onTestFinished(async () => {
    await unmount(settings);
    target.remove();
  });
  flushSync();
  return target;
}

function button(target: HTMLElement, label: string): HTMLButtonElement {
  const result = [...target.querySelectorAll("button")]
    .find(item => item.textContent.trim() === label || item.getAttribute("aria-label") === label);
  if (!result) throw new Error(`Missing button ${ label }`);

  return result;
}

it("copies inherited rules, reorders them, and keeps the draft when a save conflicts", async () => {
  const read = {
    global: {
      path   : globalPath,
      version: "global",
      error  : null,
      custom : true,
      rules,
    },
    project: {
      path   : projectPath,
      version: "original",
      error  : null,
      custom : false,
      rules,
    },
    categories: [],
  };
  vi.mocked(invoke)
    .mockReset()
    .mockResolvedValueOnce(read)
    .mockRejectedValueOnce(`${ projectPath }: Settings changed on disk. Reload before saving.`)
    .mockResolvedValue({
      ...read,
      project: {
        ...read.project,
        version: "external",
        custom : true,
      },
    });

  const target = render(1);
  const form = await vi.waitFor(() => {
    const projectForm = target.querySelector<HTMLFormElement>("form[aria-label='Project category rules']");
    expect(projectForm).toBeTruthy();
    return projectForm!;
  });
  expect(form.textContent).toContain(projectPath);
  expect(form.textContent).toContain("Inherited from Global");
  button(form, "Customize rules").click();
  flushSync();
  expect(form.querySelector<HTMLInputElement>("input[name='glob']")?.value).toBe("**/*.rs");
  button(form, "Move rule 2 up").click();
  flushSync();
  button(form, "Save rules").click();
  await vi.waitFor(() => {
    expect(form.querySelector("[role='alert']")?.textContent).toContain("Settings changed on disk");
  });
  expect(invoke).toHaveBeenCalledWith("save_category_settings", {
    mainCheckout: "/repos/main",
    version     : "original",
    rules       : rules.toReversed(),
  });
  expect(form.querySelector<HTMLInputElement>("input[name='glob']")?.value).toBe("**/tests/**");

  globalThis.dispatchEvent(new Event("focus"));
  await vi.waitFor(() => {
    expect(form.textContent).toContain("Discard edits to load the file");
  });
  expect(button(form, "Save rules").disabled).toBe(true);
  button(form, "Discard edits").click();
  flushSync();
  expect(form.textContent).toContain("Custom rules");
});

it("saves new global category and subcategory labels and resets to the built-in rules", async () => {
  const inherited = {
    global: {
      path   : globalPath,
      version: "missing",
      error  : null,
      custom : false,
      rules,
    },
    project   : null,
    categories: [],
  };
  const customRule = {
    glob       : "**/fixtures/**",
    category   : "Fixtures",
    subcategory: "Golden",
  };
  vi.mocked(invoke)
    .mockReset()
    .mockResolvedValueOnce(inherited)
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce({
      ...inherited,
      global: {
        ...inherited.global,
        version: "custom",
        custom : true,
        rules  : [rules[1], customRule],
      },
    })
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce(inherited)
    .mockResolvedValueOnce({
      ...inherited,
      global: {
        ...inherited.global,
        error: `${ globalPath }: Rule 1: invalid glob`,
      },
    })
    .mockResolvedValue(inherited);

  const target = render();
  button(target, "File categories").click();
  await vi.waitFor(() => {
    expect(target.textContent).toContain("Built-in rules");
  });
  button(target, "Customize rules").click();
  flushSync();
  button(target, "Remove rule").click();
  button(target, "General").click();
  flushSync();
  button(target, "File categories").click();
  flushSync();
  expect(target.querySelectorAll("ol li")).toHaveLength(1);
  button(target, "Add rule").click();
  flushSync();
  const values = Object.values(customRule);
  const inputs = [...target.querySelectorAll<HTMLInputElement>("ol li:last-child input")];
  expect(inputs).toHaveLength(values.length);
  for (const [index, field] of inputs.entries()) {
    field.value = values[index] ?? "";
    field.dispatchEvent(new Event("input", { bubbles: true }));
  }
  flushSync();
  button(target, "Save rules").click();
  await vi.waitFor(() => {
    expect(target.textContent).toContain("Custom rules");
  });
  expect(invoke).toHaveBeenCalledWith("save_category_settings", {
    mainCheckout: null,
    version     : "missing",
    rules       : [rules[1], customRule],
  });
  button(target, "Reset to built-in rules").click();
  await vi.waitFor(() => {
    expect(target.textContent).toContain("Built-in rules");
  });
  expect(invoke).toHaveBeenCalledWith("save_category_settings", {
    mainCheckout: null,
    version     : "custom",
    rules       : null,
  });

  globalThis.dispatchEvent(new Event("focus"));
  await vi.waitFor(() => {
    expect(target.querySelector("[role='alert']")?.textContent).toContain(globalPath);
    expect(target.querySelector("[role='alert']")?.textContent).toContain("invalid glob");
  });
  globalThis.dispatchEvent(new Event("focus"));
  await vi.waitFor(() => {
    expect(target.querySelector("[role='alert']")).toBeNull();
  });
});
