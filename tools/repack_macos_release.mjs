// After sign_macos_bundle.sh has re-signed the sidecars and the .app with
// entitlements, the Tauri-generated Parrot.app.tar.gz (+ .sig) and DMG still
// embed the OLD unsigned .app. Regenerate them here so auto-update and DMG
// installs ship the entitled version.

import { execFile } from "node:child_process";
import { readFile, rm, stat } from "node:fs/promises";
import { resolve, dirname, basename } from "node:path";
import { promisify } from "node:util";

const exec = promisify(execFile);

const root = resolve(import.meta.dirname, "..");
const tauriConfig = JSON.parse(
  await readFile(resolve(root, "src-tauri/tauri.conf.json"), "utf8"),
);
const version = tauriConfig.version;

const macosBundle = resolve(root, "src-tauri/target/release/bundle/macos");
const dmgBundle = resolve(root, "src-tauri/target/release/bundle/dmg");
const appPath = resolve(macosBundle, "Parrot.app");
const tarballPath = resolve(macosBundle, "Parrot.app.tar.gz");
const sigPath = resolve(macosBundle, "Parrot.app.tar.gz.sig");
const dmgPath = resolve(dmgBundle, `Parrot_${version}_aarch64.dmg`);

try {
  await stat(appPath);
} catch {
  throw new Error(`Signed .app missing at ${appPath}. Run sign step first.`);
}

if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
  throw new Error(
    "TAURI_SIGNING_PRIVATE_KEY must be set to re-sign the updater tarball.",
  );
}

// 1. Drop stale artifacts.
await Promise.all(
  [tarballPath, sigPath, dmgPath].map((p) =>
    rm(p, { force: true, recursive: true }),
  ),
);

// 2. Repack Parrot.app.tar.gz from the signed .app, matching Tauri's layout
//    (Parrot.app/ at the archive root).
console.log(`Packing ${basename(tarballPath)}...`);
await exec(
  "tar",
  ["-C", dirname(appPath), "-czf", tarballPath, basename(appPath)],
);

// 3. Sign the new tarball with Tauri's updater key.
console.log(`Signing ${basename(tarballPath)}...`);
await exec("npx", ["tauri", "signer", "sign", tarballPath], {
  cwd: root,
  env: process.env,
});

// 4. Rebuild DMG from the signed .app (UDZO matches Tauri's bundle_dmg output).
console.log(`Packing ${basename(dmgPath)}...`);
await exec("hdiutil", [
  "create",
  "-volname",
  "Parrot",
  "-srcfolder",
  appPath,
  "-ov",
  "-format",
  "UDZO",
  dmgPath,
]);

const [archiveStat, signatureStat, dmgStat] = await Promise.all([
  stat(tarballPath),
  stat(sigPath),
  stat(dmgPath),
]);

console.log(
  `Done. Tarball ${(archiveStat.size / 1_000_000).toFixed(1)} MB, sig ${signatureStat.size} B, DMG ${(dmgStat.size / 1_000_000).toFixed(1)} MB.`,
);
