import { execFileSync } from "node:child_process";
import console from "node:console";
import { readFileSync, writeFileSync } from "node:fs";
import process from "node:process";

const desktop = "apps/desktop";
const jsonVersion = /^( {2}"version": ")([^"\r\n]+)(")/mu;
const msiMajorMinorLimit = 255;
const msiPatchLimit = 65_535;
const versionFiles = [
  [`${ desktop }/package.json`, jsonVersion],
  [`${ desktop }/src-tauri/tauri.conf.json`, jsonVersion],
  [`${ desktop }/src-tauri/Cargo.toml`, /^(\[package\]\r?\nname = "diffest-desktop"\r?\nversion = ")([^"\r\n]+)(")/mu],
  ["Cargo.lock", /^(\[\[package\]\]\r?\nname = "diffest-desktop"\r?\nversion = ")([^"\r\n]+)(")/mu],
];

function git(...gitArguments) {
  return execFileSync("git", gitArguments, { encoding: "utf8" }).trim();
}

function versionParts(version) {
  const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/u;
  if (!stableVersion.test(version)) throw new Error("Use a stable version in MAJOR.MINOR.PATCH format.");

  const parts = version.split(".").map(Number);
  const [major, minor, patch] = parts;
  const exceedsMsiLimit = major > msiMajorMinorLimit || minor > msiMajorMinorLimit || patch > msiPatchLimit;
  if (exceedsMsiLimit) throw new Error("Windows MSI supports version components up to 255.255.65535.");
  return parts;
}

function readVersions() {
  return versionFiles.map(([path, pattern]) => {
    const contents = readFileSync(path, "utf8");
    const match = pattern.exec(contents);
    if (!match) throw new Error(`Cannot find the Desktop version in ${ path }.`);
    const [line, prefix, version, suffix] = match;
    return {
      path,
      contents,
      index: match.index,
      line,
      prefix,
      version,
      suffix,
    };
  });
}

function prepare(version, parts, files) {
  const onMain = git("branch", "--show-current") === "main";
  const atMainTip = onMain && git("rev-parse", "HEAD") === git("rev-parse", "origin/main");
  if (!atMainTip) throw new Error("Start on main at origin/main. Fetch and fast-forward main first.");

  if (git("status", "--porcelain")) throw new Error("Start with a clean worktree.");
  const tag = `desktop@v${ version }`;
  if (git("tag", "--list", tag)) throw new Error(`Tag ${ tag } already exists.`);

  const current = files[0].version;
  if (files.some(file => file.version !== current)) throw new Error("Desktop versions disagree.");
  const previous = versionParts(current);
  const difference = parts.findIndex((part, index) => part !== previous[index]);
  if (difference === -1 || parts[difference] < previous[difference]) throw new Error(`Choose a version newer than ${ current }.`);

  for (const { path, contents, index, line, prefix, suffix } of files) {
    const updated = contents.slice(0, index) + prefix + version + suffix + contents.slice(index + line.length);
    writeFileSync(path, updated);
  }

  git("add", "--", ...files.map(file => file.path));
  git("commit", "--gpg-sign", "--signoff", "-m", `chore(desktop): release v${ version }`);
  git("tag", "--sign", "--message", `Diffest Desktop v${ version }`, tag);
  console.info(`Prepared signed commit and tag ${ tag }. Review them before pushing.`);
}

try {
  const commandArguments = process.argv.slice(2);
  const verify = commandArguments[0] === "--verify";
  const version = commandArguments[verify ? 1 : 0] ?? "";
  const usage = "Usage: node scripts/release-desktop.mjs [--verify] MAJOR.MINOR.PATCH";
  if (commandArguments.length !== (verify ? 2 : 1)) throw new Error(usage);

  const parts = versionParts(version);
  const files = readVersions();
  if (verify) {
    const mismatch = files.find(file => file.version !== version);
    if (mismatch) throw new Error(`${ mismatch.path } does not match version ${ version }.`);
  } else {
    prepare(version, parts, files);
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
