<!--
  GumballZ Get - Modern cross-platform download manager
  Copyright (C) 2026 Nguyen Quoc Hung (GumballZ) <https://github.com/nughnguyen/gumballz-get>

  This program is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.

  This program is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
  GNU General Public License for more details.

  You should have received a copy of the GNU General Public License
  along with this program. If not, see <https://www.gnu.org/licenses/>.
-->

<script lang="ts">
  import "../app.css";
  import "$lib/style/queue-kinds.css";
  import { page } from "$app/state";
  import { isMac } from "$lib/platform";
  import { isTauriDesktop } from "$lib/core/platform";
  import WebDownloadHub from "$components/web/WebDownloadHub.svelte";
  import { ENABLED_PLUGINS } from "$lib/plugins/registry";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { initDownloadListener } from "$lib/stores/download-listener";
  import { getCounts } from "$lib/stores/download-store.svelte";
  import { getSettings, loadSettings } from "$lib/stores/settings-store.svelte";
  import { queueExternalPrefill, type ExternalUrlEvent } from "$lib/stores/external-url-store.svelte";
  import Toast from "$components/toast/Toast.svelte";
  import AppSidebar from "$components/shell/AppSidebar.svelte";
  import AppToolbar from "$components/shell/AppToolbar.svelte";
  import CommandPalette from "$components/shell/CommandPalette.svelte";
  import GlobalDownloadBar from "$components/download/GlobalDownloadBar.svelte";
  import { setCommandPaletteItems } from "$lib/stores/command-palette-store.svelte";
  import {
    refreshUpdateInfo,
    getUpdateInfo,
    isUpdateDismissed,
    dismissUpdate,
    getIsUpdating,
    performUpdate,
    skipUpdate,
  } from "$lib/stores/update-store.svelte";
  import { startClipboardMonitor, stopClipboardMonitor, onClipboardUrl } from "$lib/stores/clipboard-monitor";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { initChangelog } from "$lib/stores/changelog-store.svelte";
  import { checkAnnouncements } from "$lib/stores/announcement-store.svelte";
  import { needsOnboarding } from "$lib/stores/onboarding-store.svelte";
  import { isYtdlpAvailable, isDepsChecked, refreshYtdlpStatus } from "$lib/stores/dependency-store.svelte";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { t, locale, isRtlLocale, loadTranslations } from "$lib/i18n";
  import { syncTrayLabels } from "$lib/tray-labels";
  import { get } from "svelte/store";
  import { CORE_NAV_ITEMS, pluginIconForRoute, type NavItem } from "$lib/nav-config";
  import { TOOLS, toolHref } from "$lib/tools/catalog";
  import type { Snippet } from "svelte";
  import type { Component } from "svelte";

  let pluginNavItems = $state<NavItem[]>([]);
  let desktopMode = $state(isTauriDesktop());

  let coreNavItems = $derived(CORE_NAV_ITEMS);

  let allNav = $derived([...coreNavItems, ...pluginNavItems].sort((a, b) => (a.order ?? 50) - (b.order ?? 50)));
  let primaryNav = $derived(allNav.filter((item) => item.group === "primary"));
  let appNav = $derived(allNav.filter((item) => item.group === "app"));
  let pluginNav = $derived(allNav.filter((item) => item.group === "plugins"));

  let ytdlpDismissed = $state(false);
  let ytdlpMissing = $derived(isDepsChecked() && !isYtdlpAvailable());
  let showOnboarding = $derived(needsOnboarding());

  let updateInfo = $derived(getUpdateInfo());
  let updateDismissed = $derived(isUpdateDismissed());
  let isUpdating = $derived(getIsUpdating());

  let counts = $derived(getCounts());
  let badgeLabel = $derived(counts.badge > 99 ? "99+" : String(counts.badge));
  let settings = $derived(getSettings());

  let isFlushRoute = $derived(
    page.url.pathname === "/" ||
    page.url.pathname.startsWith("/downloads") ||
    page.url.pathname.startsWith("/settings"),
  );

  let DebugPanel = $state<any>(null);
  let OnboardingWizard = $state<any>(null);
  let ChangelogDialog = $state<any>(null);
  let ConfirmCloseDialog = $state<any>(null);
  let ShortcutsDialog = $state<any>(null);
  let LegalDialog = $state<any>(null);
  let RecoveryDialog = $state<any>(null);
  let AnnouncementDialog = $state<any>(null);

  function handleExternalUrlEvent(event: Omit<ExternalUrlEvent, "id">) {
    if (event.action === "prefill") {
      queueExternalPrefill(event);
      showToast("info", $t("toast.external_url_ready"));
      if (page.url.pathname !== "/") {
        goto("/");
      }
      return;
    }

    if (event.action === "queued") {
      showToast("success", $t("toast.external_url_queued"));
    }
  }

  function reloadPluginNav() {
    const builtInItems: NavItem[] = ENABLED_PLUGINS.map((plugin, index) => ({
      href: plugin.route,
      label: plugin.name,
      icon: "plugin",
      iconSvg: plugin.iconSvg,
      group: "plugins",
      pluginId: plugin.id,
      order: 60 + index,
    }));
    invoke<{ id: string; enabled: boolean; nav: { route: string; label: Record<string, string>; icon_svg: string | null; group: string; order: number }[] }[]>("list_plugins")
      .then((plugins) => {
        const items: NavItem[] = [];
        for (const p of plugins) {
          if (!p.enabled) continue;
          for (const n of p.nav) {
            const icon = pluginIconForRoute(n.route);
            items.push({
              href: n.route,
              label: n.label[get(locale)] || n.label["en"] || p.id,
              icon,
              iconSvg: icon === "plugin" ? n.icon_svg || undefined : undefined,
              group: "plugins",
              pluginId: p.id,
              order: n.order,
            });
          }
        }
        const enabledIds = new Set(plugins.filter((plugin) => plugin.enabled).map((plugin) => plugin.id));
        pluginNavItems = [...builtInItems.filter((plugin) => !enabledIds.has(plugin.pluginId ?? "")), ...items];
        buildCommandPaletteItems();
      })
      .catch(() => { pluginNavItems = builtInItems; });
  }

  onMount(() => {
    desktopMode = isTauriDesktop();
    if (!desktopMode) return;
    initDownloadListener();
    // If `get_settings` failed while the shell was booting, Settings spins forever.
    // Retry a few times instead of leaving the app half-configured until the next restart.
    if (!getSettings()) {
      let attempts = 0;
      const retry = () => {
        if (getSettings() || attempts >= 5) return;
        attempts += 1;
        loadSettings()
          .then(async (s) => {
            reloadPluginNav();
            const target = s?.appearance?.language;
            if (target && target !== get(locale)) {
              try {
                await loadTranslations(target, page.url.pathname);
                locale.set(target);
                await syncTrayLabels();
              } catch {}
            }
          })
          .catch(() => setTimeout(retry, 1000 * attempts));
      };
      setTimeout(retry, 500);
    }

    if (import.meta.env.DEV) {
      import("$components/debug/DebugPanel.svelte").then((m) => {
        DebugPanel = m.default;
      });
    }

    import("$components/onboarding/OnboardingWizard.svelte").then((m) => {
      OnboardingWizard = m.default;
    });
    import("$components/dialog/ChangelogDialog.svelte").then((m) => {
      ChangelogDialog = m.default;
    });
    import("$components/dialog/ConfirmCloseDialog.svelte").then((m) => {
      ConfirmCloseDialog = m.default;
    });
    import("$components/dialog/ShortcutsDialog.svelte").then((m) => {
      ShortcutsDialog = m.default;
    });
    import("$components/dialog/LegalDialog.svelte").then((m) => {
      LegalDialog = m.default;
    });
    import("$components/dialog/RecoveryDialog.svelte").then((m) => {
      RecoveryDialog = m.default;
    });
    import("$components/dialog/AnnouncementDialog.svelte").then((m) => {
      AnnouncementDialog = m.default;
    });

    refreshYtdlpStatus();
    refreshUpdateInfo();
    initChangelog();
    setTimeout(() => {
      void checkAnnouncements();
    }, 2000);
    reloadPluginNav();

    let unlistenExternalUrl: (() => void) | null = null;
    let unlistenPlugins: (() => void) | null = null;

    listen<Omit<ExternalUrlEvent, "id">>("external-url-event", (event) => {
      handleExternalUrlEvent(event.payload);
    }).then((un) => {
      unlistenExternalUrl = un;
    });

    listen("plugins-changed", () => {
      reloadPluginNav();
    }).then((un) => {
      unlistenPlugins = un;
    });

    return () => {
      if (unlistenExternalUrl) unlistenExternalUrl();
      if (unlistenPlugins) unlistenPlugins();
    };
  });

  function buildCommandPaletteItems() {
    setCommandPaletteItems([
      {
        id: "nav-home",
        label: get(t)("nav.home"),
        group: get(t)("command_palette.group_nav"),
        keywords: "index main prefill",
        action: () => goto("/"),
      },
      {
        id: "nav-downloads",
        label: get(t)("nav.downloads"),
        group: get(t)("command_palette.group_nav"),
        keywords: "queue active history",
        action: () => goto("/downloads"),
      },
      {
        id: "nav-settings",
        label: get(t)("nav.settings"),
        group: get(t)("command_palette.group_nav"),
        keywords: "preferences options config",
        action: () => goto("/settings"),
      },
      {
        id: "nav-tools",
        label: get(t)("nav.tools"),
        group: get(t)("command_palette.group_nav"),
        keywords: "ferramentas tools utilities apps",
        action: () => goto("/tools"),
      },
      // Cada ferramenta do catálogo entra na paleta com as mesmas
      // palavras-chave da busca do hub, então ⌘K acha "instagram" também.
      ...TOOLS.map((tool) => ({
        id: `tool-${tool.id}`,
        label: get(t)(`tools.catalog.${tool.id}.name`),
        group: get(t)("tools.hub.title"),
        keywords: [...tool.keywords, get(t)(`tools.categories.${tool.category}.name`)].join(" "),
        action: () => goto(toolHref(tool)),
      })),
      {
        id: "nav-marketplace",
        label: get(t)("nav.marketplace"),
        group: get(t)("command_palette.group_nav"),
        keywords: "plugins extensions store",
        action: () => goto("/marketplace"),
      },
      {
        id: "nav-about",
        label: get(t)("nav.about"),
        group: get(t)("command_palette.group_nav"),
        keywords: "info version changelog project",
        action: () => goto("/about"),
      },
      // The three former "action" entries ran the same goto() as the
      // navigation entries above them, so the palette listed Downloads and
      // Settings twice and Home three times. Only the paste action does
      // something navigation cannot — `command_palette.action_paste` was
      // already translated in every locale, just never wired up.
      {
        id: "action-paste",
        label: get(t)("command_palette.action_paste"),
        group: get(t)("command_palette.group_action"),
        keywords: "url link add download clipboard prefill",
        action: async () => {
          const text = (await readText().catch(() => null))?.trim();
          if (!text) {
            showToast("info", get(t)("toast.clipboard_empty") as string);
            return;
          }
          queueExternalPrefill({ action: "prefill", url: text, source: "clipboard" });
          goto("/");
        },
      },
    ]);
  }

  $effect(() => {
    if (!desktopMode) return;
    if (settings?.download.clipboard_detection) {
      onClipboardUrl((clipboardUrl) => {
        queueExternalPrefill({ action: "prefill", url: clipboardUrl, source: "clipboard" });
        showToast("info", $t("toast.clipboard_detected") as string);
        if (page.url.pathname !== "/") {
          goto("/");
        }
      });
      startClipboardMonitor();
    } else {
      onClipboardUrl(null);
      stopClipboardMonitor();
    }
    return () => {
      onClipboardUrl(null);
      stopClipboardMonitor();
    };
  });

  // The startup thread downloads yt-dlp in the background, so the first
  // check (on mount) usually runs before the binary lands. Re-check every
  // few seconds while it is still missing, for up to two minutes, so the
  // banner clears on its own once the download finishes.
  $effect(() => {
    if (!desktopMode || !ytdlpMissing) return;
    let polls = 0;
    const id = setInterval(() => {
      polls += 1;
      if (polls >= 24) {
        clearInterval(id);
        return;
      }
      void refreshYtdlpStatus();
    }, 5000);
    return () => clearInterval(id);
  });

  $effect(() => {
    document.documentElement.setAttribute("data-shell", "mac");
    // O shell é o mesmo em todas as plataformas, mas os controles de janela
    // não: o macOS põe fechar/minimizar à esquerda (sobre a sidebar, titlebar
    // overlay), Windows e Linux à direita. Sem data-platform o app reservava
    // espaço morto e encostava a busca no botão de fechar.
    document.documentElement.setAttribute("data-platform", isMac() ? "macos" : "other");
    document.documentElement.setAttribute("dir", isRtlLocale($locale) ? "rtl" : "ltr");
    document.documentElement.setAttribute("lang", $locale || "en");
    buildCommandPaletteItems();
  });

  $effect(() => {
    if (!desktopMode) return;
    const targetLang = settings?.appearance?.language;
    if (targetLang && targetLang !== $locale) {
      void (async () => {
        try {
          await loadTranslations(targetLang, page.url.pathname);
          locale.set(targetLang);
          await syncTrayLabels();
        } catch (e) {
          console.warn(`[i18n] could not sync locale to "${targetLang}":`, e);
        }
      })();
    }
  });

  let { children }: { children: Snippet } = $props();

  const VACUUM_LAST_RUN_KEY = "gumballz-get.library.auto_vacuum.last_run";

  async function checkAutoVacuum() {
    try {
      const now = Date.now();
      const lastRunStr = localStorage.getItem(VACUUM_LAST_RUN_KEY);
      const lastRun = lastRunStr ? parseInt(lastRunStr, 10) : 0;

      if (now - lastRun > 7 * 24 * 60 * 60 * 1000) {
        await invoke("db_vacuum");
        localStorage.setItem(VACUUM_LAST_RUN_KEY, String(now));
      }
    } catch {}
  }

  const SIDEBAR_KEY = "gumballz-get.sidebar.collapsed";
  let sidebarCollapsed = $state(
    typeof localStorage === "undefined"
      ? false
      : localStorage.getItem(SIDEBAR_KEY) === "1",
  );

  $effect(() => {
    document.documentElement.setAttribute("data-sidebar", sidebarCollapsed ? "collapsed" : "expanded");
  });

  function toggleSidebar() {
    sidebarCollapsed = !sidebarCollapsed;
    try {
      localStorage.setItem(SIDEBAR_KEY, sidebarCollapsed ? "1" : "0");
    } catch {}
  }

  function onSidebarShortcut(e: KeyboardEvent) {
    if (!desktopMode) return;
    if (e.key.toLowerCase() !== "s" || e.altKey) return;
    if ((e.ctrlKey || e.metaKey) && e.shiftKey) {
      e.preventDefault();
      toggleSidebar();
    }
  }

  onMount(() => {
    if (!desktopMode) return;
    void checkAutoVacuum();
  });
</script>

<svelte:window onkeydown={onSidebarShortcut} />
{#if desktopMode}

<div class="shell" data-reduce-motion={settings?.accessibility?.reduce_motion} data-reduce-transparency={settings?.accessibility?.reduce_transparency}>
  <AppSidebar
    {primaryNav}
    {appNav}
    {pluginNav}
    {badgeLabel}
    badgeCount={counts.badge}
    collapsed={sidebarCollapsed}
    onToggleCollapsed={toggleSidebar}
  />

  <div class="shell-body">
    <AppToolbar />

    {#if updateInfo.available && !updateDismissed}
      <div class="global-update-banner" role="alert">
        <span class="global-update-icon" aria-hidden="true">
          <svg
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M12 3v12" />
            <path d="m7 11 5 5 5-5" />
            <path d="M4 21h16" />
          </svg>
        </span>
        <span class="global-update-text">
          {#if isUpdating}
            {$t("settings.update_downloading")}
          {:else}
            {$t("settings.update_available", { version: updateInfo.version })}
          {/if}
        </span>
        <div class="global-update-actions">
          <button
            type="button"
            class="global-update-btn"
            onclick={performUpdate}
            disabled={isUpdating}
          >
            {#if isUpdating}
              <span class="global-update-spinner"></span>
            {:else}
              {$t("settings.update_button")}
            {/if}
          </button>
          <button
            type="button"
            class="global-update-skip"
            onclick={skipUpdate}
            disabled={isUpdating}
          >
            {$t("settings.update_skip")}
          </button>
          <button
            type="button"
            class="global-update-close"
            onclick={dismissUpdate}
            aria-label={$t("ytdlp_missing_banner.dismiss") as string}
          >
            ✕
          </button>
        </div>
      </div>
    {/if}

    {#if ytdlpMissing && !ytdlpDismissed}
      <div class="ytdlp-banner" role="alert">
        <span class="ytdlp-banner-text">
          {$t("ytdlp_missing_banner.text")}
        </span>
        <div class="ytdlp-banner-actions">
          <a href="/settings#dependencies" class="button ytdlp-banner-link">
            {$t("ytdlp_missing_banner.open_settings")}
          </a>
          <button
            type="button"
            class="ytdlp-banner-close"
            onclick={() => (ytdlpDismissed = true)}
            aria-label={$t("ytdlp_missing_banner.dismiss") as string}
          >
            ✕
          </button>
        </div>
      </div>
    {/if}

    <main id="main-content" class="content">
      <div class="mac-pane" class:mac-pane--flush={isFlushRoute}>
        <div class="mac-pane-scroll core-shell" class:core-shell--flush={isFlushRoute}>
          {@render children()}
        </div>
      </div>
      <GlobalDownloadBar />
    </main>
  </div>
</div>
{:else}
  <WebDownloadHub />
{/if}

<Toast />
<CommandPalette />

{#if showOnboarding && OnboardingWizard}
  <OnboardingWizard />
{/if}

{#if DebugPanel}
  <DebugPanel />
{/if}

{#if ChangelogDialog}
  <ChangelogDialog />
{/if}

{#if ConfirmCloseDialog}
  <ConfirmCloseDialog />
{/if}

{#if ShortcutsDialog}
  <ShortcutsDialog />
{/if}

{#if LegalDialog}
  <LegalDialog />
{/if}

{#if RecoveryDialog}
  <RecoveryDialog />
{/if}

{#if AnnouncementDialog}
  <AnnouncementDialog />
{/if}

<style>
  .shell {
    display: flex;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
  }

  .shell-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100vh;
    overflow: hidden;
  }

  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .core-shell {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    width: 100%;
    margin: 0;
    overflow-y: auto;
    overscroll-behavior-y: contain;
    transform: translateZ(0);
    will-change: scroll-position;
  }

  .core-shell--flush {
    padding: 0;
    overflow: hidden;
  }

  .global-update-banner {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3);
    margin-block: 0 var(--space-3);
    margin-inline: 0 var(--pane-inset);
    padding: var(--space-3) var(--space-4);
    background: var(--pane-bg);
    color: var(--text);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-lg);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    font-size: var(--text-base);
  }

  .global-update-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    background: var(--brand-cyan);
    color: #0f172a;
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .global-update-text {
    flex: 1;
    min-width: 0;
    font-family: var(--font-display);
    font-size: var(--text-md);
    font-weight: 600;
    letter-spacing: var(--track-snug);
  }

  .global-update-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .global-update-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    font-size: var(--text-base);
    padding: 0 var(--space-4);
    border-radius: var(--radius-md);
    background: var(--brand-cyan);
    color: #0f172a;
    border: 2px solid var(--border-ink);
    box-shadow: 3px 3px 0 var(--shadow-hard);
    font-weight: 700;
    cursor: pointer;
  }

  .global-update-btn:disabled {
    cursor: default;
    background: var(--surface-hi);
    color: var(--text-faint);
    box-shadow: none;
  }

  @media (hover: hover) {
    .global-update-btn:not(:disabled):hover {
      transform: translate(-1px, -1px);
      box-shadow: 4px 4px 0 var(--shadow-hard);
    }
  }

  .global-update-btn:not(:disabled):active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0 var(--shadow-hard);
  }

  .global-update-btn:focus-visible {
    outline: 2px solid var(--brand-cyan);
    outline-offset: 2px;
  }

  .global-update-skip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 32px;
    font-size: var(--text-base);
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    background: transparent;
    border: 2px solid var(--border-ink);
    color: var(--text);
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }

  .global-update-skip:disabled {
    cursor: default;
    background: var(--surface-hi);
    color: var(--text-faint);
  }

  @media (hover: hover) {
    .global-update-skip:not(:disabled):hover {
      background: color-mix(in srgb, var(--text) 8%, transparent);
    }
  }

  .global-update-skip:not(:disabled):active {
    transform: translate(2px, 2px);
  }

  .global-update-skip:focus-visible {
    outline: 2px solid var(--brand-cyan);
    outline-offset: 2px;
  }

  .global-update-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: transparent;
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    color: var(--text);
    cursor: pointer;
    padding: 0;
  }

  @media (hover: hover) {
    .global-update-close:hover {
      background: color-mix(in srgb, var(--text) 8%, transparent);
    }
  }

  .global-update-close:active {
    transform: translate(2px, 2px);
  }

  .global-update-spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(15, 23, 42, 0.3);
    border-top-color: #0f172a;
    border-radius: var(--radius-full);
    animation: spin var(--duration-slow) linear infinite;
    flex-shrink: 0;
  }

  .ytdlp-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    row-gap: var(--space-2);
    gap: var(--space-3);
    margin-block: 0 var(--space-2);
    margin-inline: 0 var(--pane-inset);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    background: color-mix(in srgb, var(--warning) 16%, var(--pane-bg));
    color: var(--text);
    border-radius: var(--radius-lg);
    box-shadow: inset 0 0 0 var(--hairline) color-mix(in srgb, var(--warning) 40%, transparent);
    font-size: var(--text-base);
  }

  .ytdlp-banner-text {
    flex: 1;
    min-width: 0;
    font-weight: 500;
  }

  .ytdlp-banner-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .ytdlp-banner-link {
    display: inline-flex;
    align-items: center;
    height: 24px;
    font-size: var(--text-sm);
    padding: 0 var(--space-3);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-sm);
    background: var(--cta);
    color: var(--on-cta);
    text-decoration: none;
    font-weight: 600;
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  @media (hover: hover) {
    .ytdlp-banner-link:hover {
      background: var(--cta-hover);
    }
  }

  .ytdlp-banner-link:active {
    background: var(--cta-press);
  }

  .ytdlp-banner-link:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  .ytdlp-banner-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    padding: 0;
  }

  @media (hover: hover) {
    .ytdlp-banner-close:hover {
      background: var(--fill-2);
      color: var(--text);
    }
  }
</style>
