import type { FileDetails } from "../src/lib/file-details.js";
import { flushSync, mount, unmount } from "svelte";
import { expect, it, onTestFinished } from "vitest";
import FilePreviews from "../src/lib/file-previews.svelte";

function render(details: FileDetails): HTMLDivElement {
  const target = document.createElement("div");
  document.body.append(target);
  const panel = mount(FilePreviews, {
    target,
    props: { details },
  });
  onTestFinished(async () => {
    await unmount(panel);
    target.remove();
  });
  flushSync();
  return target;
}

it("compares image previews and shows an unavailable message if one fails", () => {
  const target = render({
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
  const images = target.querySelectorAll("img");
  expect([...images].map(image => image.alt)).toEqual([
    "Before preview of before.gif",
    "After preview of after.png",
  ]);
  const before = images.item(0);
  Object.defineProperties(before, {
    naturalWidth : { value: 160 },
    naturalHeight: { value: 90 },
  });
  before.dispatchEvent(new Event("load"));
  images.item(1).dispatchEvent(new Event("error"));
  flushSync();
  expect(target.textContent).toContain("160 × 90 px · 16:9");
  expect(target.textContent).toContain("Preview unavailable");
  expect(target.querySelectorAll("img")).toHaveLength(1);
});

it("distinguishes an absent file from an unavailable preview", () => {
  const target = render({
    before: {
      path    : "broken.webp",
      format  : "WebP",
      byteSize: 18,
      preview : null,
    },
    after: null,
  });
  expect(target.textContent).toContain("File absent");
  expect(target.textContent).toContain("Preview unavailable");
  expect(target.querySelector("img")).toBeNull();
});
