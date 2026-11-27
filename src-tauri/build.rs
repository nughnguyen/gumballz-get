fn main() {
    // Stage the five bundled official plugins (cdylib + plugin.json) into
    // `resources/bundled-plugins/<id>/` BEFORE `tauri_build::build()`,
    // which copies the configured `bundle.resources` next to the binary in
    // dev (so `BaseDirectory::Resource` finds them under `tauri dev`) and
    // into the bundle in release builds. The app crate's build script runs
    // after its dependencies are compiled, so the plugin cdylibs already
    // exist in `target/<profile>/` when this runs — except under
    // `cargo check` / `cargo clippy`, which produce no cdylib artifacts;
    // staging then skips silently (with a warning) and never fails the build.
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

/// Stage the bundled plugin payloads into
/// `src-tauri/resources/bundled-plugins/<id>/` (cdylib + `plugin.json` per
/// id), cleaning the staging dir first so stale files never ship. The
/// profile target dir is derived from `OUT_DIR` ancestors (`OUT_DIR` is
/// `target/<profile>/build/<crate>-<hash>/out`, so the profile dir is three
/// ancestors up) — which honours `CARGO_TARGET_DIR` automatically, since
/// `OUT_DIR` already lives under it. A plugin whose artifact is missing
/// (e.g. `cargo check` / `cargo clippy`, which build no cdylib) is skipped
/// with a `cargo:warning=`; the build itself never fails for that.
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
        let cdylib_name = format!("{lib_prefix}gumballz-get-plugin-{plugin_id}.{lib_ext}");
        let cdylib_src = target_profile_dir.join(&cdylib_name);
        if !cdylib_src.is_file() {
            // `cargo check` / `cargo clippy` produce no cdylib artifacts.
            println!(
                "cargo:warning=bundled plugins: artifact not built yet, skipping '{plugin_id}' ({} not found)",
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
