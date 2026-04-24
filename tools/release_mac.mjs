// Orchestrates the full signed macOS release build. Reads the minisign
// signing key (and its password) from `~/.tauri/parrot.key[.password]`
// automatically, so `npm run release:mac` works end-to-end without any
// manual `export TAURI_SIGNING_PRIVATE_KEY=...` dance.
//
// Steps:
//   1. prepare_release.mjs      — wipe stale artifacts
//   2. tauri build              — compile + bundle .app, .dmg, .app.tar.gz (+ .sig)
//   3. sign_macos_bundle.sh     — ad-hoc codesign sidecars + app with entitlements
//   4. repack_macos_release.mjs — rebuild + re-sign tar.gz after codesign changes
//   5. generate_latest_json.mjs — emit latest.json for the in-app updater

import { spawnSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { resolve } from "node:path";

const TAURI_DIR = resolve(homedir(), ".tauri");
const KEY_PATH = resolve(TAURI_DIR, "parrot.key");
const PASSWORD_PATH = resolve(TAURI_DIR, "parrot.key.password");

if (!existsSync(KEY_PATH)) {
  console.error(
    `❌ Signing key not found at ${KEY_PATH}.\n` +
      `   Restore it from backup or regenerate with \`npx tauri signer generate\`.\n` +
      `   Public half must match tauri.conf.json plugins.updater.pubkey.`,
  );
  process.exit(1);
}

const key = readFileSync(KEY_PATH, "utf8").trim();
const password = existsSync(PASSWORD_PATH)
  ? readFileSync(PASSWORD_PATH, "utf8").replace(/\r?\n$/, "")
  : "";

// Child processes inherit these env vars for every downstream step.
const env = {
  ...process.env,
  TAURI_SIGNING_PRIVATE_KEY: key,
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password,
};

const root = resolve(import.meta.dirname, "..");

function run(label, cmd, args) {
  console.log(`\n▶ ${label}`);
  const result = spawnSync(cmd, args, { stdio: "inherit", env, cwd: root });
  if (result.status !== 0) {
    console.error(`\n❌ Step failed: ${label}`);
    process.exit(result.status ?? 1);
  }
}

run("Clean stale artifacts", "node", ["tools/prepare_release.mjs"]);
run("Tauri build (signed)", "npx", [
  "tauri",
  "build",
  "--config",
  "src-tauri/tauri.release.conf.json",
]);
run("Codesign app + sidecars", "bash", ["tools/sign_macos_bundle.sh"]);
run("Repack + re-sign tarball", "node", ["tools/repack_macos_release.mjs"]);
run("Write latest.json + Parrot.dmg alias", "node", [
  "tools/generate_latest_json.mjs",
]);

console.log("\n✅ Release build complete. Next: `node tools/upload_release.mjs`.");
