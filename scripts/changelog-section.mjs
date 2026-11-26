#!/usr/bin/env node
// Print one version's section from CHANGELOG.md (used for release bodies).
// Usage: node scripts/changelog-section.mjs 1.0.0
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];
if (!version) {
  console.error("Usage: node scripts/changelog-section.mjs <version>");
  process.exit(1);
}
const text = fs.readFileSync(path.join(root, "CHANGELOG.md"), "utf8");
const lines = text.split("\n");
const start = lines.findIndex((l) => l.startsWith(`## [${version}]`));
if (start === -1) {
  console.error(`No section for version ${version} in CHANGELOG.md`);
  process.exit(1);
}
const out = [];
for (let i = start; i < lines.length; i++) {
  if (i > start && lines[i].startsWith("## [")) break;
  out.push(lines[i]);
}
console.log(out.join("\n").trimEnd());
