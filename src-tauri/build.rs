fn main() {
    // Stage the five bundled official plugins (cdylib + plugin.json) into
    // `resources/bundled-plugins/<id>/` BEFORE `tauri_build::build()`,
    // which copies the configured `bundle.resources` next to the binary in
    // dev (so `BaseDirectory::Resource` finds them under `tauri dev`) and
    // into the bundle in release builds.
    //
    // The cdylibs staged here are NOT the dependency builds of the plugin
    // crates: cargo places a dependency crate's cdylib under
    // `target/<profile>/deps/` with a metadata hash in the file name, so
    // the app crate's build script can never find it at the top level.
    // They come from a pre-build step
    // (`scripts/build-bundled-plugins.mjs`, run by Tauri's
    // `beforeDevCommand` / `beforeBuildCommand` before cargo starts) that
    // builds all five plugin crates as cargo root targets with the
    // `dynamic` feature, which links each final artifact to the top level
    // without a hash (`target/<profile>/gumballz_get_plugin_<id>.dll` on
    // Windows, `libgumballz_get_plugin_<id>.so` / `.dylib` elsewhere).
    // Under `cargo check` / `cargo clippy`, which produce no cdylib
    // artifacts, staging skips with a warning and never fails the build.
    stage_bundled_plugins();
    // The bundled SQLite (libsqlite3-sys "bundled") defines sqlite3_* symbols
    // in this executable. When a linked shared library (webkit2gtk and its
    // deps reference sqlite3_open_v2, sqlite3_prepare_v2, ...) has an
    // undefined reference to one of them, ld auto-exports the definition so
    // the shared lib binds to it at runtime. Those exports then preempt
    // sqlite3 usage process-wide: libsqlite3.so.0's own sqlite3_initialize
    // is among the preempted calls, so the system library's global config is
    // never initialized while some of its internal calls execute in the
    // bundled copy. Any plugin using the system libsqlite3 (e.g. the
    // Telegram plugin's SqliteSession) then segfaults on its first
    // sqlite3_mutex_enter, which tail-jumps through the never-populated
    // mutex-methods table. --exclude-libs keeps static-archive symbols
    // local: shared libs bind to the real libsqlite3.so.0 and the bundled
    // copy stays private to the executable.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg=-Wl,--exclude-libs,ALL");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-ObjC");
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        if let Ok(out) = std::process::Command::new("xcode-select")
            .arg("-p")
            .output()
        {
            let xcode = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !xcode.is_empty() {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{xcode}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx");
            }
        }
    }
    tauri_build::build()
}

/// The five official tools that ship inside the app bundle as installable
/// payloads (see `commands::plugins::install_plugin_from_bundle`). Must stay
/// in sync with `BUNDLED_PLUGIN_IDS` in `src/commands/plugins.rs`.
const BUNDLED_PLUGIN_IDS: &[&str] = &[
    "convert",
    "telegram",
    "courses",
    "listening-studio",
    "asset-forge",
];

/// Copy one staged plugin payload (the built cdylib + its `plugin.json`)
/// into `dest_dir`. Only that one file is ever copied — never `.dll.lib`,
/// `.exp`, `.rlib` or anything else from the target dir.
fn stage_one_plugin(
    cdylib_src: &std::path::Path,
    cdylib_name: &str,
    manifest_src: &std::path::Path,
    dest_dir: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dest_dir)?;
    std::fs::copy(cdylib_src, dest_dir.join(cdylib_name))?;
    std::fs::copy(manifest_src, dest_dir.join("plugin.json"))?;
    Ok(())
}

/// Resolve where a pre-built plugin cdylib lives: normally the profile
/// dir derived from `OUT_DIR` (`target/<profile>/`, or
/// `target/<triple>/<profile>/` for a cross-target build). When the main
/// cargo invocation was given an explicit `--target` that names the host
/// triple itself, the app build sits in `target/<triple>/<profile>/`
/// while the pre-build step's host artifacts sit in `target/<profile>/`,
/// so fall back to the host profile dir in that case only
/// (`TARGET == HOST`, both set by cargo for build scripts). The fallback
/// path is only used when the profile dir really is
/// `target/<triple>/<profile>` for that triple, and only when the file
/// exists there; otherwise the primary path is returned unchanged and the
/// caller reports it as missing.
fn cdylib_source(
    target_profile_dir: &std::path::Path,
    cdylib_name: &str,
) -> std::path::PathBuf {
    let primary = target_profile_dir.join(cdylib_name);
    if primary.is_file() {
        return primary;
    }
    let target_triple = std::env::var("TARGET").ok();
    let host_triple = std::env::var("HOST").ok();
    let same_triple = matches!((&target_triple, &host_triple), (Some(t), Some(h)) if t == h);
    if !same_triple {
        return primary;
    }
    let Some(triple) = target_triple else {
        return primary;
    };
    // target_profile_dir is target/<triple>/<profile>: step up to
    // target/<triple>, check its name is the triple, then step up to
    // target/ and re-enter the plain profile dir.
    let Some(triple_dir) = target_profile_dir.parent() else {
        return primary;
    };
    if triple_dir.file_name().and_then(|n| n.to_str()) != Some(triple.as_str()) {
        return primary;
    }
    let Some(target_root) = triple_dir.parent() else {
        return primary;
    };
    let Ok(profile) = std::env::var("PROFILE") else {
        return primary;
    };
    let fallback = target_root.join(profile).join(cdylib_name);
    if fallback.is_file() {
        fallback
    } else {
        primary
    }
}

/// Stage the bundled plugin payloads into
/// `src-tauri/resources/bundled-plugins/<id>/` (cdylib + `plugin.json` per
/// id), cleaning the staging dir first so stale files never ship. The
/// profile target dir is derived from `OUT_DIR` ancestors (`OUT_DIR` is
/// `target/<profile>/build/<crate>-<hash>/out`, so the profile dir is three
/// ancestors up) — which honours `CARGO_TARGET_DIR` automatically, since
/// `OUT_DIR` already lives under it, and honours `--target` too, since the
/// triple dir sits between `target/` and the profile dir. The cdylib
/// looked up here is the one the pre-build step
/// (`scripts/build-bundled-plugins.mjs`) produced as a cargo root target
/// with the `dynamic` feature: a dependency build would land hashed in
/// `deps/` instead, so it can never match. The pre-build step builds for
/// the host target without `--target`, so its artifacts land in
/// `target/<profile>/`; when the main cargo invocation was given an
/// explicit `--target` naming the host triple anyway (e.g. the macOS
/// release job passes `--target aarch64-apple-darwin` on an arm64 runner),
/// the app build moves to `target/<triple>/<profile>/` while the
/// artifacts stay the same architecture, so that case also falls back to
/// the host profile dir — but only when `TARGET` equals `HOST`, so a true
/// cross-compile can never pick up a wrong-architecture cdylib. A plugin
/// whose artifact is missing (e.g. `cargo check` / `cargo clippy`, which
/// build no cdylib, or a pre-build step that did not run) is skipped with
/// a `cargo:warning=`; the build itself never fails for that.
fn stage_bundled_plugins() {
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
    println!("cargo:rerun-if-env-changed=PROFILE");

    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()),
    );

    // (lib prefix, extension) of the built cdylib on this target.
    let lib_name_parts: Option<(&str, &str)> =
        match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
            Ok("windows") => Some(("", "dll")),
            Ok("macos") => Some(("lib", "dylib")),
            Ok("linux") => Some(("lib", "so")),
            _ => None,
        };
    let Some((lib_prefix, lib_ext)) = lib_name_parts else {
        println!("cargo:warning=bundled plugins: unsupported target OS, skipping bundled plugins");
        return;
    };

    // OUT_DIR = target/<profile>/build/<crate>-<hash>/out.
    let target_profile_dir = std::env::var("OUT_DIR").ok().and_then(|out_dir| {
        let mut dir = std::path::PathBuf::from(out_dir);
        for _ in 0..3 {
            if !dir.pop() {
                return None;
            }
        }
        Some(dir)
    });
    let Some(target_profile_dir) = target_profile_dir else {
        println!("cargo:warning=bundled plugins: cannot locate the cargo target dir (OUT_DIR unreadable), skipping bundled plugins");
        return;
    };

    // Clean slate every build so stale payloads never ship.
    let staging_root = manifest_dir.join("resources").join("bundled-plugins");
    let _ = std::fs::remove_dir_all(&staging_root);
    if let Err(e) = std::fs::create_dir_all(&staging_root) {
        println!(
            "cargo:warning=bundled plugins: cannot create staging dir {}: {}",
            staging_root.display(),
            e
        );
        return;
    }

    for plugin_id in BUNDLED_PLUGIN_IDS {
        println!("cargo:rerun-if-changed=plugins/{plugin_id}/plugin.json");
        // Cargo names a library file after the lib target name, which is
        // the package name with hyphens replaced by underscores
        // (`gumballz-get-plugin-convert` ->
        // `gumballz_get_plugin_convert.dll`). The staged file keeps that
        // name: the installer scans the payload dir for any single
        // `.dll` / `.so` / `.dylib`, so the exact file name does not
        // matter, only that exactly one native library is present.
        let cdylib_name = format!(
            "{lib_prefix}gumballz_get_plugin_{}.{lib_ext}",
            plugin_id.replace('-', "_")
        );
        let cdylib_src = cdylib_source(&target_profile_dir, &cdylib_name);
        if !cdylib_src.is_file() {
            // `cargo check` / `cargo clippy` produce no cdylib artifacts,
            // so skipping is expected there. In a real `tauri dev` /
            // `tauri build`, a missing artifact means the pre-build step
            // (`pnpm plugins:build` for debug, `pnpm plugins:build:release`
            // for release) did not run or failed — the bundle then ships
            // without that plugin, so this warning names the step instead
            // of implying the artifact is merely "not built yet".
            println!(
                "cargo:warning=bundled plugins: pre-build step did not produce an artifact for '{plugin_id}' ({} not found); run 'pnpm plugins:build' (debug) or 'pnpm plugins:build:release' (release) before building, or use 'pnpm tauri dev' / 'pnpm tauri build'. Skipping '{plugin_id}' — expected under 'cargo check' / 'cargo clippy', which build no cdylib.",
                cdylib_src.display()
            );
            continue;
        }
        let manifest_src = manifest_dir
            .join("plugins")
            .join(plugin_id)
            .join("plugin.json");
        if !manifest_src.is_file() {
            println!(
                "cargo:warning=bundled plugins: plugin.json missing for '{plugin_id}', skipping"
            );
            continue;
        }
        let dest_dir = staging_root.join(plugin_id);
        if let Err(e) =
            stage_one_plugin(&cdylib_src, &cdylib_name, &manifest_src, &dest_dir)
        {
            println!("cargo:warning=bundled plugins: failed to stage '{plugin_id}': {e}");
            let _ = std::fs::remove_dir_all(&dest_dir);
        }
    }
}
