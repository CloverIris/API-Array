import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { execFileSync } from "node:child_process";

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
try {
  const status = execFileSync("git", ["-c", "safe.directory=E:/APIArray", "status", "--porcelain"], { cwd: resolve(sourceRoot, ".."), encoding: "utf8" }).trim();
  if (status) errors.push("工作树包含未提交改动；发布必须从已审查提交构建。");
} catch { errors.push("无法读取 Git 发布状态。"); }
if (errors.length) {
  console.error(`API ARRAY ${expected.displayVersion} 发布校验失败：\n- ${errors.join("\n- ")}`);
  process.exit(1);
}
console.log(`API ARRAY ${expected.displayVersion} 发布校验通过。`);
