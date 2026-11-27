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
  Listening Studio is a plugin page: when the plugin is not installed
  (or not loaded) this route shows the not-installed state with a link
  to the Tools page instead of a broken studio UI.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import ListeningStudio from "$components/plugins/ListeningStudio.svelte";

  type PluginStatus =
    | "checking"
    | "ready"
    | "needs-restart"
    | "not-installed"
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

  async function checkPluginStatus() {
    try {
      const plugins = await invoke<{
        id: string;
        enabled: boolean;
        loaded: boolean;
        load_error?: PluginLoadError | null;
      }[]>("list_plugins");
      const plugin = plugins.find((p) => p.id === "listening-studio");
      if (!plugin || !plugin.enabled) {
        pluginStatus = "not-installed";
        return;
      }
      if (!plugin.loaded) {
        if (plugin.load_error) {
          loadError = plugin.load_error;
          pluginStatus =
            plugin.load_error.kind === "abi_mismatch" ||
            plugin.load_error.kind === "missing_abi_symbol"
              ? "incompatible"
              : "load-failed";
        } else {
          pluginStatus = "needs-restart";
        }
        return;
      }
      loadError = null;
      pluginStatus = "ready";
    } catch {
      pluginStatus = "ready";
    }
  }

  onMount(() => {
    let pluginsUnlisten: UnlistenFn | null = null;
    let destroyed = false;

    checkPluginStatus();
    listen("plugins-changed", () => {
      checkPluginStatus();
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

{#if pluginStatus === "checking"}
  <div class="plugin-guard"><span class="spinner"></span></div>
{:else if pluginStatus === "not-installed"}
  <div class="plugin-guard">
    <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M20 10H4l4-4" /><path d="M4 14h16l-4 4" />
    </svg>
    <h2>{$t("marketplace.plugin_not_installed")}</h2>
    <p>{$t("tools.hub.plugin_install_hint")}</p>
    <a href="/tools" class="guard-link">{$t("tools.hub.go_to_tools")}</a>
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
    <a href="/tools" class="guard-link">{$t("tools.hub.go_to_tools")}</a>
    {#if loadError}
      <p class="guard-detail"><code>{loadError.message}</code></p>
    {/if}
  </div>
{:else if pluginStatus === "load-failed"}
  <div class="plugin-guard">
    <h2>{$t("marketplace.plugin_load_failed_title")}</h2>
    <p>{$t("marketplace.plugin_load_failed_hint")}</p>
    <a href="/tools" class="guard-link">{$t("tools.hub.go_to_tools")}</a>
    {#if loadError}
      <p class="guard-detail"><code>{loadError.message}</code></p>
    {/if}
  </div>
{:else}
  <ListeningStudio />
{/if}

<style>
  .plugin-guard { display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: calc(100vh - 140px); gap: calc(var(--padding) * 1.5); text-align: center; color: var(--gray); }
  .plugin-guard h2 { font-size: 17px; color: var(--secondary); }
  .plugin-guard p { font-size: 14px; max-width: 300px; }
  .guard-link { padding: 10px 24px; font-size: 14px; font-weight: 500; background: var(--cta); color: var(--on-cta); border: 2px solid var(--border-ink); border-radius: var(--border-radius); text-decoration: none; box-shadow: 2px 2px 0 var(--shadow-hard); }
  .guard-detail { font-size: 12px; color: var(--tertiary); max-width: 480px; margin-top: calc(var(--padding) * 0.5); }
  .guard-detail code { font-family: var(--font-mono, ui-monospace, monospace); font-size: 11px; word-break: break-word; background: var(--surface); padding: 2px 6px; border-radius: 4px; }
  .spinner { width: 24px; height: 24px; border: 2px solid var(--input-border); border-top-color: var(--secondary); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
