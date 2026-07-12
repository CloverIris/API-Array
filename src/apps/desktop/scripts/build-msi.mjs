import { spawnSync } from "node:child_process";
import { delimiter, resolve } from "node:path";
import { resolveWixBin } from "./wix-path.mjs";

const wixBin = resolveWixBin();
if (!wixBin) {
  console.error("未找到 WiX Toolset v3。请先运行 npm.cmd run release:preflight。");
  process.exit(1);
}

const desktopRoot = resolve(import.meta.dirname, "..");
const result = spawnSync(
  process.execPath,
  ["./node_modules/@tauri-apps/cli/tauri.js", "build", "--bundles", "msi"],
  {
    cwd: desktopRoot,
    env: { ...process.env, PATH: `${wixBin}${delimiter}${process.env.PATH ?? ""}` },
    stdio: "inherit",
  },
);
process.exit(result.status ?? 1);
