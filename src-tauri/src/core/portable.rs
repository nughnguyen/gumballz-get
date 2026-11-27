//! Where WebView2 keeps its data, in portable mode.
//!
//! The `WEBVIEW2_USER_DATA_FOLDER` set in `main.rs` is dead code: Tauri
//! resolves `data_directory` to `LocalData/{identifier}` before the variable
//! is read (`manager/webview.rs`), creates the directory, and wry receives a
//! non-empty path — the variable is never consulted. `tauri.conf.json` does
//! not resolve it either, because it only accepts a relative path, resolved
//! under `data_local_dir()`.
//!
//! The way out is to not let Tauri create the window (`"create": false`) and
//! build it in `setup()` with an explicit `.data_directory(...)`. This module
//! isolates the decidable part — which directory to use — so it can be tested
//! without launching the app.

use std::path::{Path, PathBuf};

/// WebView2 data directory when the app runs in portable mode.
///
/// `None` when not portable: in that case Tauri's default behaviour is
/// correct and must not be overridden.
pub fn portable_webview_dir(data_dir: Option<&Path>) -> Option<PathBuf> {
    data_dir.map(|d| d.join("webview"))
}

/// Reads portable mode from the environment, the way `main.rs` publishes it.
///
/// Reads `GUMBALLZ_DATA_DIR` instead of re-detecting `portable.txt`: detection
/// happens exactly once, before anything else starts, and duplicating the rule
/// here would create two sources of truth that diverge at the first change.
pub fn portable_webview_dir_from_env() -> Option<PathBuf> {
    if std::env::var("GUMBALLZ_PORTABLE").ok().as_deref() != Some("1") {
        return None;
    }
    let data_dir = std::env::var("GUMBALLZ_DATA_DIR").ok()?;
    if data_dir.trim().is_empty() {
        return None;
    }
    portable_webview_dir(Some(Path::new(&data_dir)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nao_portatil_nao_sobrescreve_o_padrao_do_tauri() {
        assert_eq!(portable_webview_dir(None), None);
    }

    #[test]
    fn portatil_fica_ao_lado_do_executavel() {
        let dir = PathBuf::from("/media/pendrive/GumballZ Get/data");
        let got = portable_webview_dir(Some(&dir)).expect("portable has a directory");
        assert_eq!(got, dir.join("webview"));
        // The point of issue #195: nothing may end up under the user's profile.
        assert!(got.starts_with(&dir));
    }

    #[test]
    fn caminho_com_espaco_sobrevive() {
        // Windows portatil vive em "C:\Users\x\Downloads\GumballZ Get 0.7.7\data".
        let dir = PathBuf::from("C:/Users/x/Downloads/GumballZ Get 0.7.7/data");
        let got = portable_webview_dir(Some(&dir)).expect("portable has a directory");
        assert!(got.to_string_lossy().contains("GumballZ Get 0.7.7"));
        assert!(got.ends_with("webview"));
    }

    #[test]
    fn leitor_de_ambiente_exige_as_duas_variaveis() {
        // A single sequential test: `set_var` is process-global and tests run
        // in parallel. No other test touches these two variables, but splitting
        // this into several would be a guaranteed race.
        let restore = (
            std::env::var("GUMBALLZ_PORTABLE").ok(),
            std::env::var("GUMBALLZ_DATA_DIR").ok(),
        );

        std::env::remove_var("GUMBALLZ_PORTABLE");
        std::env::remove_var("GUMBALLZ_DATA_DIR");
        assert_eq!(
            portable_webview_dir_from_env(),
            None,
            "no env means not portable"
        );

        std::env::set_var("GUMBALLZ_PORTABLE", "1");
        assert_eq!(
            portable_webview_dir_from_env(),
            None,
            "portable without a data dir must not invent a path"
        );

        std::env::set_var("GUMBALLZ_DATA_DIR", "   ");
        assert_eq!(
            portable_webview_dir_from_env(),
            None,
            "a blank data dir must not become a relative path"
        );

        std::env::set_var("GUMBALLZ_DATA_DIR", "/tmp/gumballz-get-portatil/data");
        assert_eq!(
            portable_webview_dir_from_env(),
            Some(PathBuf::from("/tmp/gumballz-get-portatil/data/webview"))
        );

        std::env::set_var("GUMBALLZ_PORTABLE", "0");
        assert_eq!(
            portable_webview_dir_from_env(),
            None,
            "so a string \"1\" liga o modo portatil"
        );

        match restore.0 {
            Some(v) => std::env::set_var("GUMBALLZ_PORTABLE", v),
            None => std::env::remove_var("GUMBALLZ_PORTABLE"),
        }
        match restore.1 {
            Some(v) => std::env::set_var("GUMBALLZ_DATA_DIR", v),
            None => std::env::remove_var("GUMBALLZ_DATA_DIR"),
        }
    }
}

/// Folder name the Fixed Version Runtime installer produces.
///
/// Microsoft's package unpacks as
/// `Microsoft.WebView2.FixedVersionRuntime.{version}.{arch}`. We also accept
/// `WebView2Runtime`, the short name that can be documented without asking the
/// user to type a version.
fn parece_runtime_fixo(name: &str) -> bool {
    name.eq_ignore_ascii_case("WebView2Runtime")
        || name
            .to_ascii_lowercase()
            .starts_with("microsoft.webview2.fixedversionruntime.")
}

/// Version embedded in the folder name, for ordering when there is more than one.
///
/// Returns empty for `WebView2Runtime` (no version in the name), which puts it
/// below any versioned folder — whoever unpacked an explicit version probably
/// wants that one.
fn versao_do_nome(name: &str) -> Vec<u32> {
    name.split('.')
        .filter_map(|p| p.parse::<u32>().ok())
        .collect()
}

/// Looks for a WebView2 Fixed Version Runtime next to the executable.
///
/// Requires `msedgewebview2.exe` inside the folder: pointing the loader at a
/// directory that only looks right by name fails *later*, when the window is
/// created, and then the app does not open — worse than just using the system
/// runtime.
///
/// Requested in issue #218. Works together with portable mode (#195): a USB
/// stick with the runtime next to the exe runs on Windows without WebView2
/// installed.
pub fn find_fixed_runtime(exe_dir: &Path) -> Option<PathBuf> {
    let mut achados: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(exe_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !parece_runtime_fixo(&name) {
                return None;
            }
            let dir = e.path();
            if !dir.join("msedgewebview2.exe").exists() {
                return None;
            }
            Some((versao_do_nome(&name), dir))
        })
        .collect();

    achados.sort_by(|a, b| a.0.cmp(&b.0));
    achados.pop().map(|(_, dir)| dir)
}

#[cfg(test)]
mod runtime_fixo_tests {
    use super::*;

    fn cria_runtime(raiz: &Path, nome: &str, com_exe: bool) {
        let dir = raiz.join(nome);
        std::fs::create_dir_all(&dir).expect("creates folder");
        if com_exe {
            std::fs::write(dir.join("msedgewebview2.exe"), b"stub").expect("creates exe");
        }
    }

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gumballz-get-rt-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("creates tempdir");
        d
    }

    #[test]
    fn sem_runtime_ao_lado_nao_inventa_caminho() {
        let d = tempdir("vazio");
        std::fs::write(d.join("gumballz-get.exe"), b"stub").expect("creates exe");
        assert_eq!(find_fixed_runtime(&d), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn pasta_sem_o_binario_e_recusada() {
        // The case that breaks the app if accepted: right name, missing content.
        let d = tempdir("sem-exe");
        cria_runtime(&d, "WebView2Runtime", false);
        assert_eq!(
            find_fixed_runtime(&d),
            None,
            "the right name without msedgewebview2.exe must not be accepted"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nome_curto_e_aceito() {
        let d = tempdir("curto");
        cria_runtime(&d, "WebView2Runtime", true);
        assert_eq!(find_fixed_runtime(&d), Some(d.join("WebView2Runtime")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nome_do_instalador_da_microsoft_e_aceito() {
        let d = tempdir("ms");
        let nome = "Microsoft.WebView2.FixedVersionRuntime.120.0.2210.91.x64";
        cria_runtime(&d, nome, true);
        assert_eq!(find_fixed_runtime(&d), Some(d.join(nome)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn com_duas_versoes_escolhe_a_maior() {
        let d = tempdir("duas");
        let velha = "Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x64";
        let nova = "Microsoft.WebView2.FixedVersionRuntime.120.0.2210.91.x64";
        // Created in reverse order so that "take the first one" does not pass by accident.
        cria_runtime(&d, nova, true);
        cria_runtime(&d, velha, true);
        let got = find_fixed_runtime(&d).expect("found runtime");
        assert_eq!(got, d.join(nova));
        assert_ne!(got, d.join(velha), "109 must not beat 120");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn pasta_qualquer_ao_lado_e_ignorada() {
        let d = tempdir("ruido");
        cria_runtime(&d, "plugins", true);
        cria_runtime(&d, "data", true);
        assert_eq!(find_fixed_runtime(&d), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// A single boot line, emitted before anything can go out silently.
///
/// `tauri-plugin-single-instance` calls `std::process::exit(0)` without logging
/// anything when it detects another live instance. Without this line, an app
/// that exits that way is indistinguishable from one that crashed: the log just
/// stops. It cost a whole investigation session to discover that, and a user
/// opening the app twice has no log at all to look at.
///
/// With the line, the diagnosis becomes trivial: banner present and nothing
/// after means another instance took over; no banner means the process never
/// got up.
pub fn startup_banner(version: &str, pid: u32, portable: bool, data_dir: Option<&Path>) -> String {
    let modo = if portable { "portable" } else { "standard" };
    let dir = data_dir
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    format!("GumballZ Get {version} starting — pid {pid}, {modo} mode, data dir {dir}")
}

#[cfg(test)]
mod banner_tests {
    use super::*;

    #[test]
    fn banner_nomeia_o_modo_e_o_diretorio() {
        let b = startup_banner("0.7.7", 1234, true, Some(Path::new("/pendrive/data")));
        assert!(b.contains("GumballZ Get 0.7.7"));
        assert!(b.contains("pid 1234"));
        assert!(b.contains("portable mode"));
        assert!(b.contains("/pendrive/data"));
    }

    #[test]
    fn banner_nao_esconde_diretorio_nao_resolvido() {
        // If the data dir did not resolve, saying so is worth more than omitting
        // it: it is the first thing to look at when the app behaves as if it had
        // no configuration at all.
        let b = startup_banner("0.7.7", 1, false, None);
        assert!(b.contains("standard mode"));
        assert!(b.contains("<unresolved>"));
    }
}
