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
fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2));

// 2. src-tauri/tauri.conf.json
const tauriConfPath = path.join(process.cwd(), "src-tauri/tauri.conf.json");
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, "utf-8"));
tauriConf.version = newVersion;
fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2));

// 3. src-tauri/Cargo.toml
const cargoPath = path.join(process.cwd(), "src-tauri/Cargo.toml");
let cargoContent = fs.readFileSync(cargoPath, "utf-8");
cargoContent = cargoContent.replace(
  /^version\s*=\s*".*"/m,
  `version = "${newVersion}"`
);
fs.writeFileSync(cargoPath, cargoContent);

console.log("版本号已更新为", newVersion);
