import { spawnSync } from "node:child_process";
import { delimiter, resolve } from "node:path";
import { resolveWixBin } from "./wix-path.mjs";

const wixBin = resolveWixBin();
if (!wixBin) {
  console.error("未找到 WiX Toolset v3。请先运行 npm.cmd run release:preflight。");
  process.exit(1);
}

const desktopRoot = resolve(import.meta.dirname, "..");
const buildEnvironment = {
  ...process.env,
  APIARRAY_RELEASE_BUILD: "1",
  PATH: `${wixBin}${delimiter}${process.env.PATH ?? ""}`,
};

const frontend = spawnSync(
  process.execPath,
  ["./node_modules/next/dist/bin/next", "build"],
  { cwd: desktopRoot, env: buildEnvironment, stdio: "inherit" },
);
if (frontend.status !== 0) process.exit(frontend.status ?? 1);

const result = spawnSync(
  process.execPath,
  [
    "./node_modules/@tauri-apps/cli/tauri.js",
    "build",
    "--bundles",
    "msi",
    "--config",
    "./src-tauri/tauri.release.conf.json",
  ],
  {
    cwd: desktopRoot,
    env: buildEnvironment,
    stdio: "inherit",
  },
);
process.exit(result.status ?? 1);
