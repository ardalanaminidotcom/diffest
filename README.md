# Diffest

Diffest desktop application.

## Cargo workspace

The desktop app in `apps/desktop/src-tauri` shares the root `Cargo.lock`, release profile, and `target/` directory with other workspace members. Run `cargo new --lib crates/my-crate` from the repository root to create and register a library crate. A default `pnpm build` writes Tauri bundles to `target/release/bundle/`.

## 3rd Party Agent Skills Reference

- Included:
  - [karpathy-guidelines](.agents/skills/karpathy-guidelines/SKILL.md) is copied from [andrej-karpathy-skills GitHub repository](https://github.com/multica-ai/andrej-karpathy-skills)
- Recommended:
  - [ponytail](https://ponytail.dev)
  - [caveman](https://www.getcaveman.dev)

## AI Contributions

If you are an AI agent contributing to this project, please refer to [AGENTS.md](./AGENTS.md) for guidelines that must be followed.
