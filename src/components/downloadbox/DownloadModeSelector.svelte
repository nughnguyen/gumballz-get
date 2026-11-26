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

  let { downloadMode = $bindable("auto" as "auto" | "audio" | "mute"), onChange = undefined } = $props();
</script>

<div class="mode-group">
  <button
    class="mode-btn"
    class:active={downloadMode === "auto"}
    onclick={() => { downloadMode = "auto"; onChange?.(); }}
  >
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <rect x="2" y="6" width="14" height="12" rx="2" />
      <path d="m22 8-6 4 6 4V8Z" />
    </svg>
    {$t('downloadbox.mode_auto')}
  </button>
  <button
    class="mode-btn"
    class:active={downloadMode === "audio"}
    onclick={() => { downloadMode = "audio"; onChange?.(); }}
  >
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M9 18V5l12-2v13" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="18" cy="16" r="3" />
    </svg>
    {$t('downloadbox.mode_audio')}
  </button>
  <button
    class="mode-btn"
    class:active={downloadMode === "mute"}
    onclick={() => { downloadMode = "mute"; onChange?.(); }}
  >
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M11 5L6 9H2v6h4l5 4V5z" />
      <line x1="23" y1="9" x2="17" y2="15" />
      <line x1="17" y1="9" x2="23" y2="15" />
    </svg>
    {$t('downloadbox.mode_mute')}
  </button>
</div>

<style>
  .mode-group {
    display: flex;
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    padding: 4px;
    gap: 4px;
  }

  .mode-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 26px;
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text);
    background: transparent;
    border: 2px solid transparent;
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out);
  }

  .mode-btn.active {
    background: var(--accent);
    color: var(--on-accent);
    border: 2px solid var(--border-ink);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  @media (hover: hover) {
    .mode-btn:not(.active):hover {
      color: var(--text);
      background: var(--surface);
    }
  }

  .mode-btn:focus-visible {
    outline: var(--focus-ring);
    outline-offset: -1px;
  }

  .mode-btn svg {
    flex-shrink: 0;
    pointer-events: none;
  }
</style>
