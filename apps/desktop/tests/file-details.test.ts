import { invoke } from "@tauri-apps/api/core";
import { flushSync, mount, unmount } from "svelte";
import { expect, it, onTestFinished, vi } from "vitest";
import FileDetails from "../src/lib/file-details.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function render(): HTMLDivElement {
  const target = document.createElement("div");
  document.body.append(target);
  const panel = mount(FileDetails, {
    target,
    props: {
      checkout: "/repo",
      source  : {
        kind       : "worktree",
        fingerprint: "selected patch",
      },
    },
  });
  onTestFinished(async () => {
    await unmount(panel);
    target.remove();
  });
  flushSync();
  return target;
}

it("compares image paths, formats, sizes and dimensions with a fallback for a failed preview", async () => {
  vi.mocked(invoke).mockResolvedValue({
    before: {
      path    : "before.gif",
      format  : "GIF",
      byteSize: 48,
      preview : "data:image/gif;base64,R0lGODlh",
    },
    after: {
      path    : "after.png",
      format  : "PNG",
      byteSize: 120,
      preview : "data:image/png;base64,iVBORw0KGgo=",
    },
  });
  const target = render();
  await vi.waitFor(() => expect(target.querySelectorAll("img")).toHaveLength(2));
  const images = target.querySelectorAll("img");
  const before = images.item(0);
  const after = images.item(1);
  Object.defineProperties(before, {
    naturalWidth : { value: 160 },
    naturalHeight: { value: 90 },
  });
  before.dispatchEvent(new Event("load"));
  after.dispatchEvent(new Event("error"));
  flushSync();
  expect(target.textContent).toContain("before.gif");
  expect(target.textContent).toContain("GIF");
  expect(target.textContent).toContain("48 bytes");
  expect(target.textContent).toContain("160 × 90 px");
  expect(target.textContent).toContain("16:9");
  expect(target.textContent).toContain("after.png");
  expect(target.textContent).toContain("PNG");
  expect(target.textContent).toContain("120 bytes");
  expect(target.textContent).toContain("+72 bytes");
  expect(target.textContent).toContain("Preview unavailable");
  expect(target.querySelectorAll("img")).toHaveLength(1);
});

it("shows the absent side of a deletion even when its image has no preview", async () => {
  vi.mocked(invoke).mockResolvedValue({
    before: {
      path    : "broken.webp",
      format  : "WebP",
      byteSize: 18,
      preview : null,
    },
    after: null,
  });
  const target = render();
  await vi.waitFor(() => expect(target.textContent).toContain("File absent"));
  expect(target.textContent).toContain("-18 bytes");
  expect(target.textContent).toContain("Preview unavailable");
  expect(target.querySelector("img")).toBeNull();
});
