// Uploads built artifacts to a GitHub release via the `gh` CLI.
// Creates the release if it doesn't exist, then uploads both the
// versioned DMG (Parrot_<version>_aarch64.dmg) and the stable-name copy
// (Parrot.dmg) so https://github.com/<repo>/releases/latest/download/Parrot.dmg
// always resolves to the newest build.

import { execFile } from "node:child_process";
import { readFile, stat } from "node:fs/promises";
import { resolve } from "node:path";
import { promisify } from "node:util";

const exec = promisify(execFile);

const repo = process.env.PARROT_RELEASE_REPO ?? "ungurenko/parrot";
const root = resolve(import.meta.dirname, "..");
const tauriConfig = JSON.parse(
  await readFile(resolve(root, "src-tauri/tauri.conf.json"), "utf8"),
);
const version = tauriConfig.version;
const tag = `v${version}`;

const macosBundle = resolve(root, "src-tauri/target/release/bundle/macos");
const dmgBundle = resolve(root, "src-tauri/target/release/bundle/dmg");

const assets = [
  resolve(dmgBundle, `Parrot_${version}_aarch64.dmg`),
  resolve(dmgBundle, "Parrot.dmg"),
  resolve(macosBundle, "Parrot.app.tar.gz"),
  resolve(macosBundle, "Parrot.app.tar.gz.sig"),
  resolve(macosBundle, "latest.json"),
];

for (const path of assets) {
  try {
    await stat(path);
  } catch {
    throw new Error(`Missing release asset: ${path}. Run npm run release:mac first.`);
  }
}

const { stdout: releaseCheck } = await exec(
  "gh",
  ["release", "view", tag, "--repo", repo, "--json", "tagName"],
  { reject: false },
).catch((err) => ({ stdout: "", error: err }));

if (!releaseCheck) {
  console.log(`Creating release ${tag}...`);
  await exec("gh", [
    "release",
    "create",
    tag,
    "--repo",
    repo,
    "--title",
    `Parrot ${tag}`,
    "--generate-notes",
  ]);
} else {
  console.log(`Release ${tag} exists, uploading assets with --clobber.`);
}

console.log(`Uploading ${assets.length} assets to ${tag}...`);
await exec("gh", [
  "release",
  "upload",
  tag,
  ...assets,
  "--repo",
  repo,
  "--clobber",
]);

console.log(`Done. Stable links:`);
console.log(`  https://github.com/${repo}/releases/latest/download/Parrot.dmg`);
console.log(`  https://github.com/${repo}/releases/latest/download/latest.json`);
