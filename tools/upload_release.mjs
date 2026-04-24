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

// Post-upload verification: make sure the auto-updater manifest GitHub
// actually serves matches the one we just built, and that the tar.gz it
// references is reachable. Catches broken releases before users hit them.
const latestJsonUrl = `https://github.com/${repo}/releases/latest/download/latest.json`;
console.log(`Verifying ${latestJsonUrl} ...`);

async function verifyManifest(attempt = 1) {
  const res = await fetch(latestJsonUrl, { redirect: "follow" });
  if (!res.ok) {
    throw new Error(`latest.json HTTP ${res.status} from ${latestJsonUrl}`);
  }
  const manifest = await res.json();
  if (manifest.version !== version) {
    // GitHub CDN sometimes lags a few seconds after `release upload` — retry
    // up to 3 times before failing the release.
    if (attempt < 3) {
      await new Promise((r) => setTimeout(r, 2000 * attempt));
      return verifyManifest(attempt + 1);
    }
    throw new Error(
      `latest.json serves version=${manifest.version}, expected ${version}. ` +
        `GitHub may still be propagating the release — try again in a minute.`,
    );
  }
  const platform = manifest?.platforms?.["darwin-aarch64"];
  if (!platform?.signature || !platform?.url) {
    throw new Error(
      `latest.json missing platforms["darwin-aarch64"].{signature,url}: ${JSON.stringify(manifest)}`,
    );
  }
  const headRes = await fetch(platform.url, { method: "HEAD", redirect: "follow" });
  if (!headRes.ok) {
    throw new Error(
      `Updater tarball not reachable: HEAD ${platform.url} → HTTP ${headRes.status}`,
    );
  }
  return manifest.version;
}

const verifiedVersion = await verifyManifest();
console.log(`✓ Updater manifest serves v${verifiedVersion} and tarball is reachable.`);

console.log(`Done. Stable links:`);
console.log(`  https://github.com/${repo}/releases/latest/download/Parrot.dmg`);
console.log(`  ${latestJsonUrl}`);
