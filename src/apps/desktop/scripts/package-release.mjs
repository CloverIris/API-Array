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
const badgeVersion = encodeURIComponent(version.displayVersion);
const releaseNotes = [
  '<p align="center">',
  '  <img src="https://raw.githubusercontent.com/CloverIris/API-Array/main/src/apps/desktop/src-tauri/icons/app-icon.svg" width="180" alt="API ARRAY logo">',
  '</p>',
  '',
  `<h1 align="center">API ARRAY ${version.displayVersion}</h1>`,
  '',
  '<p align="center">',
  `  <img alt="Version" src="https://img.shields.io/badge/version-${badgeVersion}-2563eb?style=flat-square">`,
  '  <img alt="Channel" src="https://img.shields.io/badge/channel-Preview-7c3aed?style=flat-square">',
  '  <img alt="Windows" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078d4?style=flat-square&logo=windows11&logoColor=white">',
  '  <img alt="Architecture" src="https://img.shields.io/badge/architecture-x64-64748b?style=flat-square">',
  '  <img alt="License" src="https://img.shields.io/badge/license-MIT-22c55e?style=flat-square">',
  '</p>',
  '',
  '> API ARRAY 是一个本地优先的 API 钱包、审计网关与可视化编组工具。这个 Preview 版本面向希望集中管理多个 AI API，并通过统一 localhost 端点安全调用的 Windows 用户。',
  '',
  '## 本版本包含',
  '',
  '- **API 钱包**：管理 OpenAI、Anthropic、Gemini 与 Custom OpenAI-compatible API，Key 保存于 Windows Credential Manager。',
  '- **审计直出**：从单个钱包资产创建独立本地端点、Token、模型映射和脱敏审计。',
  '- **编组模式**：通过项目、文件夹和编组方案组合多个上游，并发布统一 OpenAI-compatible API。',
  '- **Canonical Graph V2**：Provider、Composer、Middleware、Probe 与 Publisher 形成可验证的服务计划。',
  '- **主控台**：集中查看并启停 Direct Endpoint 与编组方案实例。',
  '- **SQLite 工作区**：持久化配置、通知、报告和脱敏运行记录，并支持备份、验证与迁移。',
  '- **接口文档**：生成 Markdown 与 cURL、Python、JavaScript/TypeScript、Go、Rust、Java、C#、C++ 示例。',
  '- **Windows 桌面集成**：Mica/Acrylic、托盘常驻、开机启动、Windows Hello 与 WebView2。',
  '',
  '## 安装',
  '',
  `1. 下载 \`API_ARRAY_${safeVersion}_x64_zh-CN.msi\`。`,
  '2. 使用同目录 `.sha256` 文件校验安装包。',
  '3. 运行 MSI，并按照 OOBE 创建或打开本地工作区。',
  '',
  '## 安全说明',
  '',
  '- LocalGateway 默认监听 `127.0.0.1:7480`，Preview 不允许公网绑定。',
  '- API Key 与本地 Token 不写入 SQLite、日志、通知或导出。',
  '- 默认审计不保存请求正文、响应正文、URL、Header 或凭据。',
  '- 本安装包不包含用户 API Key、Token 或工作区数据。',
  '',
  '## Preview 已知限制',
  '',
  '- 仅支持 Windows 10/11 x64。',
  '- 对外出口仅支持 OpenAI-compatible 协议。',
  '- 暂不支持公网暴露、团队权限、云同步、Docker 服务端和自动升级。',
  '- 当前 MSI 未进行代码签名，Windows SmartScreen 可能显示“未知发布者”。',
  '',
  '## 校验与反馈',
  '',
  '- SHA-256：见随安装包生成的 `.sha256` 文件。',
  '- 项目文档：[中文 README](https://github.com/CloverIris/API-Array#readme) · [English README](https://github.com/CloverIris/API-Array/blob/main/README_EN.md)',
  '- 问题反馈：[GitHub Issues](https://github.com/CloverIris/API-Array/issues)',
  '',
  '感谢所有参与测试、反馈问题和维护开源依赖的开发者。',
  '',
].join('\n');
await writeFile(resolve(releaseRoot, "RELEASE_NOTES.md"), releaseNotes, "utf8");
console.log(`Release artifact: ${destination}`);
console.log(`SHA-256: ${digest}`);
console.log(`Repository root: ${repositoryRoot}`);
