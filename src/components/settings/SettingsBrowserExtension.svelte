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
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "$lib/i18n";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { BUILD_INFO } from "$lib/build-info";

  type BridgeInfo = {
    enabled: boolean;
    port: number;
    token: string;
    url: string;
  };

  let info = $state<BridgeInfo | null>(null);
  let revealed = $state(false);
  let rotating = $state(false);
  let loading = $state(true);

  async function refresh() {
    try {
      info = await invoke<BridgeInfo>("get_bridge_info");
    } catch (e: any) {
      info = null;
      showToast("error", typeof e === "string" ? e : (e.message ?? String(e)));
    } finally {
      loading = false;
    }
  }

  async function copy(value: string, kind: "url" | "token") {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      showToast(
        "success",
        kind === "url"
          ? ($t("settings.bridge.copied_url") as string)
          : ($t("settings.bridge.copied_token") as string)
      );
    } catch (e: any) {
      showToast("error", typeof e === "string" ? e : (e.message ?? String(e)));
    }
  }

  async function rotate() {
    rotating = true;
    try {
      info = await invoke<BridgeInfo>("rotate_bridge_token");
      showToast("success", $t("settings.bridge.rotated") as string);
    } catch (e: any) {
      showToast("error", typeof e === "string" ? e : (e.message ?? String(e)));
    } finally {
      rotating = false;
    }
  }

  let pairing = $state(false);
  let pairSecondsLeft = $state(0);
  let pairResult = $state<"none" | "success" | "expired">("none");
  let pairTimer: ReturnType<typeof setInterval> | null = null;
  let unlistenPaired: (() => void) | undefined;

  function stopPairTimer() {
    if (pairTimer) clearInterval(pairTimer);
    pairTimer = null;
  }

  async function startPairing() {
    try {
      const res = await invoke<{ ok: boolean; window_secs: number; enabled: boolean }>(
        "bridge_open_pairing"
      );
      if (!res.ok || !res.enabled) {
        showToast("error", $t("settings.bridge.pair_unavailable") as string);
        return;
      }
      pairing = true;
      pairResult = "none";
      pairSecondsLeft = res.window_secs;
      stopPairTimer();
      pairTimer = setInterval(() => {
        pairSecondsLeft -= 1;
        if (pairSecondsLeft <= 0) {
          pairing = false;
          pairResult = "expired";
          stopPairTimer();
        }
      }, 1000);
    } catch (e: any) {
      showToast("error", typeof e === "string" ? e : (e.message ?? String(e)));
    }
  }

  onMount(() => {
    // The backend emits `bridge-paired` the moment the extension fetches the
    // token from GET /v1/pair, so we can flip the countdown into a success
    // state instead of letting it silently expire.
    listen("bridge-paired", () => {
      if (!pairing) return;
      pairing = false;
      pairResult = "success";
      stopPairTimer();
      showToast("success", $t("settings.bridge.pair_success") as string);
    }).then((fn) => {
      unlistenPaired = fn;
    });
  });

  onDestroy(() => {
    stopPairTimer();
    unlistenPaired?.();
  });

  function maskedToken(value: string): string {
    if (!value) return "";
    if (value.length <= 8) return "•".repeat(value.length);
    return value.slice(0, 4) + "…" + value.slice(-4);
  }

  onMount(refresh);
</script>

<section class="section">
  <h5 class="section-title">{$t("settings.bridge.title")}</h5>
  <div class="card">
    <p class="bridge-intro">{$t("settings.bridge.intro")}</p>

    {#if loading}
      <div class="setting-row">
        <span class="setting-label">{$t("common.loading")}</span>
      </div>
    {:else if !info || !info.enabled || info.port === 0 || !info.token}
      <div class="setting-row">
        <span class="setting-label">{$t("settings.bridge.unavailable")}</span>
      </div>
    {:else}
      <div class="setting-row">
        <div class="setting-col">
          <span class="setting-label">{$t("settings.bridge.pair_label")}</span>
          {#if pairing}
            <span class="setting-hint">
              {$t("settings.bridge.pair_active", { secs: pairSecondsLeft })}
            </span>
          {:else if pairResult === "success"}
            <span class="setting-hint pair-success" role="status">
              {$t("settings.bridge.pair_success")}
            </span>
          {:else if pairResult === "expired"}
            <span class="setting-hint pair-expired" role="status">
              {$t("settings.bridge.pair_expired")}
            </span>
          {:else}
            <span class="setting-hint">{$t("settings.bridge.pair_hint")}</span>
          {/if}
        </div>
        <button
          class="bridge-action primary"
          type="button"
          disabled={pairing}
          onclick={startPairing}
        >
          {pairing ? $t("settings.bridge.pair_waiting") : $t("settings.bridge.pair")}
        </button>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-col">
          <span class="setting-label">{$t("settings.bridge.endpoint_label")}</span>
          <span class="setting-hint">{info.url}</span>
        </div>
        <button class="bridge-action" type="button" onclick={() => copy(info!.url, "url")}>
          {$t("settings.bridge.copy")}
        </button>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-col">
          <span class="setting-label">{$t("settings.bridge.token_label")}</span>
          <span class="setting-hint token-display">
            {revealed ? info.token : maskedToken(info.token)}
          </span>
        </div>
        <div class="bridge-actions">
          <button
            class="bridge-action"
            type="button"
            onclick={() => (revealed = !revealed)}
          >
            {revealed ? $t("settings.bridge.hide") : $t("settings.bridge.reveal")}
          </button>
          <button
            class="bridge-action"
            type="button"
            onclick={() => copy(info!.token, "token")}
          >
            {$t("settings.bridge.copy")}
          </button>
        </div>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-col">
          <span class="setting-label">{$t("settings.bridge.rotate_label")}</span>
          <span class="setting-hint">{$t("settings.bridge.rotate_hint")}</span>
        </div>
        <button
          class="bridge-action danger"
          type="button"
          disabled={rotating}
          onclick={rotate}
        >
          {rotating ? $t("settings.bridge.rotating") : $t("settings.bridge.rotate")}
        </button>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-col">
          <span class="setting-label">Official repository</span>
          <span class="setting-hint">Download release zips (v{BUILD_INFO.version}), report issues, or inspect source code.</span>
        </div>
        <a
          class="bridge-action"
          href="https://github.com/nughnguyen/gumballz-get"
          target="_blank"
          rel="noreferrer"
        >
          GitHub
        </a>
      </div>
    {/if}
  </div>
</section>

<style>
  .bridge-intro {
    margin: 0 0 12px;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.5;
  }
  .pair-success {
    color: var(--text);
    font-weight: 600;
  }
  .pair-success::before {
    content: "✓ ";
  }
  .pair-expired {
    color: var(--warning);
    font-weight: 600;
  }
  .token-display {
    font-family: var(--font-mono, "IBM Plex Mono", monospace);
    word-break: break-all;
  }
  .bridge-actions {
    display: inline-flex;
    gap: 8px;
  }
  .bridge-action {
    appearance: none;
    border: 2px solid var(--border-ink);
    background: var(--surface);
    color: var(--text);
    padding: 6px 12px;
    border-radius: 8px;
    font: inherit;
    cursor: pointer;
    text-decoration: none;
    display: inline-flex;
    align-items: center;
  }
  .bridge-action:hover { box-shadow: 2px 2px 0 var(--shadow-hard); }
  .bridge-action:active:not(:disabled) { transform: translate(2px, 2px); box-shadow: none; }
  .bridge-action:disabled { opacity: 0.85; cursor: not-allowed; }
  .bridge-action.primary {
    background: var(--accent);
    color: var(--on-accent);
    border: 2px solid var(--border-ink);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }
  .bridge-action.primary:hover { background: var(--accent-lo, var(--accent)); }
  .bridge-action.danger { color: var(--danger); border: 2px solid var(--danger); }
  .bridge-action.danger:hover { background: var(--danger); color: var(--on-danger); }
</style>
