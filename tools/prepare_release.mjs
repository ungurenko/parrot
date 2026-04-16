import { readFile, rm } from "node:fs/promises";
import { resolve } from "node:path";

if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
  throw new Error(
    "Для релизной сборки нужен TAURI_SIGNING_PRIVATE_KEY. Для обычной локальной сборки используйте npm run build:local.",
  );
}

const root = resolve(import.meta.dirname, "..");
const tauriConfig = JSON.parse(
  await readFile(resolve(root, "src-tauri/tauri.conf.json"), "utf8"),
);
const version = tauriConfig.version;
const bundleDir = resolve(root, "src-tauri/target/release/bundle/macos");
const dmgDir = resolve(root, "src-tauri/target/release/bundle/dmg");

const staleArtifacts = [
  resolve(bundleDir, "latest.json"),
  resolve(bundleDir, "Parrot.app.tar.gz"),
  resolve(bundleDir, "Parrot.app.tar.gz.sig"),
  resolve(dmgDir, "Parrot.dmg"),
  resolve(dmgDir, `Parrot_${version}_aarch64.dmg`),
];

await Promise.all(
  staleArtifacts.map((path) => rm(path, { force: true, recursive: true })),
);

console.log("Old release artifacts removed.");
