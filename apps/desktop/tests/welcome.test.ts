import { mount, unmount } from "svelte";
import { expect, it, onTestFinished } from "vitest";
import { ssr } from "../src/routes/+layout.js";
import Page from "../src/routes/+page.svelte";

it("renders on the client for Tauri's static frontend", () => {
  expect(ssr).toBe(false);
});

it("shows the Diffest welcome screen", () => {
  const target = document.createElement("div");
  const page = mount(Page, { target });
  onTestFinished(async () => unmount(page));

  expect(target.querySelector("main h1")?.textContent).toBe("Diffest");
});
