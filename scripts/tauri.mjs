// `pnpm tauri dev` runs the plugin pre-build (scripts/build-bundled-plugins.mjs)
// before vite starts, and a cold plugin build takes longer than the 180 s the
// Tauri CLI waits for the dev server, so `tauri dev` used to die with
// "Could not connect to http://localhost:1420/ after 180s" even though the
// plugin build itself had succeeded. Skip that wait: the app build blocks on
// the cargo target lock until the plugin build finishes, and vite is up long
// before the app window opens.
import { spawnSync } from "node:child_process";

process.env.TAURI_CLI_NO_DEV_SERVER_WAIT = "true";
const result = spawnSync("tauri", process.argv.slice(2), {
  stdio: "inherit",
  shell: true,
});
if (result.error) {
  console.error(`[tauri] failed to start: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
