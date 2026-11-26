# Tauri + SvelteKit + TypeScript

Install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. From the repository root, run `pnpm install`, then `pnpm --filter tauri-app dev` to start the desktop app.

Run `pnpm build` to build and bundle the app for your current platform. Run `pnpm typecheck` and `pnpm test:coverage --reporter=minimal` to check the Svelte frontend and its tests.

Before distributing an app based on this template, set its product name, bundle identifier, and window title in `src-tauri/tauri.conf.json`.

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).
