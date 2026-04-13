// update-version.js
import fs from "fs";
import path from "path";

const newVersion = process.argv[2];
if (!newVersion) {
  console.error("请指定新版本号，例如: node update-version.js 0.1.2");
  process.exit(1);
}

// 1. package.json
const pkgPath = path.join(process.cwd(), "package.json");
const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
pkg.version = newVersion;

// Keep Tauri JS packages aligned (major/minor) with the Rust side used in src-tauri.
// NOTE: These pins are required because tauri-cli enforces minor-version matching.
if (pkg.dependencies) {
  if (pkg.dependencies["@tauri-apps/api"])
    pkg.dependencies["@tauri-apps/api"] = "^2.8.0";
  if (pkg.dependencies["@tauri-apps/plugin-opener"])
    pkg.dependencies["@tauri-apps/plugin-opener"] = "2.4.0";
  if (pkg.dependencies["@tauri-apps/plugin-store"])
    pkg.dependencies["@tauri-apps/plugin-store"] = "2.3.0";
}
if (pkg.devDependencies) {
  if (pkg.devDependencies["@tauri-apps/cli"])
    pkg.devDependencies["@tauri-apps/cli"] = "^2.8.0";
}

fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2));

// 2. src-tauri/tauri.conf.json
const tauriConfPath = path.join(process.cwd(), "src-tauri/tauri.conf.json");
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, "utf-8"));
tauriConf.version = newVersion;
fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2));

// 3. package-lock.json（同步项目版本号，不影响依赖）
const pkgLockPath = path.join(process.cwd(), "package-lock.json");
if (fs.existsSync(pkgLockPath)) {
  const pkgLock = JSON.parse(fs.readFileSync(pkgLockPath, "utf-8"));
  pkgLock.version = newVersion;
  if (pkgLock.packages?.[""]?.version !== undefined) {
    pkgLock.packages[""].version = newVersion;
  }
  fs.writeFileSync(pkgLockPath, JSON.stringify(pkgLock, null, 2));
}

// 5. src-tauri/Cargo.toml
const cargoPath = path.join(process.cwd(), "src-tauri/Cargo.toml");
let cargoContent = fs.readFileSync(cargoPath, "utf-8");
cargoContent = cargoContent.replace(
  /^version\s*=\s*".*"/m,
  `version = "${newVersion}"`
);
fs.writeFileSync(cargoPath, cargoContent);

console.log("版本号已更新为", newVersion);
