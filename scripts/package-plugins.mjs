#!/usr/bin/env node

// Package the five official plugins as downloadable `.zip` files for the
// rolling `plugins` GitHub release (tag `plugins` on this repo), so the
// Tools page can install them with a real network download instead of
// shipping them inside the app bundle.
//
// What it does, per plugin:
//   1. Builds the plugin cdylib in RELEASE mode as a cargo root target
//      with the `dynamic` feature enabled (the `export_plugin!` symbols
//      the loader needs — the same gating that fixed the LNK2005 link
//      error; the app itself still links the plugin crates as
//      dependencies WITHOUT `dynamic`, so no duplicate symbols).
//   2. Zips the cdylib + its `plugin.json` into
//      `dist/plugins/<id>-<platform>.zip` — e.g.
//      `convert-windows-x86_64.zip`.
//   3. Updates `plugins.json` at the repo root with the plugin version
//      and the new `artifacts[platform]` entry (download URL, SHA-256,
//      size) for the platform just built, preserving every other
//      platform's entry and every other plugin's entry.
//   4. Prints the `gh` commands that upload the zips to the `plugins`
//      release (`--upload` runs them instead of printing).
//
// Outputs live under `dist/` (gitignored) at the repo root — never inside
// `src-tauri/`, which `tauri dev` watches: the old staging step wrote
// into `src-tauri/resources/bundled-plugins/` on every build and sent the
// dev watcher into an infinite rebuild loop. Nothing here runs during a
// dev/build at all; run it by hand when a plugin changes.
//
// Usage:
//   node scripts/package-plugins.mjs [--target <triple>] [--upload] [--no-registry]
// `--target` cross-packages for another triple (its platform suffix is
// derived from the triple, e.g. `aarch64-apple-darwin` -> `macos-aarch64`).
// `--upload` creates the `plugins` release if missing and uploads the
// zips with `--clobber` (needs `gh` logged in). `--no-registry` skips
// updating `plugins.json` (the SHA-256 + size are still printed).

import { spawnSync } from "node:child_process";
import crypto from "node:crypto";
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

const RELEASE_TAG = "plugins";
const REPO = "nughnguyen/gumballz-get";
const REGISTRY_PATH = "plugins.json";

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const srcTauriDir = path.join(rootDir, "src-tauri");
const outDir = path.join(rootDir, "dist", "plugins");

function fail(message) {
  console.error(`[package-plugins] ERROR: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const options = { target: null, upload: false, updateRegistry: true };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--target") {
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
    } else if (arg === "--upload") {
      options.upload = true;
    } else if (arg === "--no-registry") {
      options.updateRegistry = false;
    } else {
      fail(
        `Unknown argument: ${arg}. Usage: node scripts/package-plugins.mjs [--target <triple>] [--upload] [--no-registry]`,
      );
    }
  }
  return options;
}

// Platform suffix for a Rust target triple (`x86_64-pc-windows-msvc` ->
// `windows-x86_64`). The registry keys artifacts by this suffix, and the
// app's installer picks the entry matching its own platform.
function suffixForTriple(triple) {
  if (/windows/.test(triple)) return "windows-x86_64";
  if (/apple-darwin/.test(triple)) {
    return triple.startsWith("aarch64") ? "macos-aarch64" : "macos-x86_64";
  }
  if (/linux/.test(triple)) return "linux-x86_64";
  return null;
}

function rustcHostTriple() {
  const result = spawnSync("rustc", ["-vV"], { encoding: "utf8" });
  if (result.error || result.status !== 0) {
    return null;
  }
  const match = /^host:\s*(\S+)/m.exec(result.stdout ?? "");
  return match ? match[1] : null;
}

// The `dynamic` feature gates the `export_plugin!` symbols in each plugin
// crate. A cdylib built without it would still produce a file, but without
// the symbols the loader needs, so check the manifests up front instead
// of shipping a symbol-less library.
function checkDynamicFeature(plugin) {
  const manifestPath = path.join(srcTauriDir, "plugins", plugin.id, "Cargo.toml");
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

// Cargo names a library file after the lib target name, which is the
// package name with hyphens replaced by underscores:
// `gumballz-get-plugin-convert` -> `gumballz_get_plugin_convert.dll`
// (Windows), `libgumballz_get_plugin_convert.so` (Linux),
// `libgumballz_get_plugin_convert.dylib` (macOS).
function cdylibFileName(plugin, platform) {
  const libName = `gumballz_get_plugin_${plugin.id.replace(/-/g, "_")}`;
  if (platform === "win32") {
    return `${libName}.dll`;
  }
  if (platform === "darwin") {
    return `lib${libName}.dylib`;
  }
  return `lib${libName}.so`;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    shell: process.platform === "win32",
    ...options,
  });
  return result;
}

function sha256Of(filePath) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
}

function main() {
  const options = parseArgs(process.argv.slice(2));

  const hostTriple = rustcHostTriple();
  const buildTriple = options.target ?? hostTriple;
  if (!buildTriple) {
    fail("Cannot determine the target triple (rustc not found?) — pass --target <triple>.");
  }
  const suffix = suffixForTriple(buildTriple);
  if (!suffix) {
    fail(`Unsupported target triple: ${buildTriple}.`);
  }

  for (const plugin of PLUGIN_PACKAGES) {
    checkDynamicFeature(plugin);
  }

  // Per-package `package/feature` syntax pins `dynamic` to exactly the
  // five plugin crates, independent of which package is in the current
  // directory. Always a release build: these zips are what users download
  // and install.
  const cargoArgs = [
    "build",
    ...PLUGIN_PACKAGES.flatMap((plugin) => ["-p", plugin.package]),
    "--features",
    PLUGIN_PACKAGES.map((plugin) => `${plugin.package}/dynamic`).join(","),
    "--release",
  ];
  if (options.target) {
    cargoArgs.push("--target", options.target);
  }

  console.log(
    `[package-plugins] Building 5 plugin cdylibs (release, target ${buildTriple}, ${suffix})...`,
  );
  const build = run("cargo", cargoArgs, { cwd: srcTauriDir, stdio: "inherit" });
  if (build.error) {
    fail(`Failed to run cargo: ${build.error.message}`);
  }
  if (build.status !== 0) {
    process.exit(build.status ?? 1);
  }

  // Cargo writes to `CARGO_TARGET_DIR` when set, otherwise
  // `src-tauri/target/`.
  const targetDir = process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(srcTauriDir, "target");
  const profileDir = options.target
    ? path.join(targetDir, options.target, "release")
    : path.join(targetDir, "release");
  const missing = [];
  for (const plugin of PLUGIN_PACKAGES) {
    const artifact = path.join(profileDir, cdylibFileName(plugin, process.platform));
    if (!fs.existsSync(artifact)) {
      missing.push(artifact);
    }
  }
  if (missing.length > 0) {
    fail(
      `Cargo succeeded but ${missing.length} plugin artifact(s) are missing:\n${missing.map((p) => `  - ${p}`).join("\n")}`,
    );
  }

  fs.mkdirSync(outDir, { recursive: true });
  const built = [];
  for (const plugin of PLUGIN_PACKAGES) {
    const cdylibName = cdylibFileName(plugin, process.platform);
    const cdylibSrc = path.join(profileDir, cdylibName);
    const manifestSrc = path.join(srcTauriDir, "plugins", plugin.id, "plugin.json");
    if (!fs.existsSync(manifestSrc)) {
      fail(`plugin.json missing for '${plugin.id}' at ${manifestSrc}.`);
    }
    const zipName = `${plugin.id}-${suffix}.zip`;
    const zipPath = path.join(outDir, zipName);

    // Stage exactly the two payload files, then zip them at the zip root
    // (the installer requires `plugin.json` at the root of the zip and
    // exactly one native library in the installed dir).
    const staging = path.join(outDir, `.staging-${plugin.id}`);
    fs.rmSync(staging, { recursive: true, force: true });
    fs.mkdirSync(staging, { recursive: true });
    fs.copyFileSync(cdylibSrc, path.join(staging, cdylibName));
    fs.copyFileSync(manifestSrc, path.join(staging, "plugin.json"));

    // `tar -a` writes a zip when the output name ends in `.zip`
    // (ships with Windows 10+, macOS and Linux).
    const tar = run("tar", ["-a", "-c", "-f", zipPath, "-C", staging, cdylibName, "plugin.json"]);
    if (tar.error || tar.status !== 0) {
      fs.rmSync(staging, { recursive: true, force: true });
      fail(
        `Failed to create ${zipName} with tar${tar.error ? `: ${tar.error.message}` : ""}. ` +
          `On Windows, tar.exe ships with Windows 10+; otherwise install a zip tool and re-run.`,
      );
    }
    const listing = run("tar", ["-tf", zipPath]);
    const listed = listing.stdout ?? "";
    if (!listed.includes(cdylibName) || !listed.includes("plugin.json")) {
      fs.rmSync(staging, { recursive: true, force: true });
      fail(`${zipName} is missing the cdylib or plugin.json after zipping.`);
    }
    fs.rmSync(staging, { recursive: true, force: true });

    const size = fs.statSync(zipPath).size;
    const sha256 = sha256Of(zipPath);
    built.push({ ...plugin, zipName, zipPath, size, sha256 });
    console.log(`[package-plugins] ${zipName}: ${size} bytes, sha256 ${sha256}`);
  }

  if (options.updateRegistry) {
    updateRegistry(built, suffix);
  }

  const uploadCommands = [
    `gh release view ${RELEASE_TAG} || gh release create ${RELEASE_TAG} --title "Official plugins" --notes "Rolling release holding the official plugin .zip downloads for the Tools page."`,
    `gh release upload ${RELEASE_TAG} ${built.map((b) => `dist/plugins/${b.zipName}`).join(" ")} --clobber`,
  ];
  if (options.upload) {
    const gh = run("gh", ["--version"]);
    if (gh.error || gh.status !== 0) {
      fail("`gh` is not available — install the GitHub CLI and log in (`gh auth login`), then re-run with --upload.");
    }
    const view = run("gh", ["release", "view", RELEASE_TAG], { stdio: "inherit" });
    if (view.status !== 0) {
      console.log(`[package-plugins] Release '${RELEASE_TAG}' not found, creating it...`);
      const create = run(
        "gh",
        [
          "release",
          "create",
          RELEASE_TAG,
          "--title",
          "Official plugins",
          "--notes",
          "Rolling release holding the official plugin .zip downloads for the Tools page.",
        ],
        { stdio: "inherit" },
      );
      if (create.error || create.status !== 0) {
        fail(`Failed to create the '${RELEASE_TAG}' release.`);
      }
    }
    const upload = run(
      "gh",
      [
        "release",
        "upload",
        RELEASE_TAG,
        ...built.map((b) => b.zipPath),
        "--clobber",
      ],
      { stdio: "inherit" },
    );
    if (upload.error || upload.status !== 0) {
      fail(`Failed to upload the plugin zips to the '${RELEASE_TAG}' release.`);
    }
    console.log(`[package-plugins] Uploaded ${built.length} zips to the '${RELEASE_TAG}' release.`);
  } else {
    console.log("[package-plugins] Upload with:");
    for (const command of uploadCommands) {
      console.log(`  ${command}`);
    }
    console.log("  (or re-run with --upload to do it now)");
  }

  console.log(`[package-plugins] Done. Zips are in ${outDir}.`);
}

// Update `plugins.json` for the five official plugins: version and
// metadata come from each plugin's own `plugin.json`, the download URL is
// the deterministic asset URL on the `plugins` release, and the checksum
// + size are the ones just built. Only `artifacts[<suffix>]` is touched
// for each id — other platforms' entries, other plugins' entries, and
// existing tags are preserved.
function updateRegistry(built, suffix) {
  const registryPath = path.join(rootDir, REGISTRY_PATH);
  let registry;
  try {
    registry = JSON.parse(fs.readFileSync(registryPath, "utf8"));
  } catch {
    fail(`Cannot read the registry at ${registryPath}.`);
  }
  if (!Array.isArray(registry.plugins)) {
    fail(`The registry at ${registryPath} has no "plugins" array.`);
  }
  for (const item of built) {
    const manifest = JSON.parse(
      fs.readFileSync(path.join(srcTauriDir, "plugins", item.id, "plugin.json"), "utf8"),
    );
    let entry = registry.plugins.find((p) => p.id === item.id);
    if (!entry) {
      entry = { id: item.id, tags: [] };
      registry.plugins.push(entry);
    }
    entry.name = manifest.name;
    entry.description = manifest.description;
    entry.author = manifest.author;
    entry.repo = REPO;
    entry.homepage = manifest.homepage ?? `https://github.com/${REPO}`;
    entry.official = true;
    entry.capabilities = manifest.capabilities ?? [];
    entry.version = manifest.version;
    entry.release_tag = RELEASE_TAG;
    if (!Array.isArray(entry.tags)) {
      entry.tags = [];
    }
    entry.artifacts = entry.artifacts ?? {};
    entry.artifacts[suffix] = {
      url: `https://github.com/${REPO}/releases/download/${RELEASE_TAG}/${item.zipName}`,
      sha256: item.sha256,
      size: item.size,
    };
  }
  fs.writeFileSync(registryPath, `${JSON.stringify(registry, null, 2)}\n`, "utf8");
  console.log(`[package-plugins] Updated ${REGISTRY_PATH} for ${suffix}.`);
}

main();
