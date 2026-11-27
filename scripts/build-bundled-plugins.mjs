#!/usr/bin/env node

// Build the five bundled plugin cdylibs as cargo root targets, with the
// `dynamic` feature enabled, so `src-tauri/build.rs` can stage them into
// `src-tauri/resources/bundled-plugins/` for the app bundle.
//
// Why a separate step: when the app crate is built, the plugin crates are
// compiled as dependencies, and cargo places a dependency crate's cdylib
// under `target/<profile>/deps/` with a metadata hash in the file name
// (`libgumballz_get_plugin_convert-<hash>.so`). Only crates selected as
// root targets on the cargo command line get their final artifact linked
// to the top level without a hash
// (`target/<profile>/gumballz_get_plugin_convert.dll` on Windows,
// `libgumballz_get_plugin_convert.so` / `.dylib` elsewhere). A build
// script cannot shell out to a nested cargo (cargo holds the target-dir
// lock while the build script runs), so this step runs BEFORE the main
// cargo invocation, from Tauri's `beforeDevCommand` / `beforeBuildCommand`
// hooks (see `src-tauri/tauri.conf.json`), where no lock is held yet. The
// hook command chains this step before the long-running Vite server, so
// the cdylibs are already built by the time cargo starts.
//
// Usage:
//   node scripts/build-bundled-plugins.mjs [--release] [--target <triple>]
// The `--release` form builds release cdylibs for `tauri build`; the
// debug form is for `tauri dev`. When run from a Tauri hook, the target
// triple is taken from `TAURI_ENV_TARGET_TRIPLE` (set by the Tauri CLI)
// when it differs from the rustc host triple, so cross-target builds (for
// example the macOS release job) stage artifacts for the right target.
// When the hook triple equals the host triple, no `--target` is passed:
// the main cargo invocation then also builds without it, and if it was
// given an explicit `--target` naming the host triple anyway, `build.rs`
// falls back to the host profile dir (same architecture, only the
// directory differs).
// Set `GUMBALLZ_SKIP_BUNDLED_PLUGINS_BUILD=1` to skip this step.
//
// The step fails loudly (non-zero exit) when cargo fails, when a plugin
// crate does not define the `dynamic` feature, or when an expected
// artifact is missing after the build, so a broken bundle is never
// shipped silently. It never edits `Cargo.toml` or the plugin sources.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const PLUGIN_PACKAGES = [
  { id: "convert", package: "gumballz-get-plugin-convert" },
  { id: "telegram", package: "gumballz-get-plugin-telegram" },
  { id: "courses", package: "gumballz-get-plugin-courses" },
  { id: "listening-studio", package: "gumballz-get-plugin-listening-studio" },
  { id: "asset-forge", package: "gumballz-get-plugin-asset-forge" },
];

const SKIP_ENV_VAR = "GUMBALLZ_SKIP_BUNDLED_PLUGINS_BUILD";

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const srcTauriDir = path.join(rootDir, "src-tauri");

function fail(message) {
  console.error(`[bundled-plugins] ERROR: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const options = { release: false, target: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--release") {
      options.release = true;
    } else if (arg === "--target") {
      const value = argv[i + 1];
      if (!value || value.startsWith("--")) {
        fail("--target requires a target triple argument.");
      }
      options.target = value;
      i += 1;
    } else if (arg.startsWith("--target=")) {
      options.target = arg.slice("--target=".length);
      if (!options.target) {
        fail("--target requires a target triple argument.");
      }
    } else {
      fail(
        `Unknown argument: ${arg}. Usage: node scripts/build-bundled-plugins.mjs [--release] [--target <triple>]`,
      );
    }
  }
  return options;
}

// The `dynamic` feature gates the `export_plugin!` symbols in each plugin
// crate. A cdylib built without it would still produce a file, but without
// the symbols the loader needs, so check the manifests up front instead
// of shipping a symbol-less library.
function checkDynamicFeature(plugin) {
  const manifestPath = path.join(
    srcTauriDir,
    "plugins",
    plugin.id,
    "Cargo.toml",
  );
  let text;
  try {
    text = fs.readFileSync(manifestPath, "utf8");
  } catch {
    fail(`Cannot read ${manifestPath}.`);
  }
  const lines = text.split("\n");
  let inFeatures = false;
  for (const line of lines) {
    if (/^\[features\]/.test(line)) {
      inFeatures = true;
      continue;
    }
    if (/^\[/.test(line)) {
      inFeatures = false;
    }
    if (inFeatures && /^\s*dynamic\s*=/.test(line)) {
      return;
    }
  }
  fail(
    `Plugin crate ${plugin.package} does not define a \`dynamic\` feature in ${manifestPath}.`,
  );
}

function rustcHostTriple() {
  const result = spawnSync("rustc", ["-vV"], { encoding: "utf8" });
  if (result.error || result.status !== 0) {
    return null;
  }
  const match = /^host:\s*(\S+)/m.exec(result.stdout ?? "");
  return match ? match[1] : null;
}

// Cargo names a library file after the lib target name, which is the
// package name with hyphens replaced by underscores:
// `gumballz-get-plugin-convert` -> `gumballz_get_plugin_convert.dll`
// (Windows), `libgumballz_get_plugin_convert.so` (Linux),
// `libgumballz_get_plugin_convert.dylib` (macOS).
function cdylibFileName(plugin) {
  const libName = `gumballz_get_plugin_${plugin.id.replace(/-/g, "_")}`;
  if (process.platform === "win32") {
    return `${libName}.dll`;
  }
  if (process.platform === "darwin") {
    return `lib${libName}.dylib`;
  }
  return `lib${libName}.so`;
}

function main() {
  if (process.env[SKIP_ENV_VAR] === "1") {
    console.log(
      `[bundled-plugins] ${SKIP_ENV_VAR}=1, skipping bundled plugin build.`,
    );
    return;
  }

  const options = parseArgs(process.argv.slice(2));
  const profile = options.release ? "release" : "debug";

  for (const plugin of PLUGIN_PACKAGES) {
    checkDynamicFeature(plugin);
  }

  // Only pass `--target` when the build target differs from the host:
  // without `--target`, both this step and the main cargo invocation put
  // artifacts in `target/<profile>/`, while an explicit `--target` puts
  // them in `target/<triple>/<profile>/`. The Tauri CLI sets
  // `TAURI_ENV_TARGET_TRIPLE` to the host triple for a plain local build,
  // so passing it through blindly would split the two locations.
  let target = options.target;
  if (!target) {
    const hookTriple = process.env.TAURI_ENV_TARGET_TRIPLE;
    const hostTriple = rustcHostTriple();
    if (hookTriple && (hostTriple === null || hookTriple !== hostTriple)) {
      target = hookTriple;
    }
  }

  // Per-package `package/feature` syntax pins `dynamic` to exactly the
  // five plugin crates, independent of which package is in the current
  // directory.
  const cargoArgs = [
    "build",
    ...PLUGIN_PACKAGES.flatMap((plugin) => ["-p", plugin.package]),
    "--features",
    PLUGIN_PACKAGES.map((plugin) => `${plugin.package}/dynamic`).join(","),
  ];
  if (options.release) {
    cargoArgs.push("--release");
  }
  if (target) {
    cargoArgs.push("--target", target);
  }

  console.log(
    `[bundled-plugins] Building 5 plugin cdylibs (${profile}${target ? `, target ${target}` : ""})...`,
  );
  const result = spawnSync("cargo", cargoArgs, {
    cwd: srcTauriDir,
    stdio: "inherit",
    // On Windows, `cargo` resolves through the `cargo.cmd` shim, which
    // needs a shell (same convention as scripts/deploy-plugins-local.mjs).
    shell: process.platform === "win32",
  });
  if (result.error) {
    fail(`Failed to run cargo: ${result.error.message}`);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }

  // Verify every artifact landed where `src-tauri/build.rs` looks for it.
  // Cargo writes to `CARGO_TARGET_DIR` when set, otherwise
  // `src-tauri/target/`.
  const targetDir = process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(srcTauriDir, "target");
  const profileDir = target
    ? path.join(targetDir, target, profile)
    : path.join(targetDir, profile);
  const missing = [];
  for (const plugin of PLUGIN_PACKAGES) {
    const artifact = path.join(profileDir, cdylibFileName(plugin));
    if (!fs.existsSync(artifact)) {
      missing.push(artifact);
    }
  }
  if (missing.length > 0) {
    fail(
      `Cargo succeeded but ${missing.length} plugin artifact(s) are missing:\n${missing.map((p) => `  - ${p}`).join("\n")}`,
    );
  }

  console.log(
    `[bundled-plugins] Built 5 plugin cdylibs (${profile}${target ? `, target ${target}` : ""}).`,
  );
}

main();
