# Diffest

Diffest desktop application.

## Cargo workspace

The desktop app in `apps/desktop/src-tauri` shares the root `Cargo.lock`, release profile, and `target/` directory with other workspace members. Run `cargo new --lib crates/my-crate` from the repository root to create and register a library crate. A default `pnpm build` writes Tauri bundles to `target/release/bundle/`.

## Desktop CI and releases

Create a GitHub Actions environment named `build` with the maintainers as required reviewers and leave its secrets empty. If the environment restricts refs, allow `refs/pull/*/merge`. Enable approval for all external fork workflows and keep code-owner review required on `main`. Review changes to workflows and build scripts before approving them. The existing required **Build** check passes only after all six **Desktop compile** checks pass. Keep **Build** and up-to-date branches required in the `main` ruleset, and verify the approval flow on a fork PR. Lint and tests must pass before the native jobs request approval.

To prepare a release, configure Git commit and tag signing with a signing key registered on GitHub. A release maintainer needs permission to push directly to `main` through the ruleset that requires PRs and checks.

```sh
git switch main
git pull --ff-only origin main
node scripts/release-desktop.mjs 0.1.0
git show --stat HEAD
git show --stat desktop@v0.1.0
```

The script updates the Desktop manifests and Cargo lockfile, then creates a signed release commit with DCO sign-off and a signed annotated tag. If a commit hook fails, fix the issue and review the staged changes before making the signed release commit. If tag signing fails after the commit succeeds, sign the tag on that commit after correcting the signing setup. Review both refs before pushing them in the same operation.

```sh
git push --atomic origin main refs/tags/desktop@v0.1.0
```

**Desktop release** builds three ad-hoc-signed macOS DMGs (Apple Silicon, Intel, and Universal), unsigned Windows NSIS and MSI installers for x64 and ARM64, and Linux AppImage, Debian, and RPM packages for x64 and ARM64. Review and publish the GitHub draft after the workflow confirms all 13 assets. Re-run failed jobs or dispatch **Desktop release** from `main` for the same tag to retry infrastructure failures. Use a new version for source fixes. Keep the repository’s **Immutable releases** setting enabled so publication locks the tag and assets. To verify a downloaded installer against the published release, run `gh release verify-asset desktop@v0.1.0 PATH_TO_INSTALLER --repo ardalanaminidotcom/diffest`.

## 3rd Party Agent Skills Reference

- Included:
  - [karpathy-guidelines](.agents/skills/karpathy-guidelines/SKILL.md) is copied from [andrej-karpathy-skills GitHub repository](https://github.com/multica-ai/andrej-karpathy-skills)
- Recommended:
  - [ponytail](https://ponytail.dev)
  - [caveman](https://www.getcaveman.dev)

## AI Contributions

If you are an AI agent contributing to this project, please refer to [AGENTS.md](./AGENTS.md) for guidelines that must be followed.
