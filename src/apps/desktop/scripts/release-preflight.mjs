import { access, readFile, stat } from "node:fs/promises";
import { delimiter, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { resolveWixBin } from "./wix-path.mjs";

const desktopRoot = resolve(import.meta.dirname, "..");
const sourceRoot = resolve(desktopRoot, "..", "..");
const version = JSON.parse(await readFile(resolve(sourceRoot, "product-version.json"), "utf8"));
const icons = ["icon.ico", "icon.png", "app-icon.svg"].map((name) => resolve(desktopRoot, "src-tauri", "icons", name));
const failures = [];

for (const icon of icons) {
  try {
    await access(icon);
  } catch {
    failures.push(`缺少图标资源：${icon}`);
  }
}

try {
  console.log(execFileSync("cargo", ["--version"], { encoding: "utf8" }).trim());
} catch {
  failures.push("未找到 Cargo/MSVC 构建环境。");
}

const wixBin = resolveWixBin();
if (wixBin) {
  console.log(`WiX v3: ${wixBin}`);
  if (!process.env.PATH?.split(delimiter).includes(wixBin)) {
    console.log("提示：WiX 未加入 PATH；发布脚本会临时使用该安装目录，不会修改系统环境变量。");
  }
} else {
  failures.push("未找到 WiX Toolset v3（candle.exe 与 light.exe）。请安装 WiX v3 后重新运行预检。");
}

try {
  const sourceDirectory = await stat(sourceRoot);
  if (!sourceDirectory.isDirectory()) failures.push("源代码目录不可访问。");
} catch {
  failures.push("源代码目录不可访问。");
}

if (failures.length) {
  console.error(`API ARRAY ${version.displayVersion} 发布预检失败：\n- ${failures.join("\n- ")}`);
  process.exit(1);
}

console.log(`API ARRAY ${version.displayVersion} 发布预检通过。请确认 Windows 的 VBSCRIPT 可选功能已启用。`);
