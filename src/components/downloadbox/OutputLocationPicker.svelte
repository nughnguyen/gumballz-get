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
  import { open } from "@tauri-apps/plugin-dialog";
  import { t } from "$lib/i18n";
  import { getSettings, updateSettings } from "$lib/stores/settings-store.svelte";

  let {
    selectedOutputDir = $bindable<string>(""),
  } = $props();

  let settings = $derived(getSettings());
  let savedDirs = $derived(settings?.download.saved_output_dirs ?? []);
  let defaultDir = $derived(settings?.download.default_output_dir ?? "");

  let isDropdownOpen = $state(false);
  let mutating = $state(false);
  let pendingWrites = 0;
  let writeChain: Promise<void> = Promise.resolve();

  function enqueueSavedDirsUpdate(mutator: (dirs: string[]) => string[]) {
    pendingWrites += 1;
    mutating = true;
    const run = writeChain
      .then(async () => {
        const current = getSettings()?.download.saved_output_dirs ?? [];
        const next = mutator(current);
        if (next.length === current.length && next.every((dir, i) => dir === current[i])) {
          return;
        }
        await updateSettings({ download: { saved_output_dirs: next } });
      })
      .finally(() => {
        pendingWrites -= 1;
        if (pendingWrites === 0) mutating = false;
      });
    writeChain = run.catch(() => {});
    return run;
  }

  async function handleBrowse() {
    isDropdownOpen = false;
    const picked = await open({
      directory: true,
      title: $t("settings.download.choose_folder"),
    });
    if (!picked || typeof picked !== "string") return;

    selectedOutputDir = picked;

    if (picked === defaultDir) return;
    await enqueueSavedDirsUpdate((dirs) =>
      dirs.includes(picked) ? dirs : [...dirs, picked],
    );
  }

  function pickDir(path: string) {
    selectedOutputDir = path;
    isDropdownOpen = false;
  }

  async function removeDir(e: MouseEvent, path: string) {
    e.stopPropagation();
    if (selectedOutputDir === path) {
      selectedOutputDir = defaultDir;
    }
    await enqueueSavedDirsUpdate((dirs) => dirs.filter((d) => d !== path));
  }
</script>

<div class="location-picker">
  <button
    type="button"
    class="location-picker-trigger"
    onclick={() => { isDropdownOpen = !isDropdownOpen; }}
    aria-haspopup="listbox"
    aria-expanded={isDropdownOpen}
  >
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
    </svg>
    <span class="location-picker-label">{$t('settings.download.section_output')}:</span>
    <span class="location-picker-value" title={selectedOutputDir || defaultDir}>{selectedOutputDir || defaultDir}</span>
    <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <polyline points="6 9 12 15 18 9" />
    </svg>
  </button>

  {#if isDropdownOpen}
    <ul class="location-picker-menu" role="listbox">
      <li class="location-picker-item-wrapper">
        <button
          type="button"
          class="location-picker-option"
          class:active={selectedOutputDir === defaultDir || !selectedOutputDir}
          role="option"
          aria-selected={selectedOutputDir === defaultDir || !selectedOutputDir}
          onclick={() => pickDir(defaultDir)}
        >
          <span class="location-picker-option-alias">{$t('settings.download.default_output_dir')}</span>
          <span class="location-picker-option-meta" title={defaultDir}>{defaultDir}</span>
        </button>
      </li>

      {#each savedDirs as dir}
        <li class="location-picker-item-wrapper">
          <button
            type="button"
            class="location-picker-option"
            class:active={selectedOutputDir === dir}
            role="option"
            aria-selected={selectedOutputDir === dir}
            onclick={() => pickDir(dir)}
          >
            <span class="location-picker-option-meta" title={dir}>{dir}</span>
          </button>
          <button
            type="button"
            class="location-picker-remove"
            onclick={(e) => removeDir(e, dir)}
            disabled={mutating}
            aria-label={$t("settings.download.remove_saved_dir")}
            title={$t("settings.download.remove_saved_dir")}
          >
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </button>
        </li>
      {/each}

      <li class="location-picker-item-wrapper">
        <button
          type="button"
          class="location-picker-option browse-btn"
          role="option"
          aria-selected="false"
          disabled={mutating}
          onclick={handleBrowse}
        >
          <span class="location-picker-option-alias">{$t('settings.download.choose_folder')}...</span>
        </button>
      </li>
    </ul>
  {/if}
</div>

<style>
  .location-picker {
    position: relative;
    display: inline-flex;
    width: 100%;
    max-width: 100%;
  }

  .location-picker-trigger {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    font-size: var(--text-sm, 13px);
    font-weight: 500;
    color: var(--text, #fff);
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md, 8px);
    cursor: pointer;
    width: 100%;
    text-align: left;
    overflow: hidden;
    transition: background var(--duration-fast, 120ms) ease, box-shadow var(--duration-fast, 120ms) ease;
  }

  .location-picker-trigger:focus-visible {
    outline: var(--focus-ring, 2px solid var(--accent));
    outline-offset: var(--focus-ring-offset, 2px);
  }

  .location-picker-trigger:hover {
    background: var(--surface-hi);
    color: var(--text, #fff);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .location-picker-label {
    white-space: nowrap;
    color: var(--text-dim);
  }

  .location-picker-value {
    color: var(--text, #fff);
    font-weight: 600;
    flex-grow: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  .location-picker-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 30;
    margin: 0;
    padding: 6px;
    list-style: none;
    background: var(--surface-elevated, var(--surface, #1e1e24));
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md, 8px);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    max-height: 250px;
    overflow-y: auto;
  }

  .location-picker-item-wrapper {
    display: flex;
    align-items: stretch;
    width: 100%;
  }

  .location-picker-option {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 2px;
    flex-grow: 1;
    padding: 8px 10px;
    font-size: var(--text-sm, 13px);
    color: var(--text, #fff);
    background: transparent;
    border: none;
    border-radius: calc(var(--radius-md, 8px) - 2px);
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    transition: background var(--duration-fast, 120ms) ease;
  }

  .location-picker-remove {
    background: transparent;
    border: none;
    color: var(--text-dim, rgba(255, 255, 255, 0.5));
    cursor: pointer;
    padding: 0 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: calc(var(--radius-md, 8px) - 2px);
    transition: color var(--duration-fast, 120ms) ease;
  }

  .location-picker-remove:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .location-picker-remove:hover {
    color: var(--danger, #ff4d4f);
  }

  .location-picker-option:hover {
    background: var(--surface-hi);
  }

  .location-picker-option.active {
    background: var(--accent);
    color: var(--on-accent);
  }

  .location-picker-option.active .location-picker-option-alias,
  .location-picker-option.active .location-picker-option-meta {
    color: var(--on-accent);
  }

  .location-picker-option-alias {
    color: var(--text, #fff);
    font-weight: 600;
  }

  .location-picker-option-meta {
    color: var(--text-dim, rgba(255, 255, 255, 0.6));
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    width: 100%;
    direction: rtl;
    text-align: left;
  }

  .browse-btn {
    align-items: center;
    flex-direction: row;
    color: var(--accent-text);
  }

  .browse-btn .location-picker-option-alias {
    color: var(--accent-text);
  }
</style>
