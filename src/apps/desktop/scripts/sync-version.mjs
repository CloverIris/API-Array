import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const desktopRoot = resolve(import.meta.dirname, "..");
const sourceRoot = resolve(desktopRoot, "..", "..");
const version = JSON.parse(await readFile(resolve(sourceRoot, "product-version.json"), "utf8"));
const files = [
  resolve(sourceRoot, "Cargo.toml"),
  resolve(desktopRoot, "package.json"),
  resolve(desktopRoot, "src-tauri", "tauri.conf.json"),
];

const replaceOne = (text, matcher, replacement, file) => {
  if (!matcher.test(text)) throw new Error(`无法在 ${file} 找到版本字段`);
  return text.replace(matcher, replacement);
};

let cargo = await readFile(files[0], "utf8");
cargo = replaceOne(cargo, /version = "[^"]+"/, `version = "${version.version}"`, files[0]);
const previousCargo = await readFile(files[0], "utf8");
if (cargo !== previousCargo) await writeFile(files[0], cargo);

for (const file of files.slice(1)) {
  const payload = JSON.parse(await readFile(file, "utf8"));
  payload.version = version.version;
  const next = `${JSON.stringify(payload, null, 2)}\n`;
  if (next !== await readFile(file, "utf8")) await writeFile(file, next);
}

console.log(`API ARRAY version synchronized: ${version.displayVersion}`);
