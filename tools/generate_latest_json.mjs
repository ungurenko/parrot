import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const repo = process.env.PARROT_RELEASE_REPO ?? "ungurenko/parrot";
const root = resolve(import.meta.dirname, "..");
const tauriConfigPath = resolve(root, "src-tauri/tauri.conf.json");
const bundleDir = resolve(root, "src-tauri/target/release/bundle/macos");
const signaturePath = resolve(bundleDir, "Parrot.app.tar.gz.sig");
const outputPath = resolve(bundleDir, "latest.json");

const tauriConfig = JSON.parse(await readFile(tauriConfigPath, "utf8"));
const version = tauriConfig.version;
const signature = (await readFile(signaturePath, "utf8")).trim();

const latest = {
  version,
  notes: "Новая версия Parrot.",
  pub_date: new Date().toISOString(),
  platforms: {
    "darwin-aarch64": {
      signature,
      url: `https://github.com/${repo}/releases/latest/download/Parrot.app.tar.gz`,
    },
  },
};

await writeFile(outputPath, `${JSON.stringify(latest, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);
