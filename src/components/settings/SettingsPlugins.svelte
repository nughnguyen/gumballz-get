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
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { getSettings, type DependencyStatus } from "./settings-helpers";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { BUILD_INFO } from "$lib/build-info";
  import DependencyRow from "./DependencyRow.svelte";

  let { deps = [], installingDep = null, autoInstalling = [], onInstallDep, onRefresh }: {
    deps?: DependencyStatus[];
    installingDep?: string | null;
    /** Dependencies the startup thread is downloading right now. */
    autoInstalling?: string[];
    onInstallDep: (name: string, variant: string | null) => void;
    onRefresh?: () => void | Promise<void>;
  } = $props();

  let settings = $derived(getSettings());

  type BrowserExtStatus = {
    browser: string;
    supported: boolean;
    bundled_version: string | null;
    installable: boolean;
    install_hint_key: string;
    store_url: string | null;
    connected: boolean;
  };

  type PrepareInstallResult = {
    folder_path: string;
    source: string;
    version: string | null;
  };

  function hintFor(key: string): string {
    if (key === "chrome") return $t("settings.browser_ext.hint_chrome") as string;
    if (key === "firefox") return $t("settings.browser_ext.hint_firefox") as string;
    if (key === "safari") return $t("settings.browser_ext.hint_safari") as string;
    return "";
  }

  let extStatuses = $state<BrowserExtStatus[]>([]);
  let busyBrowser = $state<string | null>(null);
  let installingBrowser = $state<string | null>(null);
  let installModal = $state<{
    browser: string;
    path: string;
    hint: string;
    source: "bundled" | "downloaded" | null;
    mode: "install" | "export";
    pageOpened: boolean;
    pageUrl: string;
  } | null>(null);

  onMount(async () => {
    try {
      extStatuses = await invoke<BrowserExtStatus[]>("browser_extension_status");
    } catch (e) {
      console.warn("[ext] status failed", e);
    }
  });

  async function exportExtension(browser: string) {
    if (busyBrowser) return;
    busyBrowser = browser;
    try {
      const res = await invoke<{ path: string; version: string | null; browser: string }>(
        "browser_extension_export",
        { browser },
      );
      const status = extStatuses.find((s) => s.browser === browser);
      installModal = {
        browser,
        path: res.path,
        hint: status ? hintFor(status.install_hint_key) : "",
        source: null,
        mode: "export",
        pageOpened: true,
        pageUrl: "",
      };
    } catch (e) {
      showToast("error", e instanceof Error ? e.message : String(e));
    } finally {
      busyBrowser = null;
    }
  }

  function installPageUrl(browser: string): string {
    return browser === "firefox"
      ? "about:debugging#/runtime/this-firefox"
      : "chrome://extensions/";
  }

  /** One-click install: the app prepares the unpacked folder, opens the
   * browser's extensions page, and the modal shows the last 2 clicks
   * (Developer mode -> Load unpacked). Chrome/Cốc Cốc block silent installs,
   * so those 2 clicks stay manual by design. */
  async function installExtension(browser: string) {
    if (installingBrowser) return;
    installingBrowser = browser;
    try {
      const res = await invoke<PrepareInstallResult>("browser_extension_prepare_install", {
        browser,
      });
      let pageOpened = true;
      try {
        const { openUrl } = await import("@tauri-apps/plugin-opener");
        await openUrl(installPageUrl(browser));
      } catch (e) {
        pageOpened = false;
        console.warn("[ext] open install page failed", e);
      }
      installModal = {
        browser,
        path: res.folder_path,
        hint: "",
        source: res.source === "downloaded" ? "downloaded" : "bundled",
        mode: "install",
        pageOpened,
        pageUrl: installPageUrl(browser),
      };
    } catch (e) {
      showToast("error", e instanceof Error ? e.message : String(e));
    } finally {
      installingBrowser = null;
    }
  }

  async function openFolder(path: string) {
    try {
      await invoke("browser_extension_open_folder", { path });
    } catch (e) {
      showToast("error", e instanceof Error ? e.message : String(e));
    }
  }

  function closeInstall() {
    installModal = null;
  }

  function browserLabel(b: string): string {
    if (b === "chrome") return "Chrome / Edge / Brave";
    if (b === "firefox") return "Firefox";
    if (b === "safari") return "Safari";
    return b;
  }
</script>

{#if settings}
  <section class="section">
    <h5 class="section-title">{$t("settings.browser_ext.title")}</h5>
    <p class="muted">{$t("settings.browser_ext.description")}</p>
    <div class="card">
      {#each extStatuses as ext, i (ext.browser)}
        {#if i > 0}<div class="divider"></div>{/if}
        <div class="setting-row">
          <div class="setting-col">
            <span class="setting-label">{browserLabel(ext.browser)}</span>
            <span class="setting-path">
              {#if ext.bundled_version}
                {$t("settings.browser_ext.bundled_version", { version: ext.bundled_version })}
              {:else}
                {hintFor(ext.install_hint_key)}
              {/if}
            </span>
          </div>
          {#if ext.connected}
            <span class="muted small installed-badge">✓ {$t("settings.browser_ext.installed")}</span>
          {:else}
            <span class="row-actions">
              {#if ext.browser === "chrome" || ext.installable}
                <button
                  class="button"
                  disabled={installingBrowser === ext.browser}
                  onclick={() => installExtension(ext.browser)}
                >
                  {installingBrowser === ext.browser
                    ? $t("settings.browser_ext.installing")
                    : $t("settings.browser_ext.install")}
                </button>
              {/if}
              {#if ext.installable}
                <button
                  class="button"
                  disabled={busyBrowser === ext.browser}
                  onclick={() => exportExtension(ext.browser)}
                >
                  {busyBrowser === ext.browser
                    ? $t("settings.browser_ext.extracting")
                    : $t("settings.browser_ext.update_install")}
                </button>
              {:else if ext.store_url}
                <a class="button" href={ext.store_url} target="_blank" rel="noreferrer">{$t("settings.browser_ext.open_store")}</a>
              {:else if ext.browser !== "chrome"}
                <span class="muted small">{$t("settings.browser_ext.unavailable")}</span>
              {/if}
            </span>
          {/if}
        </div>
      {/each}
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-col">
          <span class="setting-label">Official repository</span>
          <span class="setting-path">v{BUILD_INFO.version} releases, source code, and standalone install zips.</span>
        </div>
        <a
          class="button"
          href="https://github.com/nughnguyen/gumballz-get"
          target="_blank"
          rel="noreferrer"
        >
          GitHub
        </a>
      </div>
    </div>
  </section>

  {#if deps.length > 0}
    <section class="section">
      <div class="settings-section-head section-title">
        <h5 class="section-title">{$t('settings.dependencies.title')}</h5>
      </div>
      <div class="deps-table-wrap">
        <table class="deps-table">
          <thead>
            <tr>
              <th scope="col">{$t('settings.dependencies.col_name')}</th>
              <th scope="col">{$t('settings.dependencies.col_version')}</th>
              <th scope="col">{$t('settings.dependencies.col_status')}</th>
              <th scope="col" class="deps-col-action">{$t('settings.dependencies.col_action')}</th>
            </tr>
          </thead>
          <tbody>
            {#each deps as dep (dep.name)}
              <DependencyRow
                name={dep.name}
                installed={dep.installed}
                version={dep.version}
                source={dep.source}
                path={dep.path}
                outdated={dep.outdated}
                busy={installingDep === dep.name || autoInstalling.includes(dep.name)}
                onInstall={(variant) => onInstallDep(dep.name, variant)}
                onAfterCustomFile={onRefresh}
              />
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}
{/if}

{#if installModal}
  <div class="overlay" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) closeInstall(); }}>
    <div class="dialog" role="dialog" aria-modal="true">
      <header class="head">
        <h3>
          {#if installModal.mode === "install"}
            {$t("settings.browser_ext.modal_install_title", { browser: browserLabel(installModal.browser) })}
          {:else}
            Instalar / Atualizar extensão — {browserLabel(installModal.browser)}
          {/if}
        </h3>
        <button class="close" onclick={closeInstall} aria-label="fechar">×</button>
      </header>
      <div class="body">
        {#if installModal.mode === "install"}
          {#if installModal.source === "downloaded"}
            <p class="hint">{$t("settings.browser_ext.downloaded_notice")}</p>
          {:else}
            <p class="hint">{$t("settings.browser_ext.bundled_notice")}</p>
          {/if}
          {#if !installModal.pageOpened}
            <p class="hint">{$t("settings.browser_ext.open_page_failed", { url: installModal.pageUrl })}</p>
          {/if}
          <p>{$t("settings.browser_ext.folder_label")}</p>
          <code class="path">{installModal.path}</code>
          <button class="button" onclick={() => openFolder(installModal!.path)}>{$t("settings.browser_ext.open_folder")}</button>
          <h4>{$t("settings.browser_ext.install_steps_title")}</h4>
          {#if installModal.browser === "firefox"}
            <ol class="steps">
              <li>{$t("settings.browser_ext.install_step_firefox")}</li>
            </ol>
            <p class="hint small">{$t("settings.browser_ext.install_note_firefox")}</p>
          {:else}
            <ol class="steps">
              <li>{$t("settings.browser_ext.install_step1_chrome")}</li>
              <li>{$t("settings.browser_ext.install_step2_chrome")}</li>
            </ol>
            <p class="hint small">{$t("settings.browser_ext.install_note_chrome")}</p>
          {/if}
        {:else}
          <p>A extensão foi extraída para:</p>
          <code class="path">{installModal.path}</code>
          <button class="button" onclick={() => openFolder(installModal!.path)}>Abrir pasta</button>
          <h4>Como instalar / atualizar</h4>
          <p class="hint">{installModal.hint}</p>
          <p class="hint small">
            Se a extensão já está instalada, clique em "Recarregar" no <code>{installModal.browser === 'firefox' ? 'about:debugging' : 'chrome://extensions'}</code> apontando pra essa pasta — ou remova a antiga e adicione a nova.
          </p>
        {/if}
      </div>
      <footer class="foot">
        <button class="button" onclick={closeInstall}>{installModal.mode === "install" ? $t("common.close") : "Fechar"}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .muted { color: var(--text-dim); font-size: 12px; line-height: 1.4; margin: 0 0 8px; }
  .small { font-size: 11px; }
  .row-actions { display: inline-flex; gap: 8px; align-items: center; }
  .installed-badge { font-weight: 600; }
  .steps { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 6px; font-size: 13px; line-height: 1.5; }
  .overlay {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.6);
    display: grid; place-items: center; z-index: 999;
  }
  .dialog {
    background: var(--surface);
    border: 2px solid var(--border-ink);
    border-radius: 12px;
    box-shadow: 4px 4px 0 var(--shadow-hard);
    width: min(560px, 90vw);
    max-height: 80vh; overflow: auto;
    display: flex; flex-direction: column;
  }
  .head {
    display: flex; justify-content: space-between; align-items: center;
    padding: 16px 20px; border-bottom: var(--hairline) solid var(--separator);
  }
  .head h3 { margin: 0; font-size: 16px; }
  .close {
    background: transparent; border: 2px solid transparent; color: var(--text);
    font-size: 22px; line-height: 1; cursor: pointer; border-radius: 6px; padding: 0 6px;
  }
  .close:hover { border-color: var(--border-ink); background: var(--surface-hi); }
  .body { padding: 16px 20px; display: flex; flex-direction: column; gap: 12px; }
  .body h4 { margin: 8px 0 0; font-size: 13px; font-weight: 600; }
  .body p { margin: 0; font-size: 13px; line-height: 1.5; }
  .body .hint { color: var(--text-dim); }
  .path {
    display: block; padding: 8px 10px; background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: 6px; font-family: var(--font-mono, monospace); font-size: 11px;
    word-break: break-all;
  }
  .foot {
    padding: 12px 20px; border-top: var(--hairline) solid var(--separator);
    display: flex; justify-content: flex-end;
  }
</style>
