import { copyFile, readFile, stat, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const repo = process.env.PARROT_RELEASE_REPO ?? "ungurenko/parrot";
const root = resolve(import.meta.dirname, "..");
const tauriConfigPath = resolve(root, "src-tauri/tauri.conf.json");
const dmgDir = resolve(root, "src-tauri/target/release/bundle/dmg");
const bundleDir = resolve(root, "src-tauri/target/release/bundle/macos");
const signaturePath = resolve(bundleDir, "Parrot.app.tar.gz.sig");
const archivePath = resolve(bundleDir, "Parrot.app.tar.gz");
const outputPath = resolve(bundleDir, "latest.json");

const tauriConfig = JSON.parse(await readFile(tauriConfigPath, "utf8"));
const version = tauriConfig.version;
const signature = (await readFile(signaturePath, "utf8")).trim();
const versionedDmgPath = resolve(dmgDir, `Parrot_${version}_aarch64.dmg`);
const latestDmgPath = resolve(dmgDir, "Parrot.dmg");
const [archiveStat, signatureStat] = await Promise.all([
  stat(archivePath),
  stat(signaturePath),
]);

if (signatureStat.mtimeMs + 1000 < archiveStat.mtimeMs) {
  throw new Error("Signature is older than Parrot.app.tar.gz. Run npm run release:mac again.");
}

const latest = {
  version,
  notes: "Исправлена транскрибация YouTube-ссылок (ошибка подписи yt-dlp под hardened runtime).",
  pub_date: new Date().toISOString(),
  platforms: {
    "darwin-aarch64": {
      signature,
      url: `https://github.com/${repo}/releases/latest/download/Parrot.app.tar.gz`,
    },
  },
};

await writeFile(outputPath, `${JSON.stringify(latest, null, 2)}\n`);
await copyFile(versionedDmgPath, latestDmgPath);
console.log(`Wrote ${outputPath}`);
console.log(`Wrote ${latestDmgPath}`);
