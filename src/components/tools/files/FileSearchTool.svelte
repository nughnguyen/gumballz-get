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
  /** Instant search (studies 26/27): Everything, Spotlight or fd/find. */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { baseName, dirName, errText, fmtBytes, openPath, pickDir, reveal } from "$lib/tools/rt";

  type Backend = { name: string; available: boolean; path: string | null; install_hint: string };
  type Hit = { path: string; size: number | null; is_dir: boolean };
  let backend = $state<Backend | null>(null);
  let query = $state("");
  let folder = $state("");
  let hits = $state<Hit[]>([]);
  let busy = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  onMount(async () => { backend = await invoke<Backend>("tool_file_search_backend"); });

  async function search() {
    if (!query.trim()) { hits = []; return; }
    busy = true;
    try { hits = await invoke<Hit[]>("tool_file_search", { query, folder: folder || null, limit: 300 }); }
    catch (e) { showToast("error", errText(e)); } finally { busy = false; }
  }
  function schedule() { if (timer) clearTimeout(timer); timer = setTimeout(search, 250); }
</script>

<div class="tool">
  <section>
    <div class="group">
      <div class="group-row">
        <div class="group-row-content"><input class="input input-search" type="search" bind:value={query} oninput={schedule} placeholder={$t("tools.fsearch.placeholder")} /></div>
        <div class="group-row-trailing btn-row">
          <button class="btn btn-secondary btn-sm" type="button" onclick={async () => { const d = await pickDir(); if (d) { folder = d; search(); } }}>{folder ? baseName(folder) : $t("tools.fsearch.everywhere")}</button>
          {#if folder}<button class="btn btn-ghost btn-sm" type="button" onclick={() => { folder = ""; search(); }}>×</button>{/if}
        </div>
      </div>
      <div class="group-row"><div class="group-row-sub">{#if backend}{backend.name} {#if backend.available}<span class="tag tag-success">ok</span>{:else}<span class="tag tag-warning">{$t("tools.common.not_installed")}</span> <span class="mono">{backend.install_hint}</span>{/if}{/if} · {hits.length} {$t("tools.fsearch.results")}{#if busy} …{/if}</div></div>
    </div>
  </section>
  {#if hits.length}
    <section><div class="group">
      {#each hits as h (h.path)}
        <div class="group-row">
          <div class="group-row-content"><div class="group-row-title">{h.is_dir ? "📁" : "📄"} {baseName(h.path)} {#if h.size !== null}<span class="dim">· {fmtBytes(h.size)}</span>{/if}</div><div class="group-row-sub mono">{dirName(h.path)}</div></div>
          <div class="group-row-trailing btn-row"><button class="btn btn-ghost btn-sm" type="button" onclick={() => openPath(h.path)}>{$t("tools.common.open")}</button><button class="btn btn-ghost btn-sm" type="button" onclick={() => reveal(h.path)}>{$t("tools.common.reveal")}</button></div>
        </div>
      {/each}
    </div></section>
  {/if}
</div>

<style>
  .tool { display: flex; flex-direction: column; gap: var(--space-5); }
  .dim { color: var(--text-dim); font-weight: 400; }
  .mono { font-family: var(--font-mono); font-size: var(--text-xs); overflow-wrap: anywhere; }
</style>
