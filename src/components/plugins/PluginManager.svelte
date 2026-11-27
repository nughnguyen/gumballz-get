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

<!--
  PluginManager — the single place where plugins are installed, browsed
  and managed. Used by the Tools page; reads live state from
  `list_plugins` (never the hardcoded registry) and refreshes on the
  `plugins-changed` event. The 5 official tools install from the network
  via `install_plugin_from_registry` (their `.zip` downloads live on the
  rolling `plugins` release of this repo, listed in `plugins.json`);
  nothing about them is bundled in the app. Community entries install
  through the same registry command.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import NavIcon from "$components/shell/NavIcon.svelte";

  type PluginNavInfo = {
    route: string;
    label: Record<string, string>;
    icon_svg: string | null;
    group: string;
    order: number;
  };

  type PluginLoadError = {
    message: string;
    kind: string;
    plugin_abi?: number | null;
    expected_abi?: number | null;
  };

  type PluginInfo = {
    id: string;
    name: string;
    version: string;
    description: string;
    author: string;
    enabled: boolean;
    loaded: boolean;
    icon: string | null;
    nav: PluginNavInfo[];
    load_error?: PluginLoadError | null;
  };

  type MarketplaceEntry = {
    id: string;
    name: string;
    description: string;
    author: string;
    repo: string;
    homepage: string | null;
    tags: string[];
    official: boolean;
    capabilities: string[];
    installed: boolean;
    installed_version: string | null;
  };

  type UpdateInfo = {
    id: string;
    installed_version: string;
    latest_version: string;
    repo: string;
    has_update: boolean;
  };

  type OfficialTool = {
    id: string;
    nameKey: string;
    descKey: string;
    route: string;
  };

  const OFFICIAL_TOOLS: OfficialTool[] = [
    { id: "convert", nameKey: "convert.title", descKey: "convert.subtitle", route: "/convert" },
    { id: "telegram", nameKey: "telegram.title", descKey: "tools.hub.official_telegram_desc", route: "/telegram" },
    { id: "courses", nameKey: "courses.title", descKey: "courses.subtitle", route: "/courses" },
    {
      id: "listening-studio",
      nameKey: "tools.hub.official_listening_studio_name",
      descKey: "tools.hub.official_listening_studio_desc",
      route: "/plugins/listening-studio",
    },
    {
      id: "asset-forge",
      nameKey: "tools.hub.official_asset_forge_name",
      descKey: "tools.hub.official_asset_forge_desc",
      route: "/plugins/asset-forge",
    },
  ];

  const OFFICIAL_IDS = new Set(OFFICIAL_TOOLS.map((tool) => tool.id));

  // The official tools ship from this repo's rolling `plugins` release;
  // installing one downloads its `.zip` from the network (see `plugins.json`).
  const OFFICIAL_REPO = "nughnguyen/gumballz-get";

  const CAP_LABELS: Record<string, string> = {
    "core:events": "cap_events",
    "core:toast": "cap_toast",
    "core:settings": "cap_settings",
    "core:filesystem": "cap_filesystem",
    "core:proxy": "cap_proxy",
    "core:tools": "cap_tools",
    "core:download-queue": "cap_download_queue",
  };

  function capLabel(cap: string): string {
    const key = CAP_LABELS[cap];
    return key ? $t(`marketplace.${key}`) : cap;
  }

  function getPluginIcon(id: string): string {
    const norm = id.toLowerCase();
    if (norm === "convert" || norm.includes("convert")) return "convert";
    if (norm === "courses" || norm.includes("course")) return "courses";
    if (norm === "telegram" || norm.includes("telegram")) return "telegram";
    if (norm === "listening-studio" || norm === "asset-forge") return "plugin";
    return id;
  }

  function errorMessage(e: unknown): string {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message;
    if (e && typeof e === "object" && "message" in e && typeof e.message === "string") return e.message;
    return $t("common.error") as string;
  }

  let plugins = $state<PluginInfo[]>([]);
  let loadingInstalled = $state(true);
  let updates = $state<Record<string, UpdateInfo>>({});
  let updatingId = $state<string | null>(null);
  let installingId = $state<string | null>(null);
  let installingFile = $state(false);
  let confirmUninstallId = $state<string | null>(null);

  let registry = $state<MarketplaceEntry[]>([]);
  let loadingBrowse = $state(false);
  let browseError = $state(false);
  let browseFetched = $state(false);

  let communityRegistry = $derived(registry.filter((entry) => !OFFICIAL_IDS.has(entry.id)));

  function installedPlugin(id: string): PluginInfo | undefined {
    return plugins.find((p) => p.id === id);
  }

  function openRoute(plugin: PluginInfo): string | null {
    return plugin.nav?.[0]?.route ?? OFFICIAL_TOOLS.find((tool) => tool.id === plugin.id)?.route ?? null;
  }

  async function refreshPlugins() {
    try {
      plugins = await invoke<PluginInfo[]>("list_plugins");
    } catch {
      plugins = [];
    }
  }

  async function refreshUpdates() {
    if (plugins.length === 0) return;
    invoke<UpdateInfo[]>("check_plugin_updates")
      .then((updateList) => {
        for (const u of updateList) {
          if (u.has_update) updates[u.id] = u;
        }
      })
      .catch(() => {});
  }

  async function loadBrowse() {
    loadingBrowse = true;
    browseError = false;
    try {
      const timeout = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error("timeout")), 15000),
      );
      registry = await Promise.race([
        invoke<MarketplaceEntry[]>("fetch_marketplace_registry"),
        timeout,
      ]);
      browseFetched = true;
    } catch {
      browseError = true;
    }
    loadingBrowse = false;
  }

  async function installPlugin(id: string, repo: string) {
    installingId = id;
    try {
      await invoke("install_plugin_from_registry", { pluginId: id, repo });
      await refreshPlugins();
      await refreshUpdates();
      const idx = registry.findIndex((p) => p.id === id);
      if (idx >= 0) {
        const installed = plugins.find((p) => p.id === id);
        registry[idx] = {
          ...registry[idx],
          installed: true,
          installed_version: installed?.version ?? registry[idx].installed_version,
        };
      }
    } catch (e: unknown) {
      const raw = errorMessage(e);
      const msg = raw.startsWith("NetworkUnreachable|")
        ? $t("marketplace.install_network_error")
        : raw;
      showToast("error", msg as string);
    }
    installingId = null;
  }

  async function installFromFile() {
    if (installingFile) return;
    let selected: string | string[] | null = null;
    try {
      selected = await openFileDialog({
        multiple: false,
        filters: [{ name: "Plugin package", extensions: ["zip"] }],
      });
    } catch (e: unknown) {
      showToast("error", errorMessage(e));
      return;
    }
    const path = Array.isArray(selected) ? selected[0] : selected;
    if (!path) return;
    installingFile = true;
    try {
      // Added by the backend agent in parallel: installs a local .zip
      // plugin package and returns the installed version.
      await invoke<string>("install_plugin_from_file", { path });
      await refreshPlugins();
      await refreshUpdates();
    } catch (e: unknown) {
      showToast("error", errorMessage(e));
    }
    installingFile = false;
  }

  async function updatePlugin(id: string) {
    const info = updates[id];
    if (!info) return;
    updatingId = id;
    try {
      await invoke("update_plugin", { pluginId: id, repo: info.repo });
      delete updates[id];
      const idx = plugins.findIndex((p) => p.id === id);
      if (idx >= 0) {
        plugins[idx] = { ...plugins[idx], version: info.latest_version };
      }
    } catch (e: unknown) {
      showToast("error", errorMessage(e));
    }
    updatingId = null;
  }

  async function uninstallPlugin(id: string) {
    try {
      await invoke("uninstall_plugin", { pluginId: id });
      plugins = plugins.filter((p) => p.id !== id);
      delete updates[id];
      const idx = registry.findIndex((p) => p.id === id);
      if (idx >= 0) {
        registry[idx] = { ...registry[idx], installed: false, installed_version: null };
      }
    } catch (e: unknown) {
      showToast("error", errorMessage(e));
    }
    confirmUninstallId = null;
  }

  async function togglePlugin(id: string, enabled: boolean) {
    try {
      await invoke("set_plugin_enabled", { pluginId: id, enabled });
      const idx = plugins.findIndex((p) => p.id === id);
      if (idx >= 0) plugins[idx] = { ...plugins[idx], enabled };
    } catch (e: unknown) {
      showToast("error", errorMessage(e));
    }
  }

  onMount(() => {
    let pluginsUnlisten: UnlistenFn | null = null;
    let destroyed = false;

    (async () => {
      await refreshPlugins();
      loadingInstalled = false;
      await refreshUpdates();
      if (!browseFetched) loadBrowse();
    })();

    listen("plugins-changed", () => {
      void refreshPlugins().then(() => refreshUpdates());
    }).then((unlisten) => {
      if (destroyed) {
        unlisten();
      } else {
        pluginsUnlisten = unlisten;
      }
    });

    return () => {
      destroyed = true;
      pluginsUnlisten?.();
      pluginsUnlisten = null;
    };
  });
</script>

<div class="plugin-manager">
  <section class="tools-section" aria-labelledby="installed-title">
    <div class="section-header">
      <span class="section-header-title" id="installed-title">{$t("marketplace.installed")}</span>
      {#if !loadingInstalled}
        <span class="section-count">{plugins.length}</span>
      {/if}
    </div>

    {#if loadingInstalled}
      <div class="loading">
        <span class="spinner"></span>
      </div>
    {:else if plugins.length === 0}
      <div class="tools-empty">
        <img class="empty-state-art" src="/emoji/electric_plug.png" alt="" width="72" height="72" draggable="false" />
        <h2>{$t("tools.hub.no_plugins_title")}</h2>
        <p>{$t("tools.hub.no_plugins_desc")}</p>
        <a class="empty-link" href="#tools-browse">{$t("marketplace.browse")}</a>
      </div>
    {:else}
      <p class="installed-hint">{$t("tools.hub.installed_hint")}</p>
      <div class="plugin-list">
        {#each plugins as plugin (plugin.id)}
          {@render installedCard(plugin)}
        {/each}
      </div>
    {/if}
  </section>

  <section class="tools-section" id="tools-browse" aria-labelledby="browse-title">
    <div class="section-header">
      <span class="section-header-title" id="browse-title">{$t("marketplace.browse")}</span>
    </div>

    <div class="install-file-row">
      <button
        type="button"
        class="btn btn-primary install-file-btn"
        disabled={installingFile}
        onclick={installFromFile}
      >
        {installingFile ? $t("marketplace.browse_loading") : $t("tools.hub.install_from_file")}
      </button>
      <span class="install-file-hint">{$t("tools.hub.install_from_file_hint")}</span>
    </div>

    <h3 class="browse-subhead">{$t("tools.hub.official_tools")}</h3>
    <div class="plugin-list">
      {#each OFFICIAL_TOOLS as tool (tool.id)}
        {@const installed = installedPlugin(tool.id)}
        <div class="plugin-card">
          <div class="plugin-header">
            <div class="plugin-icon-slot">
              <NavIcon icon={getPluginIcon(tool.id)} size={40} />
            </div>
            <div class="plugin-info">
              <div class="plugin-name-row">
                <span class="plugin-name">{$t(tool.nameKey)}</span>
                <span class="badge-official">{$t("marketplace.official_badge")}</span>
              </div>
              {#if installed}
                <span class="plugin-meta">
                  {$t("marketplace.version", { version: installed.version })}
                  {#if installed.author}
                    <span class="meta-sep">&middot;</span>
                    {$t("marketplace.by_author", { author: installed.author })}
                  {/if}
                </span>
              {/if}
            </div>
            <div class="plugin-actions">
              {#if installed}
                {#if installed.enabled && installed.loaded && openRoute(installed)}
                  <a class="btn btn-secondary open-btn" href={openRoute(installed) as string}>
                    {$t("tools.hub.open_plugin")}
                  </a>
                {/if}
                <span class="installed-badge">{$t("marketplace.installed_badge")}</span>
              {:else}
                <button
                  class="btn btn-primary install-btn"
                  disabled={installingId === tool.id}
                  onclick={() => installPlugin(tool.id, OFFICIAL_REPO)}
                >
                  {#if installingId === tool.id}
                    {$t("marketplace.browse_loading")}
                  {:else}
                    {$t("marketplace.install")}
                  {/if}
                </button>
              {/if}
            </div>
          </div>
          <p class="plugin-desc">{$t(tool.descKey)}</p>
        </div>
      {/each}
    </div>

    {#if loadingBrowse}
      <div class="plugin-list">
        {#each [0, 1, 2] as i (i)}
          <div class="plugin-card skeleton">
            <div class="plugin-header">
              <span class="skeleton-line skeleton-line-icon"></span>
              <div class="plugin-info">
                <span class="skeleton-line skeleton-line-name"></span>
                <span class="skeleton-line skeleton-line-meta"></span>
              </div>
              <span class="skeleton-line skeleton-line-action"></span>
            </div>
            <span class="skeleton-line skeleton-line-desc"></span>
            <span class="skeleton-line skeleton-line-desc-2"></span>
          </div>
        {/each}
      </div>
    {:else if browseError}
      <div class="tools-empty">
        <svg class="empty-state-art" width="72" height="72" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="10" stroke="#EF4444" />
          <line x1="12" y1="8" x2="12" y2="12" stroke="#EF4444" />
          <line x1="12" y1="16" x2="12.01" y2="16" stroke="#EF4444" />
        </svg>
        <p>{$t("marketplace.browse_error")}</p>
        <p class="error-hint">{$t("marketplace.browse_error_hint")}</p>
        <button class="btn btn-secondary retry-btn" onclick={loadBrowse}>{$t("marketplace.browse_retry")}</button>
      </div>
    {:else if communityRegistry.length === 0}
      <div class="tools-empty">
        <p>{$t("marketplace.browse_empty")}</p>
      </div>
    {:else}
      <div class="plugin-list">
        {#each communityRegistry as entry (entry.id)}
          <div class="plugin-card">
            <div class="plugin-header">
              <div class="plugin-icon-slot">
                <NavIcon icon={getPluginIcon(entry.id)} size={40} />
              </div>
              <div class="plugin-info">
                <div class="plugin-name-row">
                  <span class="plugin-name">{entry.name}</span>
                  {#if entry.official}
                    <span class="badge-official">{$t("marketplace.official_badge")}</span>
                  {:else}
                    <span class="badge-community">{$t("marketplace.community_badge")}</span>
                  {/if}
                </div>
                <span class="plugin-meta">
                  {$t("marketplace.by_author", { author: entry.author })}
                </span>
              </div>
              <div class="plugin-actions">
                {#if entry.installed}
                  <span class="installed-badge">{$t("marketplace.installed_badge")}</span>
                {:else}
                  <button
                    class="btn btn-primary install-btn"
                    disabled={installingId === entry.id}
                    onclick={() => installPlugin(entry.id, entry.repo)}
                  >
                    {#if installingId === entry.id}
                      {$t("marketplace.browse_loading")}
                    {:else}
                      {$t("marketplace.install")}
                    {/if}
                  </button>
                {/if}
              </div>
            </div>
            <p class="plugin-desc">{entry.description}</p>
            {#if entry.tags.length > 0}
              <div class="tag-list">
                {#each entry.tags as tag}
                  <span class="tag">{tag}</span>
                {/each}
              </div>
            {/if}
            {#if entry.capabilities.length > 0}
              <details class="cap-details">
                <summary class="cap-summary">{$t("marketplace.capabilities")} ({entry.capabilities.length})</summary>
                <ul class="cap-list">
                  {#each entry.capabilities as cap}
                    <li class="cap-item">
                      <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M9 12l2 2 4-4" />
                        <circle cx="12" cy="12" r="10" />
                      </svg>
                      {capLabel(cap)}
                    </li>
                  {/each}
                </ul>
              </details>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>

{#snippet installedCard(plugin: PluginInfo)}
  <div class="plugin-card">
    <div class="plugin-header">
      <div class="plugin-icon-slot">
        <NavIcon icon={getPluginIcon(plugin.id)} iconSvg={plugin.nav?.[0]?.icon_svg || undefined} size={40} />
      </div>
      <div class="plugin-info">
        <div class="plugin-name-row">
          <span class="plugin-name">{plugin.name}</span>
        </div>
        <span class="plugin-meta">
          {$t("marketplace.version", { version: plugin.version })}
          {#if plugin.author}
            <span class="meta-sep">&middot;</span>
            {$t("marketplace.by_author", { author: plugin.author })}
          {/if}
        </span>
      </div>
      <div class="plugin-actions">
        {#if updates[plugin.id]}
          <button
            class="update-btn"
            disabled={updatingId === plugin.id}
            onclick={() => updatePlugin(plugin.id)}
          >
            {#if updatingId === plugin.id}
              {$t("marketplace.browse_loading")}
            {:else}
              {$t("marketplace.update_available")}
            {/if}
          </button>
        {/if}
        {#if plugin.enabled && plugin.loaded && openRoute(plugin)}
          <a class="btn btn-secondary open-btn" href={openRoute(plugin) as string}>
            {$t("tools.hub.open_plugin")}
          </a>
        {/if}
        <div class="sidebar-toggle-row">
          <span class="sidebar-toggle-label">
            {plugin.enabled ? $t("marketplace.disable") : $t("marketplace.enable")}
          </span>
          <button
            class="toggle"
            class:on={plugin.enabled}
            role="switch"
            aria-checked={plugin.enabled}
            aria-label={plugin.enabled ? $t("marketplace.disable") : $t("marketplace.enable")}
            onclick={() => togglePlugin(plugin.id, !plugin.enabled)}
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
        {#if confirmUninstallId === plugin.id}
          <span class="uninstall-confirm-text">{$t("tools.hub.confirm_uninstall", { name: plugin.name })}</span>
          <button class="uninstall-btn confirm" onclick={() => uninstallPlugin(plugin.id)}>
            {$t("marketplace.uninstall")}
          </button>
          <button class="uninstall-btn" onclick={() => (confirmUninstallId = null)}>
            {$t("common.cancel")}
          </button>
        {:else}
          <button class="uninstall-btn" onclick={() => (confirmUninstallId = plugin.id)}>
            {$t("marketplace.uninstall")}
          </button>
        {/if}
      </div>
    </div>
    {#if updates[plugin.id]}
      <span class="update-hint">{$t("marketplace.update_hint", { version: updates[plugin.id].latest_version })}</span>
    {/if}
    {#if plugin.enabled && !plugin.loaded && plugin.load_error}
      {@const incompatible =
        plugin.load_error.kind === "abi_mismatch" ||
        plugin.load_error.kind === "missing_abi_symbol"}
      <div class="load-error" class:incompatible>
        <strong>
          {incompatible
            ? $t("marketplace.plugin_incompatible_title")
            : $t("marketplace.plugin_load_failed_title")}
        </strong>
        <span>
          {incompatible
            ? $t("marketplace.plugin_incompatible_hint")
            : $t("marketplace.plugin_load_failed_hint")}
        </span>
        <code>{plugin.load_error.message}</code>
      </div>
    {/if}
    {#if plugin.description}
      <p class="plugin-desc">{plugin.description}</p>
    {/if}
  </div>
{/snippet}

<style>
  .plugin-manager {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    width: 100%;
  }

  .tools-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .section-count {
    font: 700 10px 'JetBrains Mono', monospace;
    letter-spacing: 0.1em;
    color: var(--text-dim);
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-8) 0;
  }

  .tools-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-8) var(--space-4);
    text-align: center;
  }

  .tools-empty h2 {
    margin: var(--space-2) 0 0;
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text);
  }

  .tools-empty p {
    margin: 0;
    font-size: var(--text-base);
    color: var(--text-muted);
  }

  .empty-link {
    padding: 10px 24px;
    font-size: var(--text-base);
    font-weight: 600;
    background: var(--cta);
    color: var(--on-cta);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    text-decoration: none;
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .empty-link:active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0 var(--shadow-hard);
  }

  .error-hint {
    font-size: var(--text-sm);
    max-width: 360px;
  }

  .retry-btn {
    margin-top: var(--space-2);
  }

  .installed-hint {
    margin: 0;
    font-size: var(--text-base);
    line-height: var(--leading-base);
    color: var(--text-dim);
    max-width: 60ch;
  }

  .install-file-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .install-file-btn {
    min-height: 36px;
  }

  .install-file-hint {
    font-size: var(--text-sm);
    color: var(--text-dim);
  }

  .browse-subhead {
    margin: var(--space-3) 0 0;
    padding: 0 var(--space-2);
    font-size: var(--text-base);
    font-weight: 700;
    color: var(--text);
  }

  .plugin-list {
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-lg);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    overflow: hidden;
  }

  .plugin-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-3);
    position: relative;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .plugin-card + .plugin-card::before {
    content: "";
    position: absolute;
    top: 0;
    left: calc(var(--space-3) + 40px + var(--space-3));
    right: 0;
    height: var(--hairline);
    background: var(--separator);
  }

  @media (hover: hover) {
    .plugin-card:hover {
      background: var(--fill-1);
    }
  }

  .plugin-card.skeleton {
    pointer-events: none;
  }

  .skeleton-line {
    display: block;
    height: 12px;
    border-radius: var(--radius-xs);
    background: var(--fill-2);
    animation: skeleton-shimmer 1.4s var(--ease-in-out) infinite;
  }

  .skeleton-line-name { width: 140px; height: 14px; }
  .skeleton-line-meta { width: 90px; height: 10px; margin-top: 6px; }
  .skeleton-line-action { width: 64px; height: 24px; border-radius: var(--radius-full); }
  .skeleton-line-desc { width: 70%; }
  .skeleton-line-desc-2 { width: 45%; }

  @keyframes skeleton-shimmer {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }

  @media (prefers-reduced-motion: reduce) {
    .skeleton-line {
      animation: none;
    }
  }

  .plugin-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .plugin-icon-slot {
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .skeleton-line-icon {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    flex-shrink: 0;
  }

  .plugin-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }

  .plugin-name-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .plugin-name {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge-official,
  .badge-community {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 6px;
    border-radius: var(--radius-full);
    font-size: var(--text-caption);
    font-weight: 600;
    white-space: nowrap;
    border: 2px solid var(--border-ink);
  }

  .badge-official {
    background: var(--accent);
    color: var(--on-accent);
  }

  .badge-community {
    background: var(--surface-hi);
    color: var(--text);
  }

  .installed-badge {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--text-dim);
  }

  .plugin-meta {
    font-size: var(--text-sm);
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .meta-sep {
    margin: 0 2px;
  }

  .plugin-desc {
    margin: 0 0 0 calc(40px + var(--space-3));
    font-size: var(--text-sm);
    line-height: var(--leading-sm);
    color: var(--text-muted);
  }

  .plugin-actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-shrink: 0;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .open-btn {
    min-height: 28px;
    text-decoration: none;
  }

  .sidebar-toggle-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .sidebar-toggle-label {
    font-size: var(--text-sm);
    color: var(--text-dim);
  }

  .update-btn {
    height: 24px;
    padding: 0 var(--space-3);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-full);
    background: var(--cta);
    color: var(--on-cta);
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .update-btn:disabled {
    opacity: 0.85;
    cursor: default;
  }

  .update-hint {
    margin-left: calc(40px + var(--space-3));
    font-size: var(--text-sm);
    color: var(--accent-hi);
  }

  .load-error {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-left: calc(40px + var(--space-3));
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    font-size: var(--text-sm);
    color: var(--text);
  }

  .load-error strong {
    font-weight: 600;
  }

  .load-error code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-dim);
    word-break: break-all;
  }

  .uninstall-btn {
    height: 24px;
    padding: 0 var(--space-2);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-sm);
    background: var(--surface-hi);
    color: var(--text-dim);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: color var(--duration-fast) var(--ease-out), background var(--duration-fast) var(--ease-out);
  }

  @media (hover: hover) {
    .uninstall-btn:hover {
      background: var(--surface-hi);
      color: var(--danger);
    }
  }

  .uninstall-btn:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  .uninstall-btn.confirm {
    background: var(--danger);
    color: #fff;
  }

  .uninstall-confirm-text {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text);
  }

  .install-btn {
    height: 26px;
    min-width: 72px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-full);
    font-size: var(--text-sm);
    font-weight: 700;
    letter-spacing: 0.01em;
  }

  .install-btn:disabled {
    opacity: 0.85;
  }

  .tag-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-left: calc(40px + var(--space-3));
  }

  .tag {
    height: 18px;
    font-size: var(--text-caption);
  }

  .cap-details {
    margin-left: calc(40px + var(--space-3));
  }

  .cap-summary {
    font-size: var(--text-xs);
    color: var(--text-dim);
    cursor: pointer;
    list-style: none;
    user-select: none;
  }

  .cap-summary::-webkit-details-marker {
    display: none;
  }

  .cap-summary::marker {
    content: "";
  }

  @media (hover: hover) {
    .cap-summary:hover {
      color: var(--text);
    }
  }

  .cap-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: var(--space-1) 0 0;
    padding: 0;
    list-style: none;
  }

  .cap-item {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .cap-item svg {
    flex-shrink: 0;
    color: var(--text-dim);
  }

  @media (prefers-reduced-motion: reduce) {
    .loading :global(.spinner) {
      animation: mp-soft-pulse 1.5s var(--ease-in-out) infinite;
    }
  }

  @keyframes mp-soft-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }
</style>
