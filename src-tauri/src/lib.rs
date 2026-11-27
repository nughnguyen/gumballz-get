// GumballZ Get - Modern cross-platform download manager
// Copyright (C) 2026 Nguyen Quoc Hung (GumballZ) <https://github.com/nughnguyen/gumballz-get>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

use tokio_util::sync::CancellationToken;

pub struct P2pSendHandle {
    pub cancel_token: CancellationToken,
    pub paused: Arc<std::sync::atomic::AtomicBool>,
}
pub type ActiveP2pSends = Arc<tokio::sync::Mutex<HashMap<String, P2pSendHandle>>>;

pub mod commands;
pub mod cookies;
pub mod core;
pub mod extension_storage;
pub mod external_url;
pub mod hotkey;
pub mod local_bridge;
pub mod mcp;
pub mod models;
pub mod platforms;
pub mod plugin_host;
pub mod plugin_loader;
pub mod storage;
pub mod tray;

struct DesktopCookieProvider;

impl gumballz_core::platforms::cookie_provider::CookieProvider for DesktopCookieProvider {
    fn cookie_path_for(&self, domain: &str) -> Option<std::path::PathBuf> {
        let root = crate::cookies::root_domain_of(domain);
        if root.is_empty() {
            return None;
        }

        let slug = gumballz_core::core::log_hook::current_cookie_slug();
        let path = crate::cookies::account_path_for_consumer(&root, slug.as_deref())
            .or_else(|| crate::cookies::account_path_for_consumer(&root, None))?;
        crate::cookies::touch_last_used(&root, slug.as_deref().unwrap_or("_default"));
        Some(path)
    }

    fn cookie_path_for_account(&self, domain: &str, slug: &str) -> Option<std::path::PathBuf> {
        let root = crate::cookies::root_domain_of(domain);
        if root.is_empty() {
            return None;
        }
        let path = crate::cookies::account_path_for_consumer(&root, Some(slug))?;
        crate::cookies::touch_last_used(&root, slug);
        Some(path)
    }

    fn manual_cookie_header(&self, domain: &str) -> Option<String> {
        let root = crate::cookies::root_domain_of(domain);
        if root != "x.com" && root != "twitter.com" {
            return None;
        }

        let raw = storage::config::load_settings_standalone()
            .advanced
            .twitter_manual_cookie;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let parsed = core::cookie_parser::parse_cookie_input(trimmed, "");
        if !parsed.cookie_string.trim().is_empty() {
            Some(parsed.cookie_string)
        } else {
            Some(trimmed.to_string())
        }
    }
}

struct DesktopBilibiliRuntimeProvider;

impl gumballz_core::platforms::bilibili::BilibiliRuntimeProvider
    for DesktopBilibiliRuntimeProvider
{
    fn active_account_slug(&self) -> Option<String> {
        let registry = crate::cookies::load_registry();
        let bucket = registry.buckets.get("bilibili.com")?;

        if let Some(selected) = gumballz_core::core::log_hook::current_cookie_slug() {
            if bucket
                .accounts
                .iter()
                .any(|a| a.slug == selected && a.slug != "_anonymous" && a.cookie_count > 0)
            {
                return Some(selected);
            }
        }

        let native = bucket
            .accounts
            .iter()
            .filter(|a| a.slug != "_anonymous" && a.slug != "_default" && a.cookie_count > 0)
            .max_by_key(|a| a.last_used_at_ms.unwrap_or(a.captured_at_ms))
            .map(|a| a.slug.clone());
        if native.is_some() {
            return native;
        }

        bucket
            .accounts
            .iter()
            .filter(|a| a.slug == "_default" && a.cookie_count > 0)
            .find(|a| desktop_bilibili_slug_has_session_cookie(&a.slug))
            .map(|a| a.slug.clone())
    }

    fn settings(&self) -> gumballz_core::models::settings::AppSettings {
        storage::config::load_settings_standalone()
    }

    fn persist_account(
        &self,
        cookies: &[gumballz_core::platforms::bilibili::BilibiliAuthCookie],
        uname: &str,
        source_label: &str,
    ) -> Result<String, String> {
        let slug = gumballz_core::platforms::bilibili::auth::slug_from_uname(uname);
        let entries: Vec<crate::extension_storage::ExtensionCookie> = cookies
            .iter()
            .map(|cookie| crate::extension_storage::ExtensionCookie {
                domain: cookie.domain.clone(),
                http_only: cookie.http_only,
                path: cookie.path.clone(),
                secure: cookie.secure,
                expires: cookie.expires,
                name: cookie.name.clone(),
                value: cookie.value.clone(),
                host_only: cookie.host_only,
                same_site: cookie.same_site.clone(),
            })
            .collect();

        crate::cookies::storage::write_account_file("bilibili.com", &slug, &entries)
            .map_err(|e| e.to_string())?;

        let mut registry = crate::cookies::storage::load_registry();
        let now_ms = crate::cookies::storage::current_unix_ms();
        let bucket = registry
            .buckets
            .entry("bilibili.com".to_string())
            .or_insert_with(|| crate::cookies::storage::BucketEntry {
                platform_kind: "bilibili".to_string(),
                enabled: true,
                accounts: Vec::new(),
            });
        bucket.platform_kind = "bilibili".to_string();

        let alias = format!("{} · {}", uname, source_label);
        if let Some(existing) = bucket.accounts.iter_mut().find(|a| a.slug == slug) {
            existing.captured_at_ms = now_ms;
            existing.cookie_count = entries.len();
            existing.source_label = Some(source_label.to_string());
            existing.alias = alias;
        } else {
            bucket.accounts.push(crate::cookies::storage::AccountEntry {
                slug: slug.clone(),
                alias,
                source_url: Some("https://www.bilibili.com".to_string()),
                source_label: Some(source_label.to_string()),
                captured_at_ms: now_ms,
                cookie_count: entries.len(),
                last_used_at_ms: Some(now_ms),
            });
        }

        crate::cookies::storage::save_registry(&registry).map_err(|e| e.to_string())?;
        Ok(slug)
    }
}

fn desktop_bilibili_slug_has_session_cookie(slug: &str) -> bool {
    let path = match crate::cookies::account_path_for_consumer("bilibili.com", Some(slug)) {
        Some(p) => p,
        None => return false,
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    content.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return false;
        }
        line.split('\t').nth(5) == Some("SESSDATA")
    })
}

pub struct AppState {
    pub active_downloads: Arc<tokio::sync::Mutex<HashMap<u64, CancellationToken>>>,
    pub active_generic_downloads:
        Arc<tokio::sync::Mutex<HashMap<u64, (String, CancellationToken)>>>,
    pub registry: core::registry::PlatformRegistry,
    pub download_queue: Arc<tokio::sync::Mutex<core::queue::DownloadQueue>>,
    pub torrent_session: Arc<tokio::sync::Mutex<Option<Arc<librqbit::Session>>>>,
    pub active_p2p_sends: ActiveP2pSends,
    pub frontend_ready: Arc<tokio::sync::Mutex<bool>>,
    pub pending_external_events: Arc<tokio::sync::Mutex<Vec<external_url::ExternalUrlEvent>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    // Emitted before single-instance can exit silently: see
    // core::portable::startup_banner.
    tracing::info!(
        "{}",
        core::portable::startup_banner(
            env!("CARGO_PKG_VERSION"),
            std::process::id(),
            std::env::var("GUMBALLZ_PORTABLE").ok().as_deref() == Some("1"),
            core::paths::app_data_dir().as_deref(),
        )
    );

    let mut registry = core::registry::PlatformRegistry::new();
    // gallery-dl first: it claims bulk-listing URLs (profiles, subreddits)
    // that the single-post Twitter/Reddit downloaders would otherwise match
    registry.register(Arc::new(platforms::gallerydl::GalleryDlDownloader::new()));
    registry.register(Arc::new(
        gumballz_core::platforms::InstagramDownloader::new(),
    ));
    registry.register(Arc::new(gumballz_core::platforms::ThreadsDownloader::new()));
    registry.register(Arc::new(
        gumballz_core::platforms::PinterestDownloader::new(),
    ));
    registry.register(Arc::new(gumballz_core::platforms::TikTokDownloader::new()));
    registry.register(Arc::new(gumballz_core::platforms::TwitterDownloader::new()));
    registry.register(Arc::new(
        gumballz_core::platforms::TwitchClipsDownloader::new(),
    ));
    registry.register(Arc::new(gumballz_core::platforms::BlueskyDownloader::new()));
    registry.register(Arc::new(gumballz_core::platforms::RedditDownloader::new()));
    registry.register(Arc::new(gumballz_core::platforms::YouTubeDownloader::new()));
    registry.register(Arc::new(gumballz_core::platforms::VimeoDownloader::new()));
    registry.register(Arc::new(gumballz_core::platforms::BilibiliDownloader::new()));
    registry.register(Arc::new(gumballz_core::platforms::DouyinDownloader::new()));
    let torrent_session: Arc<tokio::sync::Mutex<Option<Arc<librqbit::Session>>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    registry.register(Arc::new(platforms::magnet::MagnetDownloader::new(
        torrent_session.clone(),
    )));
    registry.register(Arc::new(gumballz_core::platforms::P2pDownloader::new()));
    registry.register(Arc::new(gumballz_core::platforms::GdocsDownloader::new()));
    registry.register(Arc::new(
        gumballz_core::platforms::ScribdDownloader::new(),
    ));
    registry.register(Arc::new(
        gumballz_core::platforms::StudocuDownloader::new(),
    ));
    registry.register(Arc::new(
        gumballz_core::platforms::SpotifyDownloader::new(),
    ));
    registry.register(Arc::new(
        gumballz_core::platforms::DirectFileDownloader::new(),
    ));
    registry.register(Arc::new(
        platforms::generic_ytdlp::GenericYtdlpDownloader::new(),
    ));

    let state = AppState {
        active_downloads: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        active_generic_downloads: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        registry,
        download_queue: Arc::new(tokio::sync::Mutex::new(core::queue::DownloadQueue::new(2))),
        torrent_session,
        active_p2p_sends: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        frontend_ready: Arc::new(tokio::sync::Mutex::new(false)),
        pending_external_events: Arc::new(tokio::sync::Mutex::new(Vec::new())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(url) =
                external_url::find_external_url_arg(argv.iter().skip(1).map(|arg| arg.as_str()))
            {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) =
                        external_url::handle_external_url(&app_handle, url, "command-line").await
                    {
                        tracing::warn!(
                            "Failed to handle external URL from second instance: {}",
                            error
                        );
                    }
                });
            } else {
                tray::show_window(app);
            }
        }))
        .manage(state)
        .manage(Arc::new(tokio::sync::RwLock::new(
            plugin_loader::PluginManager::new(
                core::paths::app_data_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("plugins"),
            ),
        )))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        hotkey::on_hotkey_pressed(app, shortcut);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // The main window is created here, not by `tauri.conf.json`
            // (`"create": false`), because only from here can
            // `.data_directory(...)` be passed to WebView2. Tauri resolves
            // that path to `LocalData/{identifier}` and creates the directory
            // *before* setup runs, so the environment variable that `main.rs`
            // sets is never read — which is the cause of issue #195, portable
            // mode leaving a folder outside the app directory.
            {
                // `mut` is only used in the Windows branch below; without the
                // cfg_attr this becomes a new warning on macOS and Linux and
                // fails the clippy gate.
                #[cfg_attr(not(any(windows, target_os = "linux")), allow(unused_mut))]
                let mut builder = tauri::WebviewWindowBuilder::from_config(
                    app.handle(),
                    &app.config().app.windows[0],
                )?;

                // Not Windows only. On Linux wry uses this path for
                // `base_data_directory`, `base_cache_directory` and the
                // WebKitGTK cookies; without it portable mode leaves
                // `XDG_DATA_HOME/com.gumballz.get` in the user profile — the
                // same #209, outside Windows. The B55 smoke test caught this
                // the first time it ran.
                //
                // macOS is left out because wry does not read `data_directory`
                // in WKWebView: including it would give the impression of
                // covering a case that is not covered.
                #[cfg(any(windows, target_os = "linux"))]
                if let Some(webview_dir) = core::portable::portable_webview_dir_from_env() {
                    if let Err(e) = std::fs::create_dir_all(&webview_dir) {
                        tracing::warn!(
                            "[portable] could not create {}: {} — WebView2 will use the default directory",
                            webview_dir.display(),
                            e
                        );
                    } else {
                        tracing::info!(
                            "[portable] WebView2 pointed at {}",
                            webview_dir.display()
                        );
                        builder = builder.data_directory(webview_dir);
                    }
                }

                // macOS has nowhere to point at: wry does not read
                // `data_directory` in WKWebView (no reference in
                // `src/wkwebview/`), unlike webkitgtk, which uses it for
                // base_data_directory, base_cache_directory and cookies.
                //
                // So portable mode does not deliver what it promises here, and
                // saying so is better than letting the user think the USB stick
                // left no trace. Issue #227.
                #[cfg(target_os = "macos")]
                if core::portable::portable_webview_dir_from_env().is_some() {
                    tracing::warn!(
                        "[portable] on macOS WebView stores data in ~/Library even in portable \
                         mode — wry does not allow redirecting it. See github.com/nughnguyen/gumballz-get/issues/227"
                    );
                }

                builder.build()?;

                // The only proof that the window came up. CI compiles on all
                // platforms but never opened the app: that is how #209 slipped
                // through. The CI smoke test matches exactly this line.
                tracing::info!("[startup] main window created");
            }

            // Smoke mode: starts up, proves the window exists, and exits on
            // its own with 0. It only exists so CI can answer "does it open?",
            // the question compiling never answers.
            if let Ok(raw) = std::env::var("GUMBALLZ_SMOKE_EXIT_MS") {
                if let Ok(ms) = raw.trim().parse::<u64>() {
                    let handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(ms));
                        tracing::info!("[startup] smoke mode: exiting cleanly");
                        handle.exit(0);
                    });
                }
            }

            commands::host_queue::register_event_listeners(app.handle());
            {
                let handle = app.handle().clone();
                platforms::bilibili::notify::set_emitter(Box::new(
                    move |event: &str, payload: serde_json::Value| {
                        use tauri::Emitter;
                        let _ = handle.emit(event, payload);
                    },
                ));
            }
            {
                let handle = app.handle().clone();
                gumballz_core::platforms::bilibili::notify::set_emitter(Box::new(
                    move |event: &str, payload: serde_json::Value| {
                        use tauri::Emitter;
                        let _ = handle.emit(event, payload);
                    },
                ));
            }
            let settings = storage::config::load_settings(app.handle());
            core::http_client::init_proxy(settings.proxy.clone());
            core::http_fetcher::set_global_max_concurrent_segments(
                settings.advanced.max_concurrent_segments as usize,
            );
            gumballz_core::platforms::cookie_provider::set_cookie_provider(Arc::new(
                DesktopCookieProvider,
            ));
            gumballz_core::platforms::bilibili::set_runtime_provider(Arc::new(
                DesktopBilibiliRuntimeProvider,
            ));
            core::ytdlp::set_per_domain_cookie_fn(|url| {
                let parsed = url::Url::parse(url).ok()?;
                let host = parsed.host_str()?;
                let root = crate::cookies::root_domain_of(host);
                if root.is_empty() {
                    return None;
                }
                let slug = gumballz_core::core::log_hook::current_cookie_slug();
                let path = crate::cookies::account_path_for_consumer(&root, slug.as_deref())?;
                crate::cookies::touch_last_used(&root, slug.as_deref().unwrap_or("_default"));
                Some(path)
            });
            core::ytdlp::set_managed_cookies_only_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .always_use_managed_cookies
            });
            core::ytdlp::set_global_cookie_file_fn(|| {
                let s = storage::config::load_settings_standalone();
                let cf = s.download.cookie_file.clone();
                if !cf.is_empty() && std::path::Path::new(&cf).exists() {
                    Some(cf)
                } else {
                    None
                }
            });
            core::ytdlp::set_cookies_from_browser_fn(|| {
                storage::config::load_settings_standalone()
                    .advanced
                    .cookies_from_browser
            });
            core::ytdlp::set_manual_cookie_header_fn(|| {
                storage::config::load_settings_standalone()
                    .advanced
                    .twitter_manual_cookie
            });
            core::ytdlp::set_ext_referer_fn(|url| {
                extension_storage::read_extension_metadata(url).and_then(|m| m.referer)
            });
            core::ytdlp::set_include_auto_subs_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .include_auto_subtitles
            });
            core::ytdlp::set_caption_locale_fn(|| {
                let s = storage::config::load_settings_standalone();
                let locale = s.download.caption_locale.trim().to_string();
                if locale.is_empty() || locale.eq_ignore_ascii_case("auto") {
                    let lang = s.appearance.language.trim().to_string();
                    if lang.is_empty() {
                        "en".to_string()
                    } else {
                        lang
                    }
                } else {
                    locale
                }
            });
            core::ytdlp::set_keep_vtt_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .keep_vtt
            });
            core::ytdlp::set_translate_metadata_fn(|| {
                let s = storage::config::load_settings_standalone();
                if s.download.translate_metadata {
                    let lang = s.appearance.language.trim();
                    if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    }
                } else {
                    None
                }
            });
            core::ytdlp::set_sponsorblock_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .youtube_sponsorblock
            });
            core::ytdlp::set_sponsorblock_mode_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .sponsorblock_mode
            });
            core::ytdlp::set_sponsorblock_categories_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .sponsorblock_categories
            });
            core::ytdlp::set_split_chapters_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .split_by_chapters
            });
            core::ytdlp::set_embed_metadata_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .embed_metadata
            });
            core::ytdlp::set_embed_thumbnail_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .embed_thumbnail
            });
            core::ytdlp::set_speed_limit_fn(|| {
                let v = storage::config::load_settings_standalone()
                    .download
                    .speed_limit;
                let t = v.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            });
            core::ytdlp::set_live_from_start_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .live_from_start
            });
            core::ytdlp::set_insecure_tls_fn(|| {
                storage::config::load_settings_standalone()
                    .advanced
                    .insecure_tls
            });
            core::ytdlp::set_concurrent_fragments_fn(|| {
                storage::config::load_settings_standalone()
                    .advanced
                    .concurrent_fragments
            });
            core::ytdlp::set_user_agent_fn(|| {
                let v = storage::config::load_settings_standalone()
                    .advanced
                    .user_agent;
                let t = v.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            });
            core::ytdlp::set_extra_ytdlp_flags_fn(|| {
                storage::config::load_settings_standalone()
                    .download
                    .extra_ytdlp_flags
            });
            {
                let app_handle = app.handle().clone();
                gumballz_core::core::log_hook::set_log_sink(std::sync::Arc::new(move |id, line| {
                    // B33: black box. Every download log line passes through
                    // here, and `record` redacts before storing — it is the
                    // only point where the history can be captured without
                    // instrumenting every call one by one.
                    core::flight_recorder::record(line);
                    let should_emit = core::download_log::push_line(id, line);
                    if should_emit {
                        // The line rides along so the card can show "the last
                        // thing yt-dlp said" without asking for the whole log
                        // on every event.
                        let _ = tauri::Emitter::emit(
                            &app_handle,
                            "download-log-update",
                            serde_json::json!({ "id": id, "line": line }),
                        );
                    }
                }));
            }
            core::recovery::init_from_disk();
            core::queue_history::init_from_disk();
            core::channels::init_from_disk();
            core::channel_poller::start(app.handle().clone());
            core::queue::start_scheduler(app.handle().clone());
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let snapshot = {
                        let mut q = state.download_queue.lock().await;
                        q.hydrate_from_history();
                        q.get_state()
                    };
                    if !snapshot.is_empty() {
                        core::queue::emit_queue_state_from_state(&app_handle, snapshot);
                    }
                });
            }
            {
                let pending = core::recovery::list();
                if !pending.is_empty() {
                    let app_handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                        let _ = tauri::Emitter::emit(
                            &app_handle,
                            "recovery-available",
                            serde_json::json!({ "count": pending.len() }),
                        );
                    });
                }
            }
            {
                let app_handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let raw = url.to_string();
                        let handle = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(error) =
                                external_url::handle_external_url(&handle, raw, "deep-link").await
                            {
                                tracing::warn!("Failed to handle deep-link URL: {}", error);
                            }
                        });
                    }
                });
            }
            // Make sure the OS routes `gumballzget://` to this binary even when the
            // package didn't ship a desktop file with the scheme handler MIME type.
            // or when the user is running from an AppImage / unpacked build.
            // No-op on macOS (which uses Info.plist registered by the bundler).
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            {
                if let Err(error) = app.deep_link().register_all() {
                    tracing::warn!("Failed to register gumballzget:// scheme: {}", error);
                }
            }
            tray::setup(app.handle())?;
            hotkey::register_from_settings(app.handle());

            // Migration: drop the manifests / binary copies the previous
            // native-messaging code left under `~/.config/...` so Chrome and
            // Firefox stop trying to start the legacy host process.
            extension_storage::cleanup_legacy_native_messaging();
            terminate_legacy_omniget_process();
            migrate_legacy_omniget_data();
            cleanup_legacy_omniget_artifacts();

            // Cookie Manager: repartitions the legacy
            // `chrome-extension-cookies.txt` into
            // `cookies/<domain>/_default.txt` (CK-1). When the registry
            // already has buckets, the migration becomes a no-op. After a
            // successful migration, the legacy file is deleted — from CK-7b on,
            // the multi-file layout is the only source of truth.
            if !cookies::storage::has_been_migrated() {
                match cookies::migrate_legacy_if_needed() {
                    Ok(_) => {
                        let legacy = extension_storage::extension_cookie_file_path();
                        if legacy.exists() {
                            if let Err(err) = std::fs::remove_file(&legacy) {
                                tracing::warn!(
                                    "cookies: failed to remove legacy file {}: {}",
                                    legacy.display(),
                                    err
                                );
                            }
                        }
                    }
                    Err(err) => tracing::warn!("cookies: legacy migration skipped: {err}"),
                }
            } else {
                // Registry already populated — means a previous migration
                // already ran; if the legacy file still exists (an
                // intermediate install), remove it now.
                let legacy = extension_storage::extension_cookie_file_path();
                if legacy.exists() {
                    if let Err(err) = std::fs::remove_file(&legacy) {
                        tracing::warn!(
                            "cookies: failed to remove stale legacy file {}: {}",
                            legacy.display(),
                            err
                        );
                    }
                }
            }

            // Start the localhost HTTP bridge for the browser extension. This
            // replaces the previous Chrome native-messaging path, which forced
            // us to maintain a hard-coded extension-ID allowlist. Bridge auth
            // is via a per-installation token shown in the Settings UI.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    local_bridge::spawn(app_handle).await;
                });
            }
            {
                let plugins_dir = core::paths::app_data_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("plugins");
                let host: std::sync::Arc<dyn gumballz_plugin_sdk::PluginHost> = std::sync::Arc::new(
                    plugin_host::PluginHostImpl::new(app.handle().clone(), plugins_dir),
                );
                let plugin_mgr = app
                    .handle()
                    .state::<std::sync::Arc<tokio::sync::RwLock<plugin_loader::PluginManager>>>();
                let mgr_for_plugins = std::sync::Arc::clone(&*plugin_mgr);
                let app_emit = app.handle().clone();
                std::thread::Builder::new()
                    .name("plugins-bootstrap".into())
                    .spawn(move || {
                        use tauri::Emitter;

                        // Load already-installed plugins first so they are
                        // usable immediately (and offline), without waiting
                        // on any of the network calls below. No builtin is
                        // auto-registered here anymore: a tool only appears
                        // once the user actually downloads its package.
                        {
                            let mut mgr = mgr_for_plugins.blocking_write();
                            mgr.load_all(std::sync::Arc::clone(&host));
                        }
                        let _ = app_emit.emit("plugins-changed", ());

                        let rt = match tokio::runtime::Runtime::new() {
                            Ok(rt) => rt,
                            Err(e) => {
                                tracing::warn!("plugins-bootstrap runtime failed: {}", e);
                                return;
                            }
                        };
                        rt.block_on(commands::plugins::auto_update_plugins(
                            std::sync::Arc::clone(&mgr_for_plugins),
                        ));

                        // Load anything newly installed above. load_all is
                        // not idempotent (re-inserting drops the previously
                        // loaded plugin and its dylib), so only load entries
                        // that are not loaded yet.
                        {
                            let mut mgr = mgr_for_plugins.blocking_write();
                            let to_load: Vec<String> = mgr
                                .installed_plugins()
                                .iter()
                                .filter(|p| p.enabled && !mgr.is_loaded(&p.id))
                                .map(|p| p.id.clone())
                                .collect();
                            for id in &to_load {
                                let _ = mgr.load_one(id, std::sync::Arc::clone(&host));
                            }
                        }
                        let _ = app_emit.emit("plugins-changed", ());
                    })
                    .ok();
            }

            let startup_app = app.handle().clone();
            std::thread::Builder::new()
                .name("startup-checks".into())
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("startup runtime");
                    rt.block_on(async {
                        // Ensures yt-dlp is present (zipapp when Python is
                        // available, otherwise the onefile build) before the
                        // first download asks for it.
                        if let Err(e) = core::ytdlp::ensure_ytdlp().await {
                            tracing::warn!("yt-dlp not available at startup: {}", e);
                        }
                        core::dependencies::ensure_js_runtime().await;
                        // First launch used to download only yt-dlp (and the
                        // Deno JS runtime): FFmpeg, gallery-dl and PDFium
                        // sat "Missing" until the user installed each by
                        // hand in Settings → Plugins. Download them now, in
                        // the background — anything already present is
                        // skipped, so later launches stay cheap.
                        commands::dependencies::auto_install_missing_dependencies(&startup_app)
                            .await;
                        let _ = tokio::task::spawn_blocking(
                            core::ytdlp::cleanup_stale_pyinstaller_dirs,
                        )
                        .await;
                        // `--update-to` is a full yt-dlp bootstrap plus
                        // network, and swapping the binary while a download is
                        // active breaks the process (B2). Wait for the app to
                        // settle and only run it with no yt-dlp in progress; if
                        // the queue is full, retry every 5 min for up to an
                        // hour.
                        tokio::time::sleep(std::time::Duration::from_secs(45)).await;
                        for _ in 0..12 {
                            let Some(ytdlp) = core::ytdlp::find_ytdlp_cached().await else {
                                break;
                            };
                            match core::ytdlp::check_ytdlp_update_detailed(&ytdlp).await {
                                Ok(core::ytdlp::UpdateCheck::Updated) => {
                                    tracing::info!("yt-dlp updated successfully");
                                    break;
                                }
                                Ok(core::ytdlp::UpdateCheck::UpToDate)
                                | Ok(core::ytdlp::UpdateCheck::AlreadyChecked) => {
                                    tracing::debug!("yt-dlp already up to date");
                                    break;
                                }
                                Ok(core::ytdlp::UpdateCheck::Busy) => {
                                    tracing::debug!(
                                        "yt-dlp update check deferred: downloads running"
                                    );
                                    tokio::time::sleep(std::time::Duration::from_secs(300))
                                        .await;
                                }
                                Err(e) => {
                                    tracing::warn!("yt-dlp update check failed: {}", e);
                                    break;
                                }
                            }
                        }
                    });
                })
                .ok();

            if let Some(url) = external_url::find_external_url_arg(std::env::args().skip(1)) {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) =
                        external_url::handle_external_url(&app_handle, url, "command-line").await
                    {
                        tracing::warn!("Failed to handle startup external URL: {}", error);
                    }
                });
            }

            if settings.start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            local_bridge::media_stream_url,
            commands::auth_webview::open_auth_webview,
            commands::bilibili_auth::bilibili_qr_generate,
            commands::bilibili_auth::bilibili_qr_poll,
            commands::bilibili_auth::bilibili_captcha_challenge,
            commands::bilibili_auth::bilibili_sms_send,
            commands::bilibili_auth::bilibili_sms_verify,
            commands::bilibili_auth::bilibili_account_status,
            commands::bilibili_auth::bilibili_import_watch_later,
            commands::bilibili_auth::bilibili_import_history,
            commands::bilibili_auth::bilibili_preview_info,
            commands::bilibili_auth::bilibili_webview_login,
            commands::browser_extension::browser_extension_status,
            commands::browser_extension::browser_extension_export,
            commands::browser_extension::browser_extension_open_folder,
            commands::browser_extension::browser_extension_prepare_install,
            cookies::commands::cookies_list,
            cookies::commands::cookies_read,
            cookies::commands::cookies_import,
            cookies::commands::cookies_clear,
            cookies::commands::cookies_clear_batch,
            cookies::commands::cookies_rename,
            cookies::commands::cookies_accounts_for_url,
            cookies::commands::cookies_read_as_json,
            cookies::commands::cookies_import_file,
            cookies::commands::cookies_export_to,
            cookies::commands::cookies_add_account,
            cookies::commands::cookies_set_bucket_enabled,
            cookies::commands::cookies_health,
            cookies::commands::cookies_test,
            commands::clip::clip_video,
            commands::reencode::reencode_video,
            commands::diagnostics::get_hwaccel_info,
            commands::diagnostics::diagnose_download_error,
            commands::downloads::detect_platform,
            commands::downloads::check_cookie_error,
            commands::downloads::validate_output_path,
            commands::downloads::get_media_formats,
            commands::downloads::prefetch_media_info,
            commands::downloads::download_from_url,
            commands::downloads::playlist_entries,
            commands::downloads::torrent_contents,
            commands::channels::channels_list,
            commands::channels::channel_add,
            commands::channels::channel_remove,
            commands::channels::channel_update,
            commands::channels::channel_check_now,
            commands::channels::sync_channels_tray,
            commands::tray::sync_tray_labels,
            commands::ai::ai_get_config,
            commands::ai::ai_set_config,
            commands::ai::ai_test,
            commands::ai::ai_summarize_url,
            commands::ai::whisper_generate,
            commands::ai::ai_history_list,
            commands::ai::ai_history_clear,
            commands::video_ops::video_op_silence_estimate,
            commands::video_ops::video_op_preset,
            commands::video_ops::video_op_propose,
            commands::video_ops::video_op_run,
            commands::video_ops::detect_shot_changes,
            commands::video_ops::waveform_peaks,
            commands::subtitle_ws::subtitle_load,
            commands::subtitle_ws::subtitle_save,
            commands::subtitle_ws::subtitle_translate,
            commands::subtitle_ws::subtitle_grammar_fix,
            commands::downloads::metadata_fetch,
            commands::downloads::thumbnails_list,
            commands::downloads::thumbnail_save,
            commands::downloads::subtitles_list,
            commands::downloads::subtitles_save,
            commands::downloads::subtitles_merge,
            commands::downloads::comments_fetch,
            commands::downloads::chapters_fetch,
            commands::downloads::tools_save_text,
            commands::downloads::livechat_fetch,
            commands::downloads::download_with_custom_args,
            commands::downloads::cancel_generic_download,
            commands::yt_templates::yt_templates_list,
            commands::yt_templates::yt_templates_save,
            commands::yt_templates::yt_templates_delete,
            commands::downloads::pause_download,
            commands::downloads::resume_download,
            commands::downloads::pause_all_downloads,
            commands::downloads::resume_all_downloads,
            commands::downloads::reorder_queue,
            commands::downloads::retry_download,
            commands::downloads::remove_download,
            commands::downloads::failed_download_leftovers,
            commands::downloads::update_max_concurrent,
            commands::downloads::clear_finished_downloads,
            commands::downloads::get_download_log,
            commands::downloads::get_download_command,
            commands::downloads::retry_download_with_command,
            commands::downloads::parse_batch_file,
            commands::downloads::get_recovery_items,
            commands::downloads::discard_recovery,
            commands::downloads::restore_recovery,
            commands::downloads::get_download_history,
            commands::downloads::clear_download_history,
            commands::downloads::reveal_file,
            commands::downloads::open_path_default,
            commands::host_queue::host_queue_enqueue_external,
            commands::host_queue::host_queue_report_progress,
            commands::host_queue::host_queue_report_complete,
            commands::integration::register_external_frontend,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::reset_settings,
            commands::settings::mark_onboarding_complete,
            commands::settings::mark_legal_acknowledged,
            commands::settings::get_bridge_info,
            commands::settings::rotate_bridge_token,
            commands::settings::bridge_open_pairing,
            commands::dependencies::check_dependencies,
            commands::spicetify::spicetify_status,
            commands::spicetify::spicetify_install,
            commands::spicetify::spicetify_action,
            commands::spicetify::spicetify_set_theme,
            commands::spicetify::spicetify_remove_addon,
            commands::spicetify::spicetify_install_marketplace,
            commands::tools::text::tool_humanize,
            commands::tools::ai::tool_ollama_status,
            commands::tools::ai::tool_ollama_recommended,
            commands::tools::ai::tool_ollama_pull,
            commands::tools::ai::tool_ollama_delete,
            commands::tools::ai::tool_pricing_info,
            commands::tools::ai::tool_pricing_search,
            commands::tools::ai::tool_pricing_for,
            commands::tools::ai::tool_usage_report,
            commands::tools::ai::tool_usage_clear,
            commands::tools::documents::tool_slideshare,
            commands::tools::documents::tool_gdocs_parse,
            commands::tools::documents::tool_gdocs_download,
            commands::tools::documents::tool_gslides_compile_dir,
            commands::tools::documents::tool_gslides_bridge_info,
            commands::tools::documents::tool_calameo,
            commands::tools::documents::tool_gallery_status,
            commands::tools::documents::tool_gallery_install,
            commands::tools::documents::tool_gallery_download,
            commands::tools::downloads::tool_aria2_status,
            commands::tools::downloads::tool_aria2_download,
            commands::tools::downloads::tool_manifest_download,
            commands::tools::files::tool_dupes_scan,
            commands::tools::files::tool_shred,
            commands::tools::files::tool_dupes_delete,
            commands::tools::files::tool_rename_plan,
            commands::tools::files::tool_rename_apply,
            commands::tools::files::tool_file_search_backend,
            commands::tools::files::tool_file_search,
            commands::tools::files::tool_awake_set,
            commands::tools::files::tool_awake_get,
            commands::tools::images::tool_upscale_status,
            commands::tools::images::tool_upscale_install,
            commands::tools::images::tool_upscale_run,
            commands::tools::images::tool_resize,
            commands::tools::images::tool_ocr_status,
            commands::tools::images::tool_ocr_run,
            commands::tools::images::tool_exif_read,
            commands::tools::images::tool_exif_strip,
            commands::tools::images::tool_icon_pack,
            commands::tools::images::tool_img_dupes,
            commands::tools::images::tool_img_compress,
            commands::tools::video::tool_video_compress,
            commands::tools::video::tool_video_restore,
            commands::tools::video::tool_video_gif,
            commands::tools::video::tool_video_silence,
            commands::tools::video::tool_subtitle_convert,
            commands::tools::video::tool_subtitle_burn,
            commands::tools::audio::tool_audio_clean,
            commands::tools::ctf::tool_ctf_hash,
            commands::tools::ctf::tool_ctf_hmac,
            commands::tools::ctf::tool_ctf_hash_id,
            commands::tools::ctf::tool_ctf_magic,
            commands::tools::ctf::tool_ctf_cipher,
            commands::tools::ctf::tool_ctf_caesar_brute,
            commands::tools::ctf::tool_ctf_xor,
            commands::tools::ctf::tool_ctf_encode,
            commands::tools::ctf::tool_ctf_detect,
            commands::tools::ctf::tool_ctf_freq,
            commands::tools::phone::tool_kde_status,
            commands::tools::phone::tool_kde_share,
            commands::tools::phone::tool_kde_ping,
            commands::tools::phone::tool_kde_refresh,
            commands::tools::speech::tool_whisper_status,
            commands::tools::speech::tool_whisper_install,
            commands::tools::speech::tool_whisper_model_download,
            commands::tools::speech::tool_whisper_model_remove,
            commands::tools::speech::tool_whisper_transcribe,
            commands::tools::speech::tool_tts_voices,
            commands::tools::speech::tool_tts_speak,
            commands::tools::speech::tool_srt_translate,
            commands::tools::speech::tool_dub,
            commands::tools::ai::tool_keys_kinds,
            commands::tools::ai::tool_keys_list,
            commands::tools::ai::tool_keys_save,
            commands::tools::ai::tool_keys_delete,
            commands::tools::ai::tool_keys_test,
            commands::tools::ai::tool_keys_balance,
            commands::tools::ai::tool_keys_models,
            commands::tools::ai::tool_keys_export,
            commands::tools::ai::tool_keys_use,
            commands::tools::ai::tool_mcp_status,
            commands::tools::ai::tool_mcp_set_enabled,
            commands::tools::ai::tool_mcp_selftest,
            commands::tools::desktop::tool_hotkeys_get,
            commands::tools::desktop::tool_hotkey_set,
            commands::tools::desktop::tool_autoclick_start,
            commands::tools::desktop::tool_autoclick_stop,
            commands::tools::desktop::tool_autoclick_state,
            commands::tools::desktop::tool_autoclick_mouse,
            commands::tools::desktop::tool_dictation_devices,
            commands::tools::desktop::tool_dictation_options,
            commands::tools::desktop::tool_dictation_set_options,
            commands::tools::desktop::tool_dictation_state,
            commands::tools::desktop::tool_dictation_start,
            commands::tools::desktop::tool_dictation_stop,
            commands::tools::desktop::tool_record_sources,
            commands::tools::desktop::tool_record_state,
            commands::tools::desktop::tool_record_start,
            commands::tools::desktop::tool_record_stop,
            commands::tools::desktop::tool_record_save_replay,
            commands::tools::desktop::tool_vs_status,
            commands::tools::desktop::tool_vs_launch,
            commands::tools::desktop::tool_vs_clone,
            commands::tools::desktop::tool_vs_design,
            commands::tools::desktop::tool_vs_isolate,
            commands::tools::pdf::tool_pdf_status,
            commands::tools::pdf::tool_pdf_info,
            commands::tools::pdf::tool_pdf_merge,
            commands::tools::pdf::tool_pdf_split,
            commands::tools::pdf::tool_pdf_render,
            commands::tools::pdf::tool_pdf_text,
            commands::tools::pdf::tool_pdf_from_images,
            commands::tools::pdf::tool_pdf_compress,
            commands::tools::pdf::tool_pdf_sanitize,
            commands::tools::pdf::tool_pdf_ocr,
            commands::tools::pdf::tool_pdf_office,
            commands::tools::pdf::tool_pdf_repair,
            commands::tools::pdf::tool_pdf_redaction_check,
            commands::tools::pdf::tool_pdf_password,
            commands::tools::pdf::tool_pdf_watermark,
            commands::tools::pdf::tool_pdf_crop,
            commands::tools::pdf::tool_pdf_outline_read,
            commands::tools::pdf::tool_pdf_outline_write,
            commands::tools::system::tool_win_tweaks_status,
            commands::tools::system::tool_win_tweak_apply,
            commands::tools::system::tool_clean_scan,
            commands::tools::system::tool_clean_run,
            commands::tools::system::tool_disk_volumes,
            commands::tools::system::tool_disk_scan,
            commands::tools::system::tool_disk_trash,
            commands::tools::system::tool_startup_list,
            commands::tools::system::tool_startup_set,
            commands::tools::system::tool_uninstall_list,
            commands::tools::system::tool_uninstall_leftovers,
            commands::tools::system::tool_uninstall_run,
            commands::tools::system::tool_debloat_list,
            commands::tools::system::tool_debloat_remove,
            commands::tools::system::tool_debloat_restore,
            commands::tools::system::tool_registry_scan,
            commands::tools::system::tool_registry_fix,
            commands::tools::system::tool_registry_backups_dir,
            commands::tools::system::tool_updater_status,
            commands::tools::system::tool_updater_upgrade,
            commands::tools::youtube::tool_sponsorblock,
            commands::tools::youtube::tool_ryd,
            commands::tools::youtube::tool_yt_video_id,
            commands::tools::youtube::tool_save_url,
            commands::tools::x::tool_x_session,
            commands::tools::x::tool_x_query_ids_refresh,
            commands::tools::x::tool_x_cancel,
            commands::tools::x::tool_x_post,
            commands::tools::x::tool_x_thread,
            commands::tools::x::tool_x_export_posts,
            commands::tools::x::tool_x_export_users,
            commands::tools::x::tool_x_render_posts,
            commands::tools::x::tool_x_profile,
            commands::tools::x::tool_x_profile_lookup,
            commands::tools::x::tool_x_media,
            commands::tools::x::tool_x_media_posts,
            commands::tools::x::tool_x_search,
            commands::tools::x::tool_x_trends,
            commands::tools::x::tool_x_bookmarks_export,
            commands::tools::x::tool_x_follows_audit,
            commands::tools::x::tool_x_unfollow,
            commands::tools::x::tool_x_whitelist_get,
            commands::tools::x::tool_x_whitelist_set,
            commands::tools::x::tool_x_archive_open,
            commands::tools::x::tool_x_archive_export,
            commands::tools::x::tool_x_grok_config,
            commands::tools::x::tool_x_grok_config_set,
            commands::tools::x::tool_x_grok_ask,
            commands::tools::x::tool_x_data_url,
            commands::tools::x::tool_x_save_data_url,
            commands::tools::x::tool_x_write_text,
            commands::tools::pinterest::tool_pin_inspect,
            commands::tools::pinterest::tool_pin_list,
            commands::tools::pinterest::tool_pin_related,
            commands::tools::pinterest::tool_pin_boards_search,
            commands::tools::pinterest::tool_pin_download,
            commands::tools::pinterest::tool_pin_download_many,
            commands::tools::pinterest::tool_pin_backup,
            commands::tools::pinterest::tool_pin_dupes,
            commands::tools::pinterest::tool_pin_unsave,
            commands::tools::pinterest::tool_pin_palette,
            commands::tools::pinterest::tool_pin_export,
            commands::tools::pinterest::tool_pin_keywords,
            commands::tools::pinterest::tool_pin_source,
            commands::tools::pinterest::tool_pin_expand,
            commands::tools::instagram::tool_ig_accounts,
            commands::tools::instagram::tool_ig_whoami,
            commands::tools::instagram::tool_ig_parse,
            commands::tools::instagram::tool_ig_cancel,
            commands::tools::instagram::tool_ig_post,
            commands::tools::instagram::tool_ig_resolve,
            commands::tools::instagram::tool_ig_download,
            commands::tools::instagram::tool_ig_download_bulk,
            commands::tools::instagram::tool_ig_profile,
            commands::tools::instagram::tool_ig_friendship,
            commands::tools::instagram::tool_ig_profile_media,
            commands::tools::instagram::tool_ig_stories,
            commands::tools::instagram::tool_ig_stories_tray,
            commands::tools::instagram::tool_ig_highlights,
            commands::tools::instagram::tool_ig_highlight_items,
            commands::tools::instagram::tool_ig_story_viewers,
            commands::tools::instagram::tool_ig_follow_lists,
            commands::tools::instagram::tool_ig_whitelist_get,
            commands::tools::instagram::tool_ig_whitelist_set,
            commands::tools::instagram::tool_ig_actions_today,
            commands::tools::instagram::tool_ig_actions,
            commands::tools::instagram::tool_ig_resolve_users,
            commands::tools::instagram::tool_ig_snapshot_take,
            commands::tools::instagram::tool_ig_snapshots,
            commands::tools::instagram::tool_ig_snapshot_diff,
            commands::tools::instagram::tool_ig_snapshot_delete,
            commands::tools::instagram::tool_ig_ghosts,
            commands::tools::instagram::tool_ig_export,
            commands::tools::instagram::tool_ig_write_csv,
            commands::tools::instagram::tool_ig_read_text,
            commands::tools::instagram::tool_ig_analytics,
            commands::tools::instagram::tool_ig_hashtag,
            commands::tools::instagram::tool_ig_comments,
            commands::tools::instagram::tool_ig_likers,
            commands::tools::instagram::tool_ig_giveaway,
            commands::tools::instagram::tool_ig_publish,
            commands::tools::instagram::tool_ig_publish_graph,
            commands::tools::instagram::tool_ig_schedule_list,
            commands::tools::instagram::tool_ig_schedule_add,
            commands::tools::instagram::tool_ig_schedule_remove,
            commands::dependencies::check_ytdlp_available,
            commands::dependencies::install_dependency,
            commands::dependencies::dependency_archived_versions,
            commands::diagnostics::flight_recorder_dump,
            commands::diagnostics::flight_recorder_clear,
            commands::diagnostics::preflight_batch,
            commands::dependencies::rollback_dependency,
            commands::dependencies::clear_dependency_path,
            commands::dependencies::dependency_custom_path,
            commands::rules::list_rules,
            commands::rules::save_rules,
            commands::rules::preview_rule_match,
            commands::media_history::check_media_changed,
            commands::media_history::record_media_snapshot,
            commands::dedupe::deduplicate_files,
            commands::dedupe::content_store_stats,
            commands::smart_speed::compute_silence_map,
            commands::smart_speed::silence_skip_target,
            commands::smart_speed::forget_silence_map,
            commands::torrent_playback::torrent_playback_readiness,
            commands::dependencies::dependency_variants,
            commands::dependencies::dependency_install_dir,
            commands::dependencies::set_dependency_path,
            commands::search::search_videos,
            commands::plugins::list_plugins,
            commands::plugins::get_plugin_frontend_path,
            commands::plugins::set_plugin_enabled,
            commands::plugins::uninstall_plugin,
            commands::plugins::get_loaded_plugin_manifests,
            commands::plugins::plugin_command,
            commands::plugin_tools::listening_studio_parse_subtitles,
            commands::plugin_tools::listening_studio_split_audio,
            commands::plugin_tools::listening_studio_export,
            commands::plugin_tools::asset_forge_convert,
            commands::plugins::fetch_marketplace_registry,
            commands::plugins::install_plugin_from_registry,
            commands::plugins::install_plugin_from_file,
            commands::plugins::install_plugin_from_bundle,
            commands::plugins::list_bundled_plugins,
            commands::plugins::get_plugin_i18n,
            commands::plugins::check_plugin_updates,
            commands::plugins::update_plugin,
            commands::p2p::p2p_send_file,
            commands::p2p::p2p_cancel_send,
            commands::p2p::p2p_pause_send,
            commands::p2p::p2p_resume_send,
            commands::app_lifecycle::force_exit_app,
            commands::app_lifecycle::get_debug_info,
            commands::app_lifecycle::get_portable_info,
            commands::tools::twitch::tool_tw_emotes,
            commands::tools::twitch::tool_tw_chat_replay,
            commands::tools::music::tool_music_playlist,
            commands::tools::music::tool_lyrics_sync,
            commands::tools::music::tool_music_history,
            commands::tools::reddit::tool_rd_download,
            commands::tools::reddit::tool_rd_thread,
            commands::tools::reddit::tool_rd_gdpr,
            commands::tools::images::tool_img_stitch,
            commands::tools::images::tool_img_sprite,
            commands::tools::bilibili::tool_bili_danmaku_export,
            commands::tools::bilibili::tool_bili_danmaku_burn,
            commands::tools::games::tool_switch_album,
            commands::tools::games::tool_clip_organizer,
            commands::tools::games::tool_protondb,
            commands::tools::system::tool_hosts_read,
            commands::tools::system::tool_hosts_apply,
            commands::tools::system::tool_hosts_restore,
            commands::tools::video::tool_sticker,
            commands::tools::pdf::tool_pdf_markdown,
            commands::tools::linkedin::tool_li_overview,
            commands::tools::linkedin::tool_li_connections,
            commands::tools::linkedin::tool_li_messages,
            commands::tools::linkedin::tool_li_checklist,
            commands::social_media::social_media_check_python_dependencies,
            commands::social_media::http::social_media_http_fetch,
            commands::social_media::http::social_media_cache_avatar,
            commands::social_media::http::social_media_resolve_facebook_avatar,
            commands::social_media::social_media_run_gallery_dl_download,
            commands::social_media::social_media_cancel_download,
            commands::social_media::social_media_save_app_settings,
            commands::social_media::social_media_load_app_settings,
            commands::social_media::social_media_load_profiles,
            commands::social_media::social_media_save_profile,
            commands::social_media::social_media_delete_profile,
            commands::social_media::social_media_get_history,
            commands::social_media::social_media_reconcile_disk,
            commands::social_media::social_media_delete_history_record,
            commands::social_media::social_media_pull_creator,
            commands::social_media::social_media_pull_all_creators,
            commands::social_media::social_media_detect_browsers,
            commands::social_media::social_media_extract_browser_cookies,
            commands::social_media::login::social_media_open_login_window,
            commands::social_media::login::social_media_login_status,
            commands::social_media::login::social_media_logout_platform,
            commands::social_media::browse::social_media_browse_friends,
            commands::social_media::browse::social_media_browse_posts,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = &event {
                let state = app_handle.state::<AppState>();
                let session_mutex = state.torrent_session.clone();
                tauri::async_runtime::block_on(async move {
                    let session_guard = session_mutex.lock().await;
                    let session = session_guard.as_ref().cloned();
                    drop(session_guard);
                    if let Some(session) = session {
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            session.stop(),
                        )
                        .await
                        {
                            Ok(()) => tracing::info!("torrent session stopped cleanly"),
                            Err(_) => tracing::warn!(
                                "torrent session stop timed out after 5s; exiting anyway"
                            ),
                        }
                    }
                });
            }
        });
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else if !dest_path.exists() {
            let _ = std::fs::copy(&path, &dest_path);
        }
    }
    Ok(())
}

fn migrate_legacy_omniget_data() {
    let Some(data_dir) = dirs::data_dir() else {
        return;
    };
    let legacy_dir = data_dir.join("com.nughnguyen.omniget");
    let target_dir = data_dir.join("com.gumballz.get");

    if !legacy_dir.exists() || !legacy_dir.is_dir() {
        return;
    }

    let _ = std::fs::create_dir_all(&target_dir);

    // If gumballz-get.db does not exist or is empty (<= 4096 bytes), adopt omniget.db
    let target_db = target_dir.join("gumballz-get.db");
    let legacy_db = legacy_dir.join("omniget.db");
    let should_migrate_db = !target_db.exists()
        || std::fs::metadata(&target_db)
            .map(|m| m.len() <= 4096)
            .unwrap_or(false);

    if should_migrate_db && legacy_db.exists() {
        tracing::info!("Migrating legacy omniget database to gumballz-get.db");
        let _ = std::fs::copy(&legacy_db, &target_db);
        let legacy_wal = legacy_dir.join("omniget.db-wal");
        let target_wal = target_dir.join("gumballz-get.db-wal");
        if legacy_wal.exists() {
            let _ = std::fs::copy(&legacy_wal, &target_wal);
        }
        let legacy_shm = legacy_dir.join("omniget.db-shm");
        let target_shm = target_dir.join("gumballz-get.db-shm");
        if legacy_shm.exists() {
            let _ = std::fs::copy(&legacy_shm, &target_shm);
        }
    }

    // Migrate settings, state files, and sessions
    let files = [
        "settings.json",
        "media-snapshots.json",
        "queue.wal",
        "telegram_plugin_state.json",
        "telegram.session",
        "study.db",
    ];
    for f in files {
        let src = legacy_dir.join(f);
        let dst = target_dir.join(f);
        if src.exists() && !dst.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
    }

    // Migrate cookies
    let src_cookies = legacy_dir.join("cookies");
    let dst_cookies = target_dir.join("cookies");
    if src_cookies.exists() && !dst_cookies.exists() {
        let _ = copy_dir_recursive(&src_cookies, &dst_cookies);
    }
}

fn terminate_legacy_omniget_process() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("pkill")
            .arg("-x")
            .arg("OmniGet")
            .output();
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "OmniGet.exe", "/T"])
            .output();
    }
}

fn cleanup_legacy_omniget_artifacts() {
    #[cfg(target_os = "linux")]
    {
        if let Some(data_dir) = dirs::data_dir() {
            let target = data_dir.join("applications/omniget-handler.desktop");
            if target.exists() {
                let _ = std::fs::remove_file(&target);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("pkill")
            .arg("-x")
            .arg("OmniGet")
            .output();

        let mut candidates = vec![std::path::PathBuf::from("/Applications/OmniGet.app")];
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join("Applications/OmniGet.app"));
        }

        for app_path in candidates {
            if app_path.exists() {
                if let Err(err) = std::fs::remove_dir_all(&app_path) {
                    tracing::warn!(
                        "Direct removal of {} failed: {}, attempting chflags unlock",
                        app_path.display(),
                        err
                    );
                    let _ = std::process::Command::new("chflags")
                        .args(["-R", "nouchg", &app_path.to_string_lossy()])
                        .output();
                    let _ = std::fs::remove_dir_all(&app_path);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(desktop) = dirs::desktop_dir() {
            let lnk = desktop.join("OmniGet.lnk");
            if lnk.exists() {
                let _ = std::fs::remove_file(&lnk);
            }
        }

        if let Some(roaming) = dirs::data_dir() {
            let start_menu = roaming.join(r"Microsoft\Windows\Start Menu\Programs");
            let lnk = start_menu.join("OmniGet.lnk");
            if lnk.exists() {
                let _ = std::fs::remove_file(&lnk);
            }
            let dir = start_menu.join("OmniGet");
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }

        if let Some(local_appdata) = dirs::data_local_dir() {
            let legacy_install = local_appdata.join("OmniGet");
            if legacy_install.exists() {
                let uninst = legacy_install.join("uninstall.exe");
                if uninst.exists() {
                    let _ = std::process::Command::new(&uninst).arg("/S").output();
                }
                let _ = std::fs::remove_dir_all(&legacy_install);
            }
        }
    }
}
