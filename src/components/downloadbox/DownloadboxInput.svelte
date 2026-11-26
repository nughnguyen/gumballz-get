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
  import { t } from "$lib/i18n";
  import { shortcut } from "$lib/platform";
  import type { HomeInputMode } from "$lib/home/downloadbox-controller";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";

  let {
    url = $bindable(""),
    onInput,
    prominent = false,
    advanced = false,
    onModeSelect,
    onAdvanced,
  }: {
    url?: string;
    onInput?: () => void;
    prominent?: boolean;
    advanced?: boolean;
    onModeSelect?: (mode: HomeInputMode) => void;
    onAdvanced?: () => void;
  } = $props();

  let dragOver = $state(false);
  let inputEl = $state<HTMLInputElement | null>(null);
  let menuOpen = $state(false);
  let menuWrapEl = $state<HTMLDivElement | null>(null);

  const secondary: {
    mode: HomeInputMode;
    key: "home.action_batch" | "home.action_torrent" | "home.action_p2p";
    icon: string;
  }[] = [
    { mode: "batch", key: "home.action_batch", icon: "M4 6h16M4 12h16M4 18h10" },
    { mode: "torrent", key: "home.action_torrent", icon: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5" },
    { mode: "p2p", key: "home.action_p2p", icon: "M22 2 11 13M22 2l-7 20-4-9-9-4z" },
  ];

  $effect(() => {
    if (!menuOpen) return;
    function handleDocPointerDown(e: PointerEvent) {
      if (menuWrapEl && !menuWrapEl.contains(e.target as Node)) {
        menuOpen = false;
      }
    }
    function handleDocKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        menuOpen = false;
      }
    }
    document.addEventListener("pointerdown", handleDocPointerDown);
    document.addEventListener("keydown", handleDocKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handleDocPointerDown);
      document.removeEventListener("keydown", handleDocKeyDown);
    };
  });

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = "copy";
    }
    dragOver = true;
  }

  function handleDragLeave() {
    dragOver = false;
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;

    if (e.dataTransfer?.files?.length) {
      const file = e.dataTransfer.files[0];
      if (file.name.endsWith(".torrent")) {
        url = (file as any).path || file.name;
        onInput?.();
        return;
      }
    }

    const text = e.dataTransfer?.getData("text/plain");
    if (text) {
      url = text.trim();
      onInput?.();
    }
  }

  function focusInput() {
    inputEl?.focus();
  }

  async function handlePasteClick() {
    const text =
      (await readText().catch(() => null)) ??
      (await navigator.clipboard?.readText().catch(() => null));
    if (text?.trim()) {
      url = text.trim();
      onInput?.();
      focusInput();
    } else {
      focusInput();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="downloadbox-wrapper"
  class:drag-over={dragOver}
  class:prominent
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
  onclick={(e) => {
    if (e.target === e.currentTarget) focusInput();
  }}
>
  <div class="downloadbox-plus-wrap" bind:this={menuWrapEl}>
    <button
      type="button"
      class="downloadbox-plus-btn"
      class:open={menuOpen}
      aria-label={$t('downloadbox.more_actions')}
      aria-expanded={menuOpen}
      aria-haspopup="menu"
      onclick={(e) => {
        e.stopPropagation();
        menuOpen = !menuOpen;
      }}
    >
      <svg viewBox="0 0 16 16" width={prominent ? 14 : 12} height={prominent ? 14 : 12} fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">
        <path d="M8 3.2v9.6M3.2 8h9.6" />
      </svg>
    </button>

    {#if menuOpen}
      <div class="downloadbox-plus-menu" role="menu">
        {#each secondary as item (item.mode)}
          <button
            type="button"
            class="downloadbox-plus-item"
            role="menuitem"
            onclick={(e) => {
              e.stopPropagation();
              menuOpen = false;
              onModeSelect?.(item.mode);
            }}
          >
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d={item.icon} />
            </svg>
            <span>{$t(item.key)}</span>
          </button>
        {/each}
        {#if onAdvanced}
          <div class="downloadbox-plus-sep" role="separator"></div>
          <button
            type="button"
            class="downloadbox-plus-item"
            role="menuitem"
            onclick={(e) => {
              e.stopPropagation();
              menuOpen = false;
              onAdvanced();
            }}
          >
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6" />
            </svg>
            <span>{$t(advanced ? "home.action_simple" : "home.action_advanced")}</span>
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <input
    bind:this={inputEl}
    class="downloadbox"
    type="text"
    placeholder={$t('downloadbox.placeholder')}
    aria-label={$t('downloadbox.placeholder')}
    bind:value={url}
    oninput={onInput}
    autocomplete="off"
    autocorrect="off"
    autocapitalize="off"
    spellcheck="false"
    enterkeyhint="go"
  />
  {#if url.length > 0}
    <button type="button" class="clear-btn" onclick={() => { url = ""; onInput?.(); focusInput(); }} aria-label={$t('common.clear')}>
      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true">
        <path d="M8 1.4a6.6 6.6 0 1 0 0 13.2A6.6 6.6 0 0 0 8 1.4zm2.35 3.5a.75.75 0 0 1 1.06 1.06L9.06 8l2.35 2.35a.75.75 0 1 1-1.06 1.06L8 9.06 5.65 11.4a.75.75 0 1 1-1.06-1.06L6.94 8 4.6 5.65A.75.75 0 0 1 5.65 4.6L8 6.94l2.35-2.04z" />
      </svg>
    </button>
  {:else}
    <button
      type="button"
      class="downloadbox-paste-btn"
      onclick={handlePasteClick}
      aria-label="{$t('tools.ig.download.paste')} ({shortcut('V')})"
    >
      <span class="downloadbox-paste-label">{$t('tools.ig.download.paste')}</span>
      <span class="downloadbox-paste-kbd">{shortcut("V")}</span>
    </button>
  {/if}
</div>

<style>
  .downloadbox-wrapper {
    width: 100%;
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-h-lg);
    padding: 0 var(--space-2);
    background: var(--control-bg);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    cursor: text;
    transition: box-shadow var(--duration-fast) var(--ease-out), background var(--duration-fast) var(--ease-out);
  }

  .downloadbox-wrapper.prominent {
    height: 52px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-xl);
    background: var(--surface);
    box-shadow:
      inset 0 0 0 var(--hairline) var(--content-border),
      0 10px 30px rgba(var(--shadow-ink), var(--elev-alpha-2)),
      0 2px 6px rgba(var(--shadow-ink), var(--elev-alpha-1));
  }

  .downloadbox-wrapper.prominent .downloadbox {
    font-size: var(--text-lg);
    font-weight: 400;
    letter-spacing: var(--track-snug);
  }

  .downloadbox-wrapper.drag-over {
    background: var(--accent-soft);
    box-shadow:
      inset 0 0 0 2px var(--accent),
      0 0 0 4px var(--accent-soft);
  }

  .downloadbox-wrapper:focus-within {
    box-shadow:
      inset 0 0 0 var(--hairline) var(--accent),
      0 0 0 3px var(--accent-soft);
  }

  .downloadbox-wrapper.prominent:focus-within {
    box-shadow:
      inset 0 0 0 var(--hairline) var(--accent),
      0 0 0 4px var(--accent-soft),
      0 10px 30px rgba(var(--shadow-ink), var(--elev-alpha-2));
  }

  .downloadbox-plus-wrap {
    position: relative;
    display: flex;
    align-items: center;
    height: 100%;
    flex-shrink: 0;
  }

  .downloadbox-plus-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-full);
    background: var(--surface-hi);
    color: var(--text);
    cursor: pointer;
    box-shadow: none;
    transition:
      background var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out),
      transform var(--duration-fast) var(--ease-out);
  }

  .downloadbox-wrapper.prominent .downloadbox-plus-btn {
    width: 32px;
    height: 32px;
  }

  .downloadbox-plus-btn.open {
    background: var(--fill-2);
    color: var(--text);
  }

  .downloadbox-plus-btn.open svg {
    transform: rotate(45deg);
  }

  .downloadbox-plus-btn svg {
    pointer-events: none;
    transition: transform var(--duration-fast) var(--ease-out);
  }

  @media (hover: hover) {
    .downloadbox-plus-btn:hover {
      background: var(--fill-2);
      color: var(--text);
    }
  }

  .downloadbox-plus-btn:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  .downloadbox-plus-menu {
    position: absolute;
    top: calc(100% + var(--space-2));
    inset-inline-start: 0;
    min-width: 220px;
    padding: var(--space-1);
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--surface);
    border-radius: var(--radius-lg);
    box-shadow:
      inset 0 0 0 var(--hairline) var(--content-border),
      0 12px 32px rgba(var(--shadow-ink), var(--elev-alpha-3, 0.35)),
      0 2px 8px rgba(var(--shadow-ink), var(--elev-alpha-1));
    z-index: 40;
  }

  .downloadbox-plus-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 32px;
    padding: 0 var(--space-3);
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text);
    font-size: var(--text-base);
    font-weight: 500;
    text-align: start;
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .downloadbox-plus-item svg {
    color: var(--text-muted);
    flex-shrink: 0;
    pointer-events: none;
  }

  @media (hover: hover) {
    .downloadbox-plus-item:hover {
      background: var(--fill-2);
    }

    .downloadbox-plus-item:hover svg {
      color: var(--text);
    }
  }

  .downloadbox-plus-item:focus-visible {
    outline: var(--focus-ring);
    outline-offset: -2px;
  }

  .downloadbox-plus-sep {
    height: var(--hairline);
    margin: var(--space-1) var(--space-2);
    background: var(--separator);
  }

  .downloadbox {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    font-size: var(--text-base);
    background: transparent;
    color: var(--text);
    border: none;
  }

  .downloadbox::placeholder {
    color: var(--text-dim);
  }

  .downloadbox:focus {
    outline: none;
  }

  .downloadbox-paste-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 var(--space-2);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-sm);
    background: var(--surface-hi);
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
    flex-shrink: 0;
    transition:
      background var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .downloadbox-wrapper.prominent .downloadbox-paste-btn {
    height: 32px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
  }

  .downloadbox-paste-label {
    color: var(--text);
  }

  .downloadbox-paste-kbd {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-dim);
  }

  @media (hover: hover) {
    .downloadbox-paste-btn:hover {
      background: var(--fill-2);
      color: var(--text);
    }
  }

  .downloadbox-paste-btn:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  .clear-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    background: transparent;
    border-radius: var(--radius-full);
    color: var(--text-dim);
    cursor: pointer;
    padding: 0;
    flex-shrink: 0;
  }

  .downloadbox-wrapper.prominent .clear-btn {
    width: 32px;
    height: 32px;
  }

  .clear-btn :global(svg) {
    pointer-events: none;
  }

  @media (hover: hover) {
    .clear-btn:hover {
      color: var(--text);
      background: var(--fill-2);
    }
  }

  .clear-btn:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }
</style>
