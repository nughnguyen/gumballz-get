import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const extensionDir = path.join(root, "browser-extension", "chrome");
const outputDir = path.join(root, "dist");
const outputZip = path.join(outputDir, "gumballz-get-chrome-extension-v1.0.0.zip");
const relative = path.relative(outputDir, outputZip);
if (relative.startsWith("..") || path.isAbsolute(relative)) throw new Error("Refusing to write outside dist/");

const sync = spawnSync(process.execPath, [path.join(root, "browser-extension", "sync.mjs")], { cwd: root, stdio: "inherit" });
if (sync.status !== 0) process.exit(sync.status ?? 1);
fs.mkdirSync(outputDir, { recursive: true });
if (fs.existsSync(outputZip)) fs.unlinkSync(outputZip);

let result;
if (process.platform === "win32") {
  const quote = (value) => `'${value.replaceAll("'", "''")}'`;
  const command = `Compress-Archive -Path ${quote(path.join(extensionDir, "*"))} -DestinationPath ${quote(outputZip)} -Force`;
  result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", command], { cwd: root, stdio: "inherit" });
} else {
  result = spawnSync("zip", ["-qr", outputZip, "."], { cwd: extensionDir, stdio: "inherit" });
}
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
console.log(`Created ${path.relative(root, outputZip)}`);
