import { existsSync } from "node:fs";
import { delimiter, join } from "node:path";

const executableNames = ["candle.exe", "light.exe"];

function isWixV3Bin(directory) {
  return executableNames.every((name) => existsSync(join(directory, name)));
}

export function resolveWixBin(environment = process.env) {
  const fromEnvironment = environment.WIX_BIN;
  if (fromEnvironment && isWixV3Bin(fromEnvironment)) return fromEnvironment;

  const fromPath = (environment.PATH ?? "")
    .split(delimiter)
    .filter(Boolean)
    .find(isWixV3Bin);
  if (fromPath) return fromPath;

  const programFilesX86 = environment["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)";
  const programFiles = environment.ProgramFiles ?? "C:\\Program Files";
  const candidates = [
    join(programFilesX86, "WiX Toolset v3.14", "bin"),
    join(programFilesX86, "WiX Toolset v3.11", "bin"),
    join(programFilesX86, "WiX Toolset v3.10", "bin"),
    join(programFiles, "WiX Toolset v3.14", "bin"),
  ];
  return candidates.find(isWixV3Bin) ?? null;
}
