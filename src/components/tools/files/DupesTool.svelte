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
  /** Duplicates by hash (study 38): pick folders, review groups, delete copies. */
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { errText, fmtBytes, onToolProgress, pct, pickDir, reveal, type ToolProgress } from "$lib/tools/rt";

  type DupeFile = { path: string; modified: string | null };
  type Group = { size: number; hash: string; files: DupeFile[] };
  type Result = { scanned: number; groups: Group[]; wasted_bytes: number };

  let dirs = $state<string[]>([]);
  let minMb = $state(0.1);
  let exts = $state("");
  let busy = $state(false);
  let progress = $state<ToolProgress | null>(null);
  let result = $state<Result | null>(null);
  let selected = $state<Set<string>>(new Set());
  let unlisten: (() => void) | null = null;

  onMount(async () => { unlisten = await onToolProgress((p) => { if (p.id === "dupes") progress = p; }); });
  onDestroy(() => unlisten?.());

  async function scan() {
    if (!dirs.length || busy) return;
    busy = true; result = null; selected = new Set(); progress = null;
    try {
      result = await invoke<Result>("tool_dupes_scan", { opts: { dirs, min_size: Math.round(minMb * 1024 * 1024), extensions: exts.split(",").map((s) => s.trim()).filter(Boolean), skip_hidden: true } });
    } catch (e) { showToast("error", errText(e)); } finally { busy = false; }
  }

  function toggle(p: string) {
    const s = new Set(selected);
    if (s.has(p)) s.delete(p); else s.add(p);
    selected = s;
  }

  /** Marks all but the oldest in each group. */
  function autoSelect() {
    const s = new Set<string>();
    for (const g of result?.groups ?? []) {
      const sorted = [...g.files].sort((a, b) => (a.modified ?? "").localeCompare(b.modified ?? ""));
      sorted.slice(1).forEach((f) => s.add(f.path));
    }
    selected = s;
  }

  async function del() {
    if (!selected.size) return;
    if (!confirm(`${$t("tools.dupes.delete_confirm")} (${selected.size})`)) return;
    try {
      const r = await invoke<{ deleted: string[]; failed: string[] }>("tool_dupes_delete", { paths: [...selected] });
      showToast(r.failed.length ? "info" : "success", `${r.deleted.length} ${$t("tools.dupes.deleted")}`);
      await scan();
    } catch (e) { showToast("error", errText(e)); }
  }

  let selectedBytes = $derived(result ? result.groups.reduce((n, g) => n + g.files.filter((f) => selected.has(f.path)).length * g.size, 0) : 0);
</script>

<div class="tool">
  <section>
    <div class="group">
      <div class="group-row">
        <div class="group-row-content"><div class="group-row-title">{$t("tools.dupes.folders")}</div><div class="group-row-sub mono">{dirs.join(" · ") || $t("tools.dupes.folders_hint")}</div></div>
        <div class="group-row-trailing btn-row">{#if dirs.length}<button class="btn btn-ghost btn-sm" type="button" onclick={() => (dirs = [])}>×</button>{/if}<button class="btn btn-secondary btn-sm" type="button" onclick={async () => { const d = await pickDir(); if (d && !dirs.includes(d)) dirs = [...dirs, d]; }}>{$t("tools.common.add")}</button></div>
      </div>
      <div class="group-row">
        <div class="group-row-content"><div class="group-row-title">{$t("tools.dupes.min_size")}</div></div>
        <div class="group-row-trailing btn-row"><input class="input" type="number" min="0" step="0.1" bind:value={minMb} style:width="6em" /> MB</div>
      </div>
      <div class="group-row">
        <div class="group-row-content"><div class="group-row-title">{$t("tools.dupes.extensions")}</div><div class="group-row-sub">{$t("tools.common.optional")}</div></div>
        <div class="group-row-trailing"><input class="input" type="text" bind:value={exts} placeholder="mp4, mkv, pdf" /></div>
      </div>
      <div class="group-row">
        <div class="group-row-content">{#if busy}<div class="group-row-sub">{progress ? `${progress.done}/${progress.total ?? "?"}` : $t("tools.dupes.walking")}</div><div class="progress"><div class="progress-fill" style:width="{pct(progress) ?? 0}%"></div></div>{/if}</div>
        <div class="group-row-trailing"><button class="btn btn-primary" type="button" disabled={busy || !dirs.length} onclick={scan}>{busy ? $t("tools.common.working") : $t("tools.dupes.scan")}</button></div>
      </div>
    </div>
  </section>
  {#if result}
    <section>
      <span class="group-label">{result.groups.length} {$t("tools.dupes.groups")} · {fmtBytes(result.wasted_bytes)} {$t("tools.dupes.wasted")} · {result.scanned} {$t("tools.dupes.scanned")}</span>
      <div class="group">
        <div class="group-row">
          <div class="group-row-content"><div class="group-row-sub">{selected.size} {$t("tools.dupes.selected")} · {fmtBytes(selectedBytes)}</div></div>
          <div class="group-row-trailing btn-row">
            <button class="btn btn-secondary btn-sm" type="button" disabled={!result.groups.length} onclick={autoSelect}>{$t("tools.dupes.auto_select")}</button>
            <button class="btn btn-destructive btn-sm" type="button" disabled={!selected.size} onclick={del}>{$t("tools.dupes.delete")}</button>
          </div>
        </div>
        {#each result.groups.slice(0, 300) as g (g.hash)}
          <div class="group-row dupe-group">
            <div class="group-row-content">
              <div class="group-row-title">{fmtBytes(g.size)} × {g.files.length}</div>
              {#each g.files as f (f.path)}
                <label class="dupe-file"><input class="checkbox" type="checkbox" checked={selected.has(f.path)} onchange={() => toggle(f.path)} /><span class="mono">{f.path}</span><button class="btn btn-ghost btn-sm" type="button" onclick={() => reveal(f.path)}>↗</button></label>
              {/each}
            </div>
          </div>
        {/each}
        {#if result.groups.length === 0}<div class="group-row"><div class="group-row-sub">{$t("tools.dupes.none")}</div></div>{/if}
      </div>
    </section>
  {/if}
</div>

<style>
  .tool { display: flex; flex-direction: column; gap: var(--space-5); }
  .mono { font-family: var(--font-mono); font-size: var(--text-xs); overflow-wrap: anywhere; }
  .dupe-file { display: flex; align-items: center; gap: var(--space-2); padding: 2px 0; }
  .dupe-file .mono { flex: 1; }
</style>
