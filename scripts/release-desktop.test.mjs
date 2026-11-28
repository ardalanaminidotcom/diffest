import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { test } from "node:test";

test("prepares a signed desktop commit and tag locally", (t) => {
  const temporary = mkdtempSync(join(tmpdir(), "diffest-release-"));
  t.after(() => rmSync(temporary, {
    recursive: true,
    force    : true,
  }));
  const cwd = join(temporary, "repo");
  mkdirSync(cwd);
  const git = (...gitArguments) => execFileSync("git", gitArguments, {
    cwd,
    encoding: "utf8",
  }).trim();
  const key = join(temporary, "signing-key");
  execFileSync("ssh-keygen", ["-q", "-t", "ed25519", "-N", "", "-f", key]);

  const files = [
    "Cargo.lock",
    "apps/desktop/package.json",
    "apps/desktop/src-tauri/Cargo.toml",
    "apps/desktop/src-tauri/tauri.conf.json",
    "crates/diffest-core/Cargo.toml",
    "package.json",
  ];
  const root = resolve(import.meta.dirname, "..");
  for (const file of files) {
    mkdirSync(dirname(join(cwd, file)), { recursive: true });
    copyFileSync(join(root, file), join(cwd, file));
  }

  git("init", "-b", "main");
  git("config", "user.name", "Diffest Maintainer");
  git("config", "user.email", "release@example.test");
  git("config", "gpg.format", "ssh");
  git("config", "user.signingkey", key);
  git("add", ".");
  git("commit", "--no-gpg-sign", "-m", "chore: initialize fixture");
  const base = git("rev-parse", "HEAD");
  git("update-ref", "refs/remotes/origin/main", base);

  const script = join(root, "scripts/release-desktop.mjs");
  const current = JSON.parse(readFileSync(join(cwd, "apps/desktop/package.json"))).version;
  const [major, minor, patch] = current.split(".").map(Number);
  const version = [major, minor, patch + 1].join(".");
  const tag = `desktop@v${ version }`;
  const result = spawnSync(process.execPath, [script, version], {
    cwd,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(git("diff", "--name-only", base, "HEAD").split("\n"), files.slice(0, 4));
  assert.ok(readFileSync(join(cwd, "Cargo.lock"), "utf8").includes(`name = "diffest-desktop"\nversion = "${ version }"`));
  assert.ok(readFileSync(join(cwd, "apps/desktop/src-tauri/Cargo.toml"), "utf8").includes(`version = "${ version }"`));
  for (const file of ["apps/desktop/package.json", "apps/desktop/src-tauri/tauri.conf.json"]) {
    const manifest = JSON.parse(readFileSync(join(cwd, file)));
    assert.equal(manifest.version, version);
  }

  assert.match(git("cat-file", "-p", "HEAD"), /BEGIN SSH SIGNATURE/);
  assert.match(git("log", "-1", "--format=%B"), /Signed-off-by: Diffest Maintainer <release@example\.test>/u);
  assert.match(git("cat-file", "-p", tag), /BEGIN SSH SIGNATURE/);
  assert.equal(git("rev-parse", `${ tag }^{commit}`), git("rev-parse", "HEAD"));
  assert.equal(git("rev-parse", "origin/main"), base);
  assert.equal(git("status", "--porcelain"), "");
  execFileSync(process.execPath, [script, "--verify", version], { cwd });
  const invalid = spawnSync(process.execPath, [script, "--verify", current], {
    cwd,
    encoding: "utf8",
  });
  assert.equal(invalid.status, 1, invalid.stderr);
  assert.match(invalid.stderr, /does not match version/u);
});
