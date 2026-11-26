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

  let {
    selectedQuality = $bindable("best"),
    selectedFormatId,
    availableHeights = null as number[] | null,
    hasAudioOnly = false,
  } = $props();

  // Full ordered list of resolution options (label, yt-dlp quality value, height for filtering)
  const RESOLUTION_OPTIONS = [
    { value: "best", key: "downloadbox.quality_best", height: Infinity },
    { value: "2160p", key: "downloadbox.quality_2160p", height: 2160 },
    { value: "1440p", key: "downloadbox.quality_1440p", height: 1440 },
    { value: "1080p", key: "downloadbox.quality_1080p", height: 1080 },
    { value: "720p", key: "downloadbox.quality_720p", height: 720 },
    { value: "480p", key: "downloadbox.quality_480p", height: 480 },
    { value: "360p", key: "downloadbox.quality_360p", height: 360 },
  ];

  let visibleOptions = $derived.by(() => {
    if (!availableHeights) {
      return RESOLUTION_OPTIONS;
    }
    return RESOLUTION_OPTIONS.filter(o => {
      if (o.height === Infinity || o.value === selectedQuality) return true;
      return availableHeights!.some(h => h === o.height);
    });
  });

  // i18n keys that may not exist yet — provide a fallback label
  function qualityLabel(value: string, key: string): string {
    const translated = $t(key);
    // If no translation key exists (e.g. 2160p, 1440p), build a label from value
    if (translated === key) return value;
    return translated;
  }
</script>

{#if !selectedFormatId}
  <div class="quality-group">
    <span class="quality-label">{$t('downloadbox.quality')}</span>
    <div class="quality-pills">
      {#each visibleOptions as q (q.value)}
        <button
          class="quality-pill"
          class:active={selectedQuality === q.value}
          onclick={() => { selectedQuality = q.value; }}
        >
          {qualityLabel(q.value, q.key)}
        </button>
      {/each}
      {#if hasAudioOnly}
        <button
          class="quality-pill quality-pill--audio"
          class:active={selectedQuality === "audio"}
          onclick={() => { selectedQuality = "audio"; }}
        >
          {$t('downloadbox.quality_audio')}
        </button>
      {/if}
    </div>
    {#if availableHeights}
      <span class="quality-hint">{$t('downloadbox.quality_from_formats')}</span>
    {/if}
  </div>
{/if}

<style>
  .quality-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .quality-label {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-dim);
  }

  .quality-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .quality-pill {
    height: 26px;
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text);
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-full);
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out), transform var(--duration-fast) var(--ease-out);
  }

  .quality-pill.active {
    background: var(--accent);
    color: var(--on-accent);
    border: 2px solid var(--border-ink);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  @media (hover: hover) {
    .quality-pill:not(.active):hover {
      background: var(--surface-hi);
      color: var(--text);
      box-shadow: 2px 2px 0 var(--shadow-hard);
    }
  }

  .quality-pill:active {
    transform: translate(2px, 2px);
    box-shadow: none;
  }

  .quality-pill:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  .quality-hint {
    font-size: var(--text-sm);
    color: var(--text-dim);
  }
</style>
