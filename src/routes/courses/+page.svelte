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
  import { goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import { pluginInvoke } from "$lib/plugin-invoke";
  import { onMount } from "svelte";
  import { PLATFORM_ICONS, DEFAULT_ICON } from "./platform-icons";
  import { t } from "$lib/i18n";

  type PlatformConfig = {
    id: string;
    name: string;
    color: string;
    icon: string;
    commands: { check_session: string };
  };

  const FALLBACK_PLATFORMS: PlatformConfig[] = [
    { id: "hotmart", name: "Hotmart", color: "#F04E23", icon: "hotmart", commands: { check_session: "hotmart_check_session" } },
    { id: "udemy", name: "Udemy", color: "#A435F0", icon: "udemy", commands: { check_session: "udemy_check_session" } },
    { id: "kiwify", name: "Kiwify", color: "#22C55E", icon: "kiwify", commands: { check_session: "kiwify_check_session" } },
    { id: "rocketseat", name: "Rocketseat", color: "#8257E5", icon: "rocketseat", commands: { check_session: "rocketseat_check_session" } },
    { id: "metaanalysis", name: "Meta-Analysis Academy", color: "#4338CA", icon: "metaanalysis", commands: { check_session: "metaanalysis_check_session" } },
  ];

  type SessionState = "checking" | "connected" | "not-connected" | "expired" | "unreachable" | "missing";

  function errorMessage(e: unknown): string {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message;
    if (e && typeof e === "object" && "message" in e && typeof e.message === "string") return e.message;
    return String(e);
  }

  export function classifySessionError(msg: string): SessionState {
    const lower = msg.toLowerCase();
    if (
      lower.startsWith("err_plugin_outdated") ||
      lower.includes("unknown command") ||
      (lower.includes("command") && lower.includes("not found")) ||
      lower.includes("no handler")
    ) return "missing";
    if (lower.includes("session_expired")) return "expired";
    if (lower.includes("not_authenticated") || lower.includes("not authenticated")) return "not-connected";
    return "unreachable";
  }

  type PluginStatus =
    | "checking"
    | "ready"
    | "not-installed"
    | "needs-restart"
    | "incompatible"
    | "load-failed";
  type PluginLoadError = {
    message: string;
    kind: string;
    plugin_abi?: number | null;
    expected_abi?: number | null;
  };
  let pluginStatus = $state<PluginStatus>("checking");
  let loadError = $state<PluginLoadError | null>(null);

  let platforms: PlatformConfig[] = $state([]);
  let searchQuery = $state("");
  let authStatus: Record<string, { state: SessionState; email: string | null; detail: string }> = $state({});
  let usingFallback = $state(false);

  let visiblePlatforms = $derived(platforms.filter((p) => authStatus[p.id]?.state !== "missing"));
  let hiddenCount = $derived(platforms.length - visiblePlatforms.length);

  let filteredPlatforms = $derived(
    searchQuery.trim() === ""
      ? visiblePlatforms
      : visiblePlatforms.filter((p) =>
          p.name.toLowerCase().includes(searchQuery.trim().toLowerCase())
        )
  );

  onMount(async () => {
    console.log("[courses] onMount start");
    try {
      const plugins = await invoke<{
        id: string;
        enabled: boolean;
        loaded: boolean;
        load_error?: PluginLoadError | null;
      }[]>("list_plugins");
      console.log("[courses] list_plugins:", JSON.stringify(plugins));
      const courses = plugins.find((p) => p.id === "courses");
      if (!courses || !courses.enabled) {
        console.log("[courses] plugin not installed or disabled");
        pluginStatus = "not-installed";
        return;
      }
      if (!courses.loaded) {
        if (courses.load_error) {
          loadError = courses.load_error;
          pluginStatus =
            courses.load_error.kind === "abi_mismatch" ||
            courses.load_error.kind === "missing_abi_symbol"
              ? "incompatible"
              : "load-failed";
          console.log("[courses] plugin load failed:", courses.load_error);
        } else {
          console.log("[courses] plugin enabled but not loaded — needs restart");
          pluginStatus = "needs-restart";
        }
        return;
      }
      console.log("[courses] plugin ready");
      pluginStatus = "ready";
    } catch (e) {
      console.error("[courses] list_plugins failed:", e);
      pluginStatus = "ready";
    }

    try {
      console.log("[courses] calling get_platforms...");
      platforms = await pluginInvoke<PlatformConfig[]>("courses", "get_platforms");
      console.log("[courses] get_platforms returned:", platforms.length, "platforms", JSON.stringify(platforms.map(p => p.id)));
    } catch (e) {
      console.error("[courses] get_platforms FAILED, using fallback:", e);
      platforms = FALLBACK_PLATFORMS;
      usingFallback = true;
    }

    for (const platform of platforms) {
      authStatus[platform.id] = { state: "checking", email: null, detail: "" };
      if (!platform.commands?.check_session) {
        authStatus[platform.id] = { state: "missing", email: null, detail: "" };
        continue;
      }
      pluginInvoke<string>("courses", platform.commands.check_session)
        .then((email) => {
          authStatus[platform.id] = { state: "connected", email, detail: "" };
        })
        .catch((e) => {
          const detail = errorMessage(e);
          const state = classifySessionError(detail);
          console.log(`[courses] ${platform.id} session: ${state}`, detail);
          authStatus[platform.id] = { state, email: null, detail };
        });
    }
  });

  function handleCardClick(platform: PlatformConfig) {
    goto(`/courses/${platform.id}`);
  }

  function handleKeyDown(e: KeyboardEvent, platform: PlatformConfig) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleCardClick(platform);
    }
  }

  function getIconSvg(icon: string): string {
    return PLATFORM_ICONS[icon] ?? DEFAULT_ICON;
  }

  function statusClass(state: SessionState | undefined): string {
    if (state === "connected") return "st-connected";
    if (state === "expired") return "st-expired";
    if (state === "unreachable") return "st-error";
    if (state === "not-connected") return "st-off";
    return "";
  }
</script>

{#if pluginStatus === "checking"}
  <div class="plugin-guard"><span class="spinner"></span></div>
{:else if pluginStatus === "not-installed"}
  <div class="plugin-guard">
    <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M4 19.5A2.5 2.5 0 016.5 17H20" />
      <path d="M6.5 2H20v20H6.5A2.5 2.5 0 014 19.5v-15A2.5 2.5 0 016.5 2z" />
    </svg>
    <h2>{$t("marketplace.plugin_not_installed")}</h2>
    <p>{$t("marketplace.plugin_install_hint")}</p>
    <a href="/marketplace" class="guard-link">{$t("marketplace.go_to_marketplace")}</a>
  </div>
{:else if pluginStatus === "needs-restart"}
  <div class="plugin-guard">
    <h2>{$t("marketplace.restart_required")}</h2>
    <p>{$t("marketplace.plugin_restart_hint")}</p>
  </div>
{:else if pluginStatus === "incompatible"}
  <div class="plugin-guard">
    <h2>{$t("marketplace.plugin_incompatible_title")}</h2>
    <p>{$t("marketplace.plugin_incompatible_hint")}</p>
    <a href="/marketplace" class="guard-link">{$t("marketplace.go_to_marketplace")}</a>
    {#if loadError}
      <p class="guard-detail"><code>{loadError.message}</code></p>
    {/if}
  </div>
{:else if pluginStatus === "load-failed"}
  <div class="plugin-guard">
    <h2>{$t("marketplace.plugin_load_failed_title")}</h2>
    <p>{$t("marketplace.plugin_load_failed_hint")}</p>
    <a href="/marketplace" class="guard-link">{$t("marketplace.go_to_marketplace")}</a>
    {#if loadError}
      <p class="guard-detail"><code>{loadError.message}</code></p>
    {/if}
  </div>
{:else}
<div class="courses-page">
  <header class="courses-head">
    <div class="courses-hero-tile">
      <img src="/icons/nav/courses.svg" alt="" class="courses-hero-icon" width="40" height="40" draggable="false" />
    </div>
    <div class="courses-identity">
      <h1 class="page-title">{$t("courses.title")}</h1>
      <p class="courses-subtitle">{$t("courses.subtitle")}</p>
    </div>
  </header>

  {#if usingFallback}
    <div class="fallback-banner">
      {$t("courses.update_plugin_hint")}
    </div>
  {:else if hiddenCount > 0}
    <div class="fallback-banner">
      {$t("courses.platform_hidden_hint")}
    </div>
  {/if}

  <div class="search-wrap">
    <svg class="search-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <circle cx="11" cy="11" r="7" />
      <line x1="21" y1="21" x2="16.5" y2="16.5" />
    </svg>
    <input
      class="search-input"
      type="text"
      placeholder={$t("courses.search_placeholder")}
      bind:value={searchQuery}
    />
  </div>

  <div class="platform-grid">
    {#each filteredPlatforms as platform (platform.id)}
      <div
        class="platform-card"
        role="button"
        tabindex={0}
        onclick={() => handleCardClick(platform)}
        onkeydown={(e) => handleKeyDown(e, platform)}
      >
        <div class="card-icon" style:--platform-color="{platform.color}">
          <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            {@html getIconSvg(platform.icon)}
          </svg>
        </div>
        <span class="card-name">{platform.name}</span>
        <span class="card-status {statusClass(authStatus[platform.id]?.state)}" title={authStatus[platform.id]?.state === "unreachable" ? authStatus[platform.id].detail : undefined}>
          {#if authStatus[platform.id]?.state === "connected"}
            <span class="status-dot connected"></span>
            <span class="status-email">{authStatus[platform.id].email}</span>
          {:else if authStatus[platform.id]?.state === "expired"}
            <span class="status-dot expired"></span>
            {$t("courses.session_expired")}
          {:else if authStatus[platform.id]?.state === "unreachable"}
            <span class="status-dot error"></span>
            {$t("courses.connection_failed")}
          {:else if authStatus[platform.id]?.state === "not-connected"}
            <span class="status-dot disconnected"></span>
            {$t("courses.not_connected")}
          {/if}
        </span>
      </div>
    {/each}
  </div>

  {#if filteredPlatforms.length === 0}
    <p class="empty-text">{$t("courses.no_search_results") ?? "No matching courses"}</p>
  {/if}
</div>
{/if}

<style>
  .courses-page {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: calc(var(--padding) * 2);
    width: 100%;
  }

  .courses-head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    width: 100%;
    max-width: 900px;
    margin-bottom: var(--space-2);
    padding: var(--space-4) var(--space-5);
    background: var(--button-elevated);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-lg);
    box-shadow: 4px 4px 0 var(--shadow-hard);
  }

  .courses-hero-tile {
    width: 56px;
    height: 56px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    border: 2px solid var(--border-ink);
    border-radius: 14px;
    color: var(--accent);
  }

  .courses-hero-icon {
    width: 40px;
    height: 40px;
    object-fit: contain;
    flex-shrink: 0;
  }

  .courses-identity {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .courses-subtitle {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  h1 {
    font-family: var(--font-display);
    font-size: 22px;
    font-weight: 600;
    letter-spacing: var(--track-snug);
    margin-block: 0;
    width: 100%;
    max-width: 900px;
  }

  .search-wrap {
    position: relative;
    width: 100%;
    max-width: 900px;
  }

  .search-wrap .search-icon {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--gray);
    pointer-events: none;
  }

  .search-wrap .search-input {
    padding-left: 36px;
  }

  .search-input {
    width: 100%;
    max-width: 900px;
    padding: 10px var(--padding);
    font-size: 14px;
    color: var(--secondary);
    background: var(--input-bg);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    outline: none;
    box-sizing: border-box;
  }

  .search-input:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
    border-color: var(--accent);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .search-input::placeholder { color: var(--gray); }

  .fallback-banner {
    width: 100%;
    max-width: 900px;
    padding: 8px 14px;
    font-size: 12px;
    color: var(--text-muted);
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    text-align: center;
  }

  .platform-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: var(--padding);
    width: 100%;
    max-width: 900px;
    justify-items: center;
  }

  .platform-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 100%;
    gap: calc(var(--padding) * 0.75);
    padding: calc(var(--padding) * 2) var(--padding);
    background: var(--button-elevated);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    cursor: pointer;
    transition: transform 0.15s, background 0.15s, box-shadow 0.15s;
  }

  @media (hover: hover) {
    .platform-card:hover {
      background: var(--sidebar-highlight);
      transform: translateY(-2px);
      box-shadow: 4px 4px 0 var(--shadow-hard);
    }
  }

  .platform-card:active { transform: translate(2px, 2px); box-shadow: 2px 2px 0 var(--shadow-hard); }

  .platform-card:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  .card-icon {
    width: 52px;
    height: 52px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: calc(var(--border-radius) - 2px);
    background: color-mix(in srgb, var(--platform-color) 15%, transparent);
    color: var(--platform-color);
  }

  .card-name {
    font-size: var(--text-base);
    font-weight: 500;
    color: var(--secondary);
  }

  .card-status {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--gray);
    min-height: 16px;
    padding: 3px 10px;
    border: 2px solid var(--border-ink);
    border-radius: 9999px;
    background: var(--surface-hi);
    max-width: 100%;
  }

  .card-status.st-connected {
    background: color-mix(in srgb, var(--success) 18%, transparent);
    color: var(--secondary);
  }

  .card-status.st-expired {
    background: color-mix(in srgb, var(--warning) 18%, transparent);
    color: var(--secondary);
  }

  .card-status.st-error {
    background: color-mix(in srgb, var(--danger) 18%, transparent);
    color: var(--secondary);
  }

  .card-status.st-off {
    background: var(--surface-hi);
  }

  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status-dot.connected { background: var(--success); }
  .status-dot.disconnected { background: var(--gray); }
  .status-dot.expired { background: var(--warning); }
  .status-dot.error { background: var(--danger); }

  .status-email {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 140px;
  }

  .empty-text {
    color: var(--gray);
    font-size: var(--text-base);
    text-align: center;
    padding: calc(var(--padding) * 3) 0;
    margin-block: 0;
  }

  @media (max-width: 535px) {
    .platform-grid { grid-template-columns: repeat(2, 1fr); }
  }

  @media (prefers-reduced-motion: reduce) {
    .platform-card { transition: none; }
  }

  .plugin-guard {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: calc(100vh - 140px);
    gap: calc(var(--padding) * 1.5);
    text-align: center;
    color: var(--gray);
  }
  .plugin-guard h2 { font-size: 17px; color: var(--secondary); }
  .plugin-guard p { font-size: 14px; max-width: 300px; }
  .guard-link { padding: 10px 24px; font-size: 14px; font-weight: 600; background: var(--cta); color: var(--on-cta); border: 2px solid var(--border-ink); border-radius: var(--border-radius); text-decoration: none; box-shadow: 4px 4px 0 var(--cta-shadow); }
  .guard-link:active { transform: translate(2px, 2px); box-shadow: 2px 2px 0 var(--cta-shadow); }
  .guard-detail { font-size: 12px; color: var(--tertiary); max-width: 480px; margin-top: calc(var(--padding) * 0.5); }
  .guard-detail code { font-family: var(--font-mono, ui-monospace, monospace); font-size: 11px; word-break: break-word; background: var(--surface); padding: 2px 6px; border-radius: 4px; }
  .spinner { width: 24px; height: 24px; border: 2px solid var(--input-border); border-top-color: var(--secondary); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
