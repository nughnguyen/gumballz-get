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
  import type { PreviewQuality } from "$lib/stores/media-preview-store.svelte";

  let {
    qualities,
    onDownload,
  }: {
    qualities: PreviewQuality[];
    /** label of the video version to download ("video_wm" / "video_no_wm"). */
    onDownload: (label: string) => void;
  } = $props();

  // Only the versions TikTok actually serves for this video are offered:
  // playAddr (watermarked) and/or downloadAddr (clean). Never offer a
  // choice that cannot be fulfilled.
  let wm = $derived(qualities.find((q) => q.label === "video_wm"));
  let noWm = $derived(qualities.find((q) => q.label === "video_no_wm"));
</script>

{#if wm || noWm}
  <div class="watermark-panel">
    <div class="watermark-choices">
      {#if wm}
        <button
          type="button"
          class="button watermark-btn"
          onclick={() => onDownload("video_wm")}
        >
          {$t("downloadbox.video_with_watermark")}
        </button>
      {/if}
      {#if noWm}
        <button
          type="button"
          class="button watermark-btn"
          onclick={() => onDownload("video_no_wm")}
        >
          {$t("downloadbox.video_no_watermark")}
        </button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .watermark-panel {
    width: 100%;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    padding: var(--padding);
  }

  .watermark-choices {
    display: flex;
    flex-wrap: wrap;
    gap: calc(var(--padding) / 2);
  }

  .watermark-btn {
    flex: 1 1 auto;
    padding: 10px 18px;
    font-size: var(--text-sm);
    font-weight: 700;
  }
</style>
