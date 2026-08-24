import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");

async function readJson(path) {
  return JSON.parse(await readFile(resolve(root, path), "utf8"));
}

function cargoPackageVersion(contents) {
  const packageBlock = contents.match(/\[package\]([\s\S]*?)(?:\n\[|$)/)?.[1];
  return packageBlock?.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? null;
}

function cargoLockPackageVersion(contents) {
  const packages = contents.split("[[package]]");
  const parrot = packages.find((block) => /^\s*name\s*=\s*"parrot"\s*$/m.test(block));
  return parrot?.match(/^\s*version\s*=\s*"([^"]+)"\s*$/m)?.[1] ?? null;
}

const [packageJson, packageLock, tauriConfig, cargoToml, cargoLock] =
  await Promise.all([
    readJson("package.json"),
    readJson("package-lock.json"),
    readJson("src-tauri/tauri.conf.json"),
    readFile(resolve(root, "src-tauri/Cargo.toml"), "utf8"),
    readFile(resolve(root, "src-tauri/Cargo.lock"), "utf8"),
  ]);

const versions = new Map([
  ["package.json", packageJson.version],
  ["package-lock.json", packageLock.version],
  ["package-lock.json packages root", packageLock.packages?.[""]?.version],
  ["src-tauri/Cargo.toml", cargoPackageVersion(cargoToml)],
  ["src-tauri/Cargo.lock", cargoLockPackageVersion(cargoLock)],
  ["src-tauri/tauri.conf.json", tauriConfig.version],
]);

const expected = packageJson.version;
const mismatches = [...versions].filter(([, version]) => version !== expected);

if (!expected || mismatches.length > 0) {
  const details = [...versions]
    .map(([file, version]) => `${file}: ${version ?? "missing"}`)
    .join("\n");
  throw new Error(`Release versions do not match:\n${details}`);
}

console.log(`Release versions match: ${expected}`);
