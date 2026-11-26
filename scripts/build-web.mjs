import { spawnSync } from "node:child_process";

const result = spawnSync(
  process.execPath,
  ["--max-old-space-size=4096", "./node_modules/vite/bin/vite.js", "build"],
  {
    cwd: process.cwd(),
    env: { ...process.env, VITE_TARGET: "web" },
    stdio: "inherit",
  },
);

if (result.error) throw result.error;
process.exit(result.status ?? 1);
