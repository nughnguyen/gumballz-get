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
  /** Send to the phone via KDE Connect (study 30). */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { errText, pickFile } from "$lib/tools/rt";

  type Device = { id: string; name: string; reachable: boolean; paired: boolean };
  type Status = { installed: boolean; path: string | null; devices: Device[]; install_hint: string };
  let status = $state<Status | null>(null);
  let device = $state("");
  let kind = $state<"file" | "url" | "text">("url");
  let value = $state("");
  let busy = $state(false);

  async function refresh() {
    try { status = await invoke<Status>("tool_kde_status"); if (!device) device = status.devices.find((d) => d.reachable)?.id ?? status.devices[0]?.id ?? ""; }
    catch (e) { showToast("error", errText(e)); }
  }
  onMount(refresh);

  async function send() {
    if (!device || !value.trim() || busy) return;
    busy = true;
    try { await invoke("tool_kde_share", { device, kind, value }); showToast("success", $t("tools.kde.sent") as string); if (kind !== "file") value = ""; }
    catch (e) { showToast("error", errText(e)); } finally { busy = false; }
  }
  async function ping() {
    try { await invoke("tool_kde_ping", { device, message: "GumballZ Get" }); showToast("success", "ping"); } catch (e) { showToast("error", errText(e)); }
  }
</script>

<div class="tool">
  <section>
    <span class="group-label">KDE Connect</span>
    <div class="group">
      <div class="group-row">
        <div class="group-row-content"><div class="group-row-title">{$t("tools.kde.daemon")}</div><div class="group-row-sub">{#if !status}…{:else if status.installed}<span class="mono">{status.path}</span>{:else}{$t("tools.common.not_installed")} · <span class="mono">{status.install_hint}</span>{/if}</div></div>
        <div class="group-row-trailing btn-row"><button class="btn btn-secondary btn-sm" type="button" onclick={async () => { try { await invoke("tool_kde_refresh"); } catch {} await refresh(); }}>{$t("tools.common.refresh")}</button></div>
      </div>
      {#if status?.installed}
        <div class="group-row">
          <div class="group-row-content"><div class="group-row-title">{$t("tools.kde.device")}</div><div class="group-row-sub">{status.devices.length === 0 ? $t("tools.kde.no_devices") : ""}</div></div>
          <div class="group-row-trailing btn-row">
            <select class="input" bind:value={device}>{#each status.devices as d (d.id)}<option value={d.id}>{d.name}{d.reachable ? "" : " (offline)"}</option>{/each}</select>
            <button class="btn btn-ghost btn-sm" type="button" disabled={!device} onclick={ping}>ping</button>
          </div>
        </div>
      {/if}
    </div>
  </section>
  {#if status?.installed}
    <section>
      <span class="group-label">{$t("tools.kde.send")}</span>
      <div class="group">
        <div class="group-row">
          <div class="group-row-content"><div class="segmented"><button class="segmented-btn" class:active={kind === "url"} type="button" onclick={() => (kind = "url")}>URL</button><button class="segmented-btn" class:active={kind === "text"} type="button" onclick={() => (kind = "text")}>{$t("tools.kde.text")}</button><button class="segmented-btn" class:active={kind === "file"} type="button" onclick={() => (kind = "file")}>{$t("tools.common.file")}</button></div></div>
        </div>
        <div class="group-row">
          <div class="group-row-content">
            {#if kind === "file"}<div class="group-row-sub mono">{value || $t("tools.kde.pick_file")}</div>
            {:else if kind === "text"}<textarea class="input" rows="3" bind:value={value}></textarea>
            {:else}<input class="input" type="url" bind:value={value} placeholder="https://…" onkeydown={(e) => e.key === "Enter" && send()} />{/if}
          </div>
          <div class="group-row-trailing btn-row">
            {#if kind === "file"}<button class="btn btn-secondary btn-sm" type="button" onclick={async () => { const f = await pickFile(); if (f) value = f; }}>{$t("tools.common.choose")}</button>{/if}
            <button class="btn btn-primary btn-sm" type="button" disabled={busy || !device || !value.trim()} onclick={send}>{busy ? $t("tools.common.working") : $t("tools.kde.send")}</button>
          </div>
        </div>
      </div>
    </section>
  {/if}
</div>

<style>
  .tool { display: flex; flex-direction: column; gap: var(--space-5); }
  .mono { font-family: var(--font-mono); font-size: var(--text-xs); overflow-wrap: anywhere; }
  .segmented-btn.active { background: var(--surface-hi); color: var(--text); }
  textarea.input { width: 100%; resize: vertical; font-family: inherit; }
</style>
