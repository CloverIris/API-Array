import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const desktopRoot = resolve(import.meta.dirname, "..");
const sourceRoot = resolve(desktopRoot, "..", "..");
const expected = JSON.parse(await readFile(resolve(sourceRoot, "product-version.json"), "utf8"));
const cargo = await readFile(resolve(sourceRoot, "Cargo.toml"), "utf8");
const packageJson = JSON.parse(await readFile(resolve(desktopRoot, "package.json"), "utf8"));
const tauri = JSON.parse(await readFile(resolve(desktopRoot, "src-tauri", "tauri.conf.json"), "utf8"));
const errors = [];
if (!cargo.includes(`version = "${expected.version}"`)) errors.push("Cargo workspace version 不匹配。");
if (packageJson.version !== expected.version) errors.push("desktop package version 不匹配。");
if (tauri.version !== expected.version) errors.push("Tauri version 不匹配。");
if (!tauri.bundle?.active || tauri.bundle?.targets !== "msi") errors.push("Tauri MSI 打包未启用。");
if (tauri.bundle?.windows?.webviewInstallMode?.type !== "embedBootstrapper") errors.push("WebView2 必须使用 embedBootstrapper。");
if (errors.length) {
  console.error(`API ARRAY ${expected.displayVersion} 发布校验失败：\n- ${errors.join("\n- ")}`);
  process.exit(1);
}
console.log(`API ARRAY ${expected.displayVersion} 发布校验通过。`);
