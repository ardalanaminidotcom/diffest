# Diffest Desktop

Install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. From the repository root, run `pnpm install`, then `pnpm --filter diffest-desktop dev` to start the desktop app.

Diffest uses the `git` executable on your `PATH`. Select **+** beside **Projects** and choose a local Git checkout folder. Its linked worktrees appear in the Projects sidebar. Select a worktree to see its changed files on the right.

Run `pnpm build` to build and bundle the app for your current platform. Run `pnpm --filter diffest-desktop check` and `pnpm test:coverage --reporter=minimal` to check the Svelte frontend and its tests.

Run `cargo test -p diffest-core` to test project registration and comparisons against temporary Git repositories and SQLite databases.

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).
