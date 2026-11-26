import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { mount, unmount } from "svelte";
import { assert, expect, it, onTestFinished, vi } from "vitest";
import { ssr } from "../src/routes/+layout.js";
import Page from "../src/routes/+page.svelte";

it("renders on the client for Tauri's static frontend", () => {
  expect(ssr).toBe(false);
});

it("submits a name to Tauri and displays the greeting", async () => {
  const greeting = "Hello, Ada! You've been greeted from Rust!";
  const invoke = vi.fn().mockReturnValue(greeting);
  mockIPC(invoke);

  const page = mount(Page, { target: document.body });
  onTestFinished(async () => {
    await unmount(page);
    clearMocks();
  });

  const form = document.querySelector("form");
  const input = form?.querySelector("input");
  assert(form && input);
  input.value = "Ada";
  input.dispatchEvent(new Event("input"));

  const submit = new Event("submit", {
    bubbles   : true,
    cancelable: true,
  });
  form.dispatchEvent(submit);

  expect(submit.defaultPrevented).toBe(true);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("greet", { name: "Ada" });
  await expect.poll(() => document.body.textContent).toContain(greeting);
});
