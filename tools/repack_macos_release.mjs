// After sign_macos_bundle.sh has re-signed the sidecars and the .app with
// entitlements, the Tauri-generated Parrot.app.tar.gz (+ .sig) and DMG still
// embed the OLD unsigned .app. Regenerate them here so auto-update and DMG
// installs ship the entitled version.

import { execFile } from "node:child_process";
import { cp, mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, dirname, basename, join } from "node:path";
import { promisify } from "node:util";

const exec = promisify(execFile);

const root = resolve(import.meta.dirname, "..");
const tauriConfig = JSON.parse(
  await readFile(resolve(root, "src-tauri/tauri.conf.json"), "utf8"),
);
const version = tauriConfig.version;

const macosBundle = resolve(root, "src-tauri/target/release/bundle/macos");
const dmgBundle = resolve(root, "src-tauri/target/release/bundle/dmg");
const bundleDmgScript = resolve(dmgBundle, "bundle_dmg.sh");
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
//    IMPORTANT: macOS BSD tar writes AppleDouble resource forks (`._*` files)
//    into the archive by default. Tauri's updater then tries to unpack those
//    siblings as real apps and bails with
//    "failed to unpack `._Parrot.app` into …/tauri_updated_app…/".
//    COPYFILE_DISABLE=1 + --no-mac-metadata tell tar to skip them.
console.log(`Packing ${basename(tarballPath)}...`);
await exec(
  "tar",
  [
    "--no-mac-metadata",
    "--no-xattrs",
    "-C",
    dirname(appPath),
    "-czf",
    tarballPath,
    basename(appPath),
  ],
  { env: { ...process.env, COPYFILE_DISABLE: "1" } },
);

// 3. Sign the new tarball with Tauri's updater key.
console.log(`Signing ${basename(tarballPath)}...`);
await exec("npx", ["tauri", "signer", "sign", tarballPath], {
  cwd: root,
  env: process.env,
});

// 4. Rebuild DMG from the signed .app using Tauri's own bundle_dmg.sh,
//    so the installer window keeps the "drag to Applications" layout
//    (otherwise hdiutil -srcfolder yields a bare DMG with no symlink).
console.log(`Packing ${basename(dmgPath)}...`);
try {
  await stat(bundleDmgScript);
} catch {
  throw new Error(
    `bundle_dmg.sh missing at ${bundleDmgScript}. Run \`tauri build\` first so Tauri generates it.`,
  );
}

const stagingDir = await mkdtemp(join(tmpdir(), "parrot-dmg-"));
try {
  await cp(appPath, join(stagingDir, basename(appPath)), {
    recursive: true,
    verbatimSymlinks: true,
  });

  await exec(
    "bash",
    [
      bundleDmgScript,
      "--volname",
      "Parrot",
      "--icon",
      basename(appPath),
      "180",
      "170",
      "--app-drop-link",
      "480",
      "170",
      "--window-size",
      "660",
      "400",
      "--icon-size",
      "80",
      "--hide-extension",
      basename(appPath),
      "--format",
      "UDZO",
      dmgPath,
      stagingDir,
    ],
    { cwd: dmgBundle },
  );
} finally {
  await rm(stagingDir, { recursive: true, force: true });
}

const [archiveStat, signatureStat, dmgStat] = await Promise.all([
  stat(tarballPath),
  stat(sigPath),
  stat(dmgPath),
]);

console.log(
  `Done. Tarball ${(archiveStat.size / 1_000_000).toFixed(1)} MB, sig ${signatureStat.size} B, DMG ${(dmgStat.size / 1_000_000).toFixed(1)} MB.`,
);
