import { copyFile, mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";

const desktopRoot = resolve(import.meta.dirname, "..");
const sourceRoot = resolve(desktopRoot, "..", "..");
const repositoryRoot = resolve(sourceRoot, "..");
const version = JSON.parse(await readFile(resolve(sourceRoot, "product-version.json"), "utf8"));
const bundleRoot = resolve(sourceRoot, "target", "release", "bundle", "msi");
const releaseRoot = resolve(sourceRoot, "release");

async function findMsi(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const candidates = await Promise.all(entries.map(async (entry) => {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) return findMsi(path);
    return entry.isFile() && entry.name.toLowerCase().endsWith(".msi") ? [path] : [];
  }));
  return candidates.flat();
}

const installers = await findMsi(bundleRoot);
if (!installers.length) throw new Error("未找到 Tauri 生成的 MSI；请先完成 release:msi 构建。");
const installer = (await Promise.all(installers.map(async (path) => ({ path, stat: await stat(path) })))).sort((left, right) => right.stat.mtimeMs - left.stat.mtimeMs)[0].path;
const safeVersion = `${version.version}_${version.channel}`.replaceAll(/[^a-zA-Z0-9._-]/g, "_");
const destination = resolve(releaseRoot, `API_ARRAY_${safeVersion}_x64_zh-CN.msi`);
await mkdir(releaseRoot, { recursive: true });
await copyFile(installer, destination);
const bytes = await readFile(destination);
const digest = createHash("sha256").update(bytes).digest("hex");
await writeFile(`${destination}.sha256`, `${digest}  ${destination.split(/[\\/]/).at(-1)}\n`, "utf8");
await writeFile(resolve(releaseRoot, "RELEASE_NOTES.md"), `# API ARRAY ${version.displayVersion}\n\n- Windows x64、每用户 MSI 安装包。\n- 内嵌 WebView2 引导程序；首次缺失运行时仍需要网络。\n- 本 Preview 未进行代码签名，Windows SmartScreen 可能显示警告。\n- 本地 API Key 与 Token 不包含在安装包、日志或导出中。\n\nSHA-256：见同目录 \`.sha256\` 文件。\n`, "utf8");
console.log(`Release artifact: ${destination}`);
console.log(`SHA-256: ${digest}`);
console.log(`Repository root: ${repositoryRoot}`);
